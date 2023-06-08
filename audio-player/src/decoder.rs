use flume::Sender;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use symphonia::{
    core::{
        audio::{AudioBufferRef, SampleBuffer, SignalSpec},
        codecs::{Decoder, DecoderOptions},
        errors::Error as SymphoniaError,
        formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track},
        io::MediaSourceStream,
        meta::{MetadataOptions, MetadataRevision},
        probe::Hint,
        units::{Time, TimeBase},
    },
    default::get_probe,
};
use tracing::warn;

use rodio::Source;

use crate::PlayerEngineCommand;

// Decoder errors are not considered fatal.
// The correct action is to just get a new packet and try again.
// But a decode error in more than 3 consecutive packets is fatal.
const MAX_DECODE_ERRORS: usize = 3;

#[derive(Clone)]
pub struct MediaInfo {
    pub duration: Option<Duration>,
    pub metadata: Option<MetadataRevision>,
    pub track: Track,
}

pub struct SymphoniaDecoder {
    decoder: Box<dyn Decoder>,
    current_frame_offset: usize,
    format: Box<dyn FormatReader>,
    buffer: SampleBuffer<i16>,
    spec: SignalSpec,
    time_base: Option<TimeBase>,
    duration: u64,
    elapsed: u64,
    metadata: Option<MetadataRevision>,
    track: Track,
    tx: Sender<PlayerEngineCommand>,
}

impl SymphoniaDecoder {
    pub fn new(
        mss: MediaSourceStream,
        hint: Hint,
        tx: Sender<PlayerEngineCommand>,
    ) -> Result<Self, DecoderError> {
        match SymphoniaDecoder::init(mss, hint, tx) {
            Err(e) => match e {
                SymphoniaError::IoError(e) => Err(DecoderError::IoError(e.to_string())),
                SymphoniaError::DecodeError(e) => Err(DecoderError::DecodeError(e)),
                SymphoniaError::SeekError(_) => {
                    unreachable!("Seek errors should not occur during initialization")
                }
                SymphoniaError::Unsupported(_) => Err(DecoderError::UnrecognizedFormat),
                SymphoniaError::LimitError(e) => Err(DecoderError::LimitError(e)),
                SymphoniaError::ResetRequired => Err(DecoderError::ResetRequired),
            },
            Ok(Some(decoder)) => Ok(decoder),
            Ok(None) => Err(DecoderError::NoStreams),
        }
    }

    pub fn into_inner(self) -> MediaSourceStream {
        self.format.into_inner()
    }

    fn init(
        mss: MediaSourceStream,
        hint: Hint,
        tx: Sender<PlayerEngineCommand>,
    ) -> symphonia::core::errors::Result<Option<SymphoniaDecoder>> {
        let format_opts: FormatOptions = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let metadata_opts: MetadataOptions = Default::default();
        let mut probed = get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;

        let track = match probed.format.default_track() {
            Some(stream) => stream,
            None => return Ok(None),
        }
        .clone();

        let time_base = track.codec_params.time_base;

        let duration = track
            .codec_params
            .n_frames
            .map(|frames| track.codec_params.start_ts + frames)
            .unwrap_or_default();

        let mut _elapsed = 0;

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })?;

        let mut decode_errors: usize = 0;
        let decoded = loop {
            let current_frame = probed.format.next_packet()?;
            _elapsed = current_frame.ts();
            match decoder.decode(&current_frame) {
                Ok(decoded) => break decoded,
                Err(e) => match e {
                    SymphoniaError::DecodeError(_) => {
                        decode_errors += 1;
                        if decode_errors > MAX_DECODE_ERRORS {
                            return Err(e);
                        } else {
                            continue;
                        }
                    }
                    _ => return Err(e),
                },
            }
        };
        let spec = decoded.spec().to_owned();
        let buffer = SymphoniaDecoder::get_buffer(decoded, &spec);

        // Prefer metadata that's provided in the container format, over other tags found during the
        // probe operation.
        let metadata = probed.format.metadata().current().cloned().or_else(|| {
            probed
                .metadata
                .get()
                .as_ref()
                .and_then(|m| m.current().cloned())
        });

        Ok(Some(SymphoniaDecoder {
            decoder,
            current_frame_offset: 0,
            format: probed.format,
            buffer,
            spec,
            time_base,
            duration,
            elapsed: _elapsed,
            metadata,
            track,
            tx,
        }))
    }

    #[inline]
    pub fn media_info(&self) -> MediaInfo {
        MediaInfo {
            duration: self.total_duration(),
            metadata: self.metadata.clone(),
            track: self.track.clone(),
        }
    }

    #[inline]
    pub fn elapsed(&self) -> Duration {
        if let Some(tb) = self.time_base {
            let time = tb.calc_time(self.elapsed);
            return Duration::from_secs_f64(time.seconds as f64 + time.frac);
        };
        Duration::default()
    }

    #[inline]
    pub fn seek(&mut self, time: Duration) -> Option<Duration> {
        let nanos_per_sec = 1_000_000_000.0;
        match self.format.seek(
            SeekMode::Coarse,
            SeekTo::Time {
                time: Time::new(
                    time.as_secs(),
                    f64::from(time.subsec_nanos()) / nanos_per_sec,
                ),
                track_id: None,
            },
        ) {
            Ok(seeked_to) => {
                let base = TimeBase::new(1, self.sample_rate());
                let time = base.calc_time(seeked_to.actual_ts);

                Some(Duration::from_millis(
                    time.seconds * 1000 + ((time.frac * 60. * 1000.).round() as u64),
                ))
            }
            Err(_) => None,
        }
    }

    #[inline]
    fn get_buffer(decoded: AudioBufferRef, spec: &SignalSpec) -> SampleBuffer<i16> {
        let duration = decoded.capacity() as u64;
        let mut buffer = SampleBuffer::<i16>::new(duration, *spec);
        buffer.copy_interleaved_ref(decoded);
        buffer
    }
}

