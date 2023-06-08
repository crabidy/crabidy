use flume::Sender;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use std::{fs::File, sync::atomic::Ordering};
use symphonia::core::probe::Hint;
use tracing::warn;
use url::Url;

use crate::decoder::{MediaInfo, SymphoniaDecoder};
use anyhow::{anyhow, Result};
use rodio::{OutputStream, Sink, Source};
use stream_download::StreamDownload;
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use thiserror::Error;

pub enum PlayerEngineCommand {
    Play(String, Sender<Result<MediaInfo>>),
    SetVolume(f32, Sender<f32>),
    Pause(Sender<Result<()>>),
    Unpause(Sender<Result<()>>),
    TogglePlay(Sender<Result<bool>>),
    Restart(Sender<Result<MediaInfo>>),
    Stop(Sender<Result<()>>),
    GetDuration(Sender<Result<Duration>>),
    GetElapsed(Sender<Result<Duration>>),
    SeekTo(Duration, Sender<Result<Duration>>),
    GetVolume(Sender<f32>),
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

#[derive(Debug, Error)]
pub enum PlayerEngineError {
    #[error("Sink is not playing")]
    NotPlaying,
}

// Used for seeking in the stream
static SEEK_TO: AtomicU64 = AtomicU64::new(0);

pub struct PlayerEngine {
    elapsed: Duration,
    current_source: Option<String>,
    media_info: Option<MediaInfo>,
    sink: Sink,
    // We need to keep the stream around as it will stop playing when it's dropped
    _stream: OutputStream,
    tx_engine: Sender<PlayerEngineCommand>,
    tx_player: Sender<PlayerMessage>,
}

impl PlayerEngine {
    pub fn init(
        tx_engine: Sender<PlayerEngineCommand>,
        tx_player: Sender<PlayerMessage>,
    ) -> Result<Self> {
        let (_stream, handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&handle)?;
        Ok(Self {
            current_source: None,
            media_info: None,
            elapsed: Duration::default(),
            sink,
            _stream,
            tx_engine,
            tx_player,
        })
    }

    pub fn play(&mut self, source_str: &str) -> Result<MediaInfo> {
        let tx_player = self.tx_player.clone();
        let tx_engine = self.tx_engine.clone();

        if !self.sink.empty() {
            self.reset();
        }

        let (source, hint) = self.get_source(source_str)?;
        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
        let decoder = SymphoniaDecoder::new(mss, hint, self.tx_engine.clone())?;

        let media_info = decoder.media_info();
        let media_info_copy = media_info.clone();
        let duration = media_info.duration.unwrap_or_default();

        self.media_info = Some(media_info);
        self.current_source = Some(source_str.to_string());

        tx_player
            .send(PlayerMessage::Duration { duration })
            .unwrap_or_else(|e| warn!("Send error {}", e));

        // FIXME: regularly update metadata revision
        let decoder = decoder.periodic_access(Duration::from_millis(250), move |src| {
            let seek = SEEK_TO.load(Ordering::SeqCst);
            if seek > 0 {
                src.seek(Duration::from_secs(seek));
                SEEK_TO.store(0, Ordering::SeqCst);
            }
            let elapsed = src.elapsed();
            tx_engine
                .send(PlayerEngineCommand::SetElapsed(elapsed))
                .unwrap_or_else(|e| warn!("Send error {}", e));
            tx_player
                .send(PlayerMessage::Elapsed { elapsed, duration })
                .unwrap_or_else(|e| warn!("Send error {}", e));
        });

        self.sink.append(decoder);
        self.sink.play();

        self.tx_player
            .send(PlayerMessage::Playing)
            .unwrap_or_else(|e| warn!("Send error {}", e));

        Ok(media_info_copy)
    }

    pub fn restart(&mut self) -> Result<MediaInfo> {
        if let Some(source) = self.current_source.clone() {
            return self.play(&source);
        }
        Err(PlayerEngineError::NotPlaying.into())
    }

    pub fn pause(&mut self) -> Result<()> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        self.sink.pause();
        self.tx_player
            .send(PlayerMessage::Paused)
            .unwrap_or_else(|e| warn!("Send error {}", e));
        Ok(())
    }

    pub fn unpause(&mut self) -> Result<()> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        self.sink.play();
        self.tx_player
            .send(PlayerMessage::Playing)
            .unwrap_or_else(|e| warn!("Send error {}", e));
        Ok(())
    }

    pub fn toggle_play(&mut self) -> Result<bool> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        if self.sink.is_paused() {
            self.sink.play();
            Ok(true)
        } else {
            self.sink.pause();
            Ok(false)
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        self.reset();
        self.tx_player
            .send(PlayerMessage::Stopped)
            .unwrap_or_else(|e| warn!("Send error {}", e));
        Ok(())
    }

    pub fn is_paused(&self) -> Result<bool> {
        if self.is_stopped() {
            return Err(PlayerEngineError::NotPlaying.into());
        }
        Ok(self.sink.is_paused())
    }

    pub fn is_stopped(&self) -> bool {
        self.sink.len() == 0
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

    pub fn seek_to(&self, time: Duration) -> Result<Duration> {
        // We can seek between 1 second and the total duration of the track
        let duration = self.duration().unwrap_or(self.elapsed);
        let time = time.clamp(Duration::from_secs(1), duration);
        SEEK_TO.store(time.as_secs(), Ordering::SeqCst);
        // FIXME: ideally we would like to return once the seeking is successful
        // then return the current elapsed time
        Ok(time)
    }

    pub fn volume(&self) -> f32 {
        self.sink.volume()
    }

    pub fn set_volume(&mut self, volume: f32) -> f32 {
        self.sink.set_volume(volume.clamp(0.0, 1.1));
        self.sink.volume()
    }

    pub fn handle_eos(&mut self) {
        self.reset();
        self.tx_player
            .send(PlayerMessage::EndOfStream)
            .unwrap_or_else(|e| warn!("Send error {}", e));
    }

    pub fn handle_elapsed(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;
    }

    fn reset(&mut self) {
        self.elapsed = Duration::default();
        self.current_source = None;
        self.sink.pause();
        self.sink.clear();
    }

    fn get_source(&self, source_str: &str) -> Result<(Box<dyn MediaSource>, Hint)> {
        match Url::parse(source_str) {
            Ok(url) => {
                if let "http" | "https" = url.scheme() {
                    let reader = StreamDownload::new_http(source_str.parse().unwrap());
                    let path = Path::new(url.path());
                    let hint = self.get_hint(path);

                    Ok((Box::new(reader), hint))
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
