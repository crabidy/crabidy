mod decoder;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::thread;
use std::time::Duration;
use symphonia::core::probe::Hint;
use url::Url;

use anyhow::{anyhow, Result};
use decoder::SymphoniaDecoder;
use rodio::source::{PeriodicAccess, SineWave};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use stream_download::StreamDownload;
use symphonia::core::io::{
    MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource,
};

struct Player {
    sink: Option<Sink>,
    stream: Option<OutputStream>,
}

// TODO:
// * Emit Metadata
// * Emit duration
// * Emit track data
// * Emit EOS
// * Emit buffering

impl Player {
    pub fn default() -> Self {
        Self {
            sink: None,
            stream: None,
        }
    }

    pub fn play(&mut self, source_str: &str) -> Result<()> {
        let (stream, handle) = OutputStream::try_default()?;
        let mut sink = Sink::try_new(&handle)?;
        let (source, hint) = self.get_source(source_str)?;
        let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

        let decoder = SymphoniaDecoder::new(mss, hint)?;

        let media_info = decoder.media_info();

        let decoder = decoder.periodic_access(Duration::from_millis(500), |src| {
            println!("ELAPSED: {:?}", src.elapsed());

            if src.elapsed().as_secs() > 10 {
                src.seek(Duration::from_secs(2));
            }
        });

        sink.append(decoder);

        // We need to keep the stream around, otherwise it gets dropped outside of this scope
        self.stream = Some(stream);
        // The sink is used to control the stream
        self.sink = Some(sink);

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn unpause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.stop();
            self.sink.take();
            self.stream.take();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().map(|s| !s.is_paused()).unwrap_or(false)
    }

    pub fn is_paused(&self) -> bool {
        self.sink.as_ref().map(|s| s.is_paused()).unwrap_or(false)
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

fn main() {
    let mut player = Player::default();
    player.play("./Slip.m4a");

    thread::sleep(Duration::from_millis(5000));

    player.stop();
}