impl Source for SymphoniaDecoder {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.spec.channels.count() as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.spec.rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        match self.time_base {
            Some(tb) => {
                let time = tb.calc_time(self.duration);
                Some(Duration::from_secs_f64(time.seconds as f64 + time.frac))
            }
            None => None,
        }
    }
}

impl Iterator for SymphoniaDecoder {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset == self.buffer.len() {
            let mut decode_errors: usize = 0;
            let decoded = loop {
                match self.format.next_packet() {
                    Ok(packet) => {
                        self.elapsed = packet.ts();
                        match self.decoder.decode(&packet) {
                            Ok(decoded) => break decoded,
                            Err(e) => match e {
                                SymphoniaError::DecodeError(_) => {
                                    decode_errors += 1;
                                    if decode_errors > MAX_DECODE_ERRORS {
                                        return None;
                                    } else {
                                        continue;
                                    }
                                }
                                _ => return None,
                            },
                        }
                    }
                    Err(SymphoniaError::IoError(err)) => {
                        if err.kind() == std::io::ErrorKind::UnexpectedEof
                            && err.to_string() == "end of stream"
                        {
                            self.tx
                                .send(PlayerEngineCommand::Eos)
                                .unwrap_or_else(|e| warn!("Send error {}", e));
                            return None;
                        }
                    }
                    Err(_) => return None,
                }
            };
            self.spec = decoded.spec().to_owned();
            self.buffer = SymphoniaDecoder::get_buffer(decoded, &self.spec);
            self.current_frame_offset = 0;
        }

        let sample = *self.buffer.samples().get(self.current_frame_offset)?;
        self.current_frame_offset += 1;

        Some(sample)
    }
}

/// Error that can happen when creating a decoder.
#[derive(Debug, Clone)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    UnrecognizedFormat,

    /// An IO error occurred while reading, writing, or seeking the stream.
    IoError(String),

    /// The stream contained malformed data and could not be decoded or demuxed.
    DecodeError(&'static str),

    /// A default or user-defined limit was reached while decoding or demuxing the stream. Limits
    /// are used to prevent denial-of-service attacks from malicious streams.
    LimitError(&'static str),

    /// The demuxer or decoder needs to be reset before continuing.
    ResetRequired,

    /// No streams were found by the decoder
    NoStreams,
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            DecoderError::UnrecognizedFormat => "Unrecognized format",
            DecoderError::IoError(msg) => &msg[..],
            DecoderError::DecodeError(msg) => msg,
            DecoderError::LimitError(msg) => msg,
            DecoderError::ResetRequired => "Reset required",
            DecoderError::NoStreams => "No streams",
        };
        write!(f, "{}", text)
    }
}

impl Error for DecoderError {}
