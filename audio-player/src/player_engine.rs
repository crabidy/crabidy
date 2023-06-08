use flume::{Receiver, Sender};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::thread;
use std::time::Duration;
use symphonia::core::probe::Hint;
use url::Url;

use crate::decoder::{MediaInfo, SymphoniaDecoder};
use anyhow::{anyhow, Result};
use rodio::source::{PeriodicAccess, SineWave};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use stream_download::StreamDownload;
use symphonia::core::io::{
    MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource,
};

pub enum PlayerEngineCommand {
    Play(String),
    Pause,
    Unpause,
    TogglePlay,
    Stop,
    Eos,
}

// FIXME: sort out media info size (probably send pointers to stuff on the heap)
pub enum PlayerMessage {
    // MediaInfo(MediaInfo),
    Duration(Duration),
    Elapsed(Duration),
    Stopped,
    Paused,
    Playing,
}

// TODO:
// * Emit buffering

pub struct PlayerEngine {
    sink: Option<Sink>,
    stream: Option<OutputStream>,
    tx_engine: Sender<PlayerEngineCommand>,
    tx_player: Sender<PlayerMessage>,
}

impl PlayerEngine {
    pub fn new(tx_engine: Sender<PlayerEngineCommand>, tx_player: Sender<PlayerMessage>) -> Self {
        Self {
            sink: None,
            stream: None,
            tx_engine,
            tx_player,
        }
    }

    pub fn play(&mut self, source_str: &str) -> Result<()> {
        let (stream, handle) = OutputStream::try_default()?;
        let mut sink = Sink::try_new(&handle)?;
        let (source, hint) = self.get_source(source_str)?;
        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        let tx_player = self.tx_player.clone();

        let decoder = SymphoniaDecoder::new(mss, hint, self.tx_engine.clone())?;

        let media_info = decoder.media_info();
        tx_player.send(PlayerMessage::Duration(
            media_info.duration.unwrap_or_default(),
        ));
        // tx_player.send(PlayerEngineMessage::MediaInfo(media_info));

        let decoder = decoder.periodic_access(Duration::from_millis(250), move |src| {
            tx_player.send(PlayerMessage::Elapsed(src.elapsed()));
        });

        sink.append(decoder);

        // We need to keep the stream around, otherwise it gets dropped outside of this scope
        self.stream = Some(stream);
        // The sink is used to control the stream
        self.sink = Some(sink);

        self.tx_player.send(PlayerMessage::Playing);

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
            self.tx_player.send(PlayerMessage::Paused);
        }
    }

    pub fn unpause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
            self.tx_player.send(PlayerMessage::Playing);
        }
    }

    pub fn toggle_play(&mut self) {
        if self.is_stopped() {
            return;
        }
        if self.is_paused() {
            self.unpause();
        } else {
            self.pause();
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.stop();
            self.sink.take();
            self.stream.take();
            self.tx_player.send(PlayerMessage::Stopped);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.sink
            .as_ref()
            .map(|s| s.is_paused())
            .unwrap_or_default()
    }

    pub fn is_stopped(&self) -> bool {
        self.sink.is_none()
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
