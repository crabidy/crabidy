use flume::Sender;
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::probe::Hint;
use tracing::warn;
use url::Url;

use crate::decoder::{MediaInfo, SymphoniaDecoder};
use anyhow::{anyhow, Result};
use rodio::{OutputStream, Sink, Source};
use stream_download::StreamDownload;
use symphonia::core::io::{
    MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource,
};
use thiserror::Error;

pub enum PlayerEngineCommand {
    Play(String, Sender<Result<MediaInfo>>),
    SetVolume(f32, Sender<Result<f32>>),
    Pause(Sender<Result<()>>),
    Unpause(Sender<Result<()>>),
    TogglePlay(Sender<Result<bool>>),
    Restart(Sender<Result<MediaInfo>>),
    Stop(Sender<Result<()>>),
    GetDuration(Sender<Result<Duration>>),
    GetElapsed(Sender<Result<Duration>>),
    GetVolume(Sender<Result<f32>>),
    GetPaused(Sender<Result<bool>>),
    Eos,
    SetElapsed(Duration),
}

pub enum PlayerMessage {
    Duration {
        duration: Duration,
    },
    Elapsed {
        duration: Duration,
        elapsed: Duration,
    },
    Stopped,
    Paused,
    Playing,
    EndOfStream,
}

// TODO:
// * Emit buffering

pub struct PlayerEngine {
    elapsed: Duration,
    // FIXME: We only need this to re-start a track
    // Might do that using seeking in the future
    current_source: Option<String>,
    media_info: Option<MediaInfo>,
    sink: Option<Sink>,
    stream: Option<OutputStream>,
    tx_engine: Sender<PlayerEngineCommand>,
    tx_player: Sender<PlayerMessage>,
}

#[derive(Debug, Error)]
pub enum PlayerEngineError {
    #[error("Sink is not playing")]
    NotPlaying,
}

impl PlayerEngine {
    pub fn new(tx_engine: Sender<PlayerEngineCommand>, tx_player: Sender<PlayerMessage>) -> Self {
        Self {
            current_source: None,
            media_info: None,
            elapsed: Duration::default(),
            sink: None,
            stream: None,
            tx_engine,
            tx_player,
        }
    }

    pub fn play(&mut self, source_str: &str) -> Result<MediaInfo> {
        let tx_player = self.tx_player.clone();
        let tx_engine = self.tx_engine.clone();

        let (stream, handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&handle)?;
        let (source, hint) = self.get_source(source_str)?;
        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
        let decoder = SymphoniaDecoder::new(mss, hint, self.tx_engine.clone())?;

        let media_info = decoder.media_info();
        let media_info_copy = media_info.clone();
        let duration = media_info.duration.unwrap_or_default();

        self.media_info = Some(media_info);

        tx_player
            .send(PlayerMessage::Duration { duration })
            .unwrap_or_else(|e| warn!("Send error {}", e));

        // FIXME: regularly update metadata revision
        let decoder = decoder.periodic_access(Duration::from_millis(250), move |src| {
            let elapsed = src.elapsed();
            tx_engine
                .send(PlayerEngineCommand::SetElapsed(elapsed))
                .unwrap_or_else(|e| warn!("Send error {}", e));
            tx_player
                .send(PlayerMessage::Elapsed { elapsed, duration })
                .unwrap_or_else(|e| warn!("Send error {}", e));
        });
        sink.append(decoder);

        // We need to keep the stream around, otherwise it gets dropped outside of this scope
        self.stream = Some(stream);
        // The sink is used to control the stream
        self.sink = Some(sink);

        self.tx_player
            .send(PlayerMessage::Playing)
            .unwrap_or_else(|e| warn!("Send error {}", e));

        Ok(media_info_copy)
    }

    pub fn restart(&mut self) -> Result<MediaInfo> {
        if let Some(source) = self.current_source.clone() {
            self.reset()?;
            return self.play(&source);
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn pause(&mut self) -> Result<()> {
        if let Some(sink) = &self.sink {
            sink.pause();
            self.tx_player
                .send(PlayerMessage::Paused)
                .unwrap_or_else(|e| warn!("Send error {}", e));
            return Ok(());
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn unpause(&mut self) -> Result<()> {
        if let Some(sink) = &self.sink {
            sink.play();
            self.tx_player
                .send(PlayerMessage::Playing)
                .unwrap_or_else(|e| warn!("Send error {}", e));
            return Ok(());
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn toggle_play(&mut self) -> Result<bool> {
        if let Some(sink) = &self.sink {
            if sink.is_paused() {
                sink.play();
                return Ok(true);
            } else {
                sink.pause();
                return Ok(false);
            }
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.reset()?;
        self.tx_player
            .send(PlayerMessage::Stopped)
            .unwrap_or_else(|e| warn!("Send error {}", e));
        Ok(())
    }

    pub fn is_paused(&self) -> Result<bool> {
        self.sink
            .as_ref()
            .map_or(Err(PlayerEngineError::NotPlaying.into()), |s| {
                Ok(s.is_paused())
            })
    }

    pub fn is_stopped(&self) -> bool {
        self.sink.is_none()
    }

    pub fn duration(&self) -> Result<Duration> {
        self.media_info
            .as_ref()
            .map_or(Err(PlayerEngineError::NotPlaying.into()), |m| {
                Ok(m.duration.unwrap_or_default())
            })
    }

    pub fn elapsed(&self) -> Result<Duration> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        Ok(self.elapsed)
    }

    pub fn volume(&self) -> Result<f32> {
        self.sink.as_ref().map_or(
            Err(PlayerEngineError::NotPlaying.into()),
            |s| Ok(s.volume()),
        )
    }

    pub fn set_volume(&mut self, volume: f32) -> Result<f32> {
        if let Some(sink) = &self.sink {
            sink.set_volume(volume);
            return Ok(sink.volume());
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn handle_eos(&mut self) {
        self.reset().unwrap_or_else(|e| {
            warn!("Sink error {}", e);
        });
        self.tx_player
            .send(PlayerMessage::EndOfStream)
            .unwrap_or_else(|e| warn!("Send error {}", e));
    }

    pub fn handle_elapsed(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;
    }

    fn reset(&mut self) -> Result<()> {
        self.elapsed = Duration::default();
        self.current_source = None;
        if let Some(sink) = &self.sink {
            sink.stop();
            self.sink.take();
            self.stream.take();
            return Ok(());
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    fn get_source(&self, source_str: &str) -> Result<(Box<dyn MediaSource>, Hint)> {
        match Url::parse(source_str) {
            Ok(url) => {
                if let "http" | "https" = url.scheme() {
                    let reader = StreamDownload::new_http(source_str.parse().unwrap());
                    let path = Path::new(url.path());
                    let hint = self.get_hint(path);

                    Ok((Box::new(ReadOnlySource::new(reader)), hint))
                } else {
                    Err(anyhow!("Not a valid URL scheme: {}", url.scheme()))
                }
            }
            Err(_) => {
                let path = Path::new(source_str);
                let hint = self.get_hint(path);
                Ok((Box::new(File::open(path)?), hint))
            }
        }
    }

    fn get_hint(&self, path: &Path) -> Hint {
        // Create a hint to help the format registry guess what format reader is appropriate.
        let mut hint = Hint::new();
        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }
        hint
    }
}
