use audio_player::PlayerMessage;
use crabidy_core::proto::crabidy::{
    crabidy_service_server::CrabidyServiceServer, InitResponse, LibraryNode, PlayState, Track,
};
use crabidy_core::{ProviderClient, ProviderError};
use tracing::{debug_span, info, instrument, warn, Span};
use tracing_subscriber::{filter::Targets, prelude::*};

mod playback;
use playback::Playback;
mod provider;
use provider::ProviderOrchestrator;
mod rpc;
use rpc::RpcService;

use tonic::{transport::Server, Result};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(err) = tracing_log::LogTracer::init_with_filter(log::LevelFilter::Debug) {
        println!("Failed to initialize log tracer: {}", err);
    }
    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stderr());

    let targets_filter =
        Targets::new().with_target("crabidy_server", tracing::level_filters::LevelFilter::DEBUG);
    let subscriber = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_file(true)
        .with_line_number(true);

    let registry = tracing_subscriber::registry()
        .with(targets_filter)
        .with(subscriber);

    tracing::subscriber::set_global_default(registry)
        .expect("Setting the default tracing subscriber failed");

    info!("audio player started initialized");

    let (update_tx, _) = tokio::sync::broadcast::channel(2048);
    let orchestrator = ProviderOrchestrator::init("").await.unwrap();

    let playback = Playback::new(update_tx.clone(), orchestrator.provider_tx.clone());

    let playback_tx = playback.playback_tx.clone();
    let player_msg = playback.player.messages.clone();

    std::thread::spawn(|| {
        poll_play_bus(player_msg, playback_tx);
    });
    info!("gstreamer bus handler started");

    let crabidy_service = RpcService::new(
        update_tx,
        playback.playback_tx.clone(),
        orchestrator.provider_tx.clone(),
    );
    orchestrator.run();
    info!("provider orchestrator started");
    playback.run();
    info!("playback started");

    let addr = "0.0.0.0:50051".parse()?;
    Server::builder()
        .add_service(CrabidyServiceServer::new(crabidy_service))
        .serve(addr)
        .await?;

    Ok(())
}

#[instrument(skip(rx, tx))]
fn poll_play_bus(rx: flume::Receiver<PlayerMessage>, tx: flume::Sender<PlaybackMessage>) {
    for msg in rx.iter() {
        let span = debug_span!("play-chan");
        match msg {
            PlayerMessage::EndOfStream => {
                tx.send(PlaybackMessage::Next { span }).unwrap();
            }
            PlayerMessage::Stopped => {
                tx.send(PlaybackMessage::StateChanged {
                    state: PlayState::Stopped,
                    span,
                })
                .unwrap();
            }
            PlayerMessage::Paused => {
                tx.send(PlaybackMessage::StateChanged {
                    state: PlayState::Paused,
                    span,
                })
                .unwrap();
            }
            PlayerMessage::Playing => {
                tx.send(PlaybackMessage::StateChanged {
                    state: PlayState::Playing,
                    span,
                })
                .unwrap();
            }
            PlayerMessage::Elapsed { duration, elapsed } => {
                tx.send(PlaybackMessage::PostitionChanged {
                    duration: duration.as_millis() as u32,
                    position: elapsed.as_millis() as u32,
                    span,
                })
                .unwrap();
            }
            PlayerMessage::Duration { duration } => {
                tx.send(PlaybackMessage::PostitionChanged {
                    duration: duration.as_millis() as u32,
                    position: 0,
                    span,
                })
                .unwrap();
            }
        }
    }
}

#[derive(Debug)]
pub enum ProviderMessage {
    GetLibraryNode {
        uuid: String,
        result_tx: flume::Sender<Result<LibraryNode, ProviderError>>,
        span: Span,
    },
    GetTrack {
        uuid: String,
        result_tx: flume::Sender<Result<Track, ProviderError>>,
        span: Span,
    },
    GetTrackUrls {
        uuid: String,
        result_tx: flume::Sender<Result<Vec<String>, ProviderError>>,
        span: Span,
    },
    FlattenNode {
        uuid: String,
        result_tx: flume::Sender<Vec<Track>>,
        span: Span,
    },
}

#[derive(Debug)]
pub enum PlaybackMessage {
    Init {
        result_tx: flume::Sender<InitResponse>,
        span: Span,
    },
    Replace {
        uuids: Vec<String>,
        span: Span,
    },
    Queue {
        uuids: Vec<String>,
        span: Span,
    },
    Append {
        uuids: Vec<String>,
        span: Span,
    },
    Remove {
        positions: Vec<u32>,
        span: Span,
    },
    Insert {
        position: u32,
        uuids: Vec<String>,
        span: Span,
    },
    SetCurrent {
        position: u32,
        span: Span,
    },
    ToggleShuffle {
        span: Span,
    },
    ToggleRepeat {
        span: Span,
    },
    TogglePlay {
        span: Span,
    },
    Stop {
        span: Span,
    },
    ChangeVolume {
        delta: f32,
        span: Span,
    },
    ToggleMute {
        span: Span,
    },
    Next {
        span: Span,
    },
    Prev {
        span: Span,
    },
    RestartTrack {
        span: Span,
    },
    StateChanged {
        state: PlayState,
        span: Span,
    },
    VolumeChanged {
        volume: f32,
        span: Span,
    },

    MuteChanged {
        muted: bool,
        span: Span,
    },
    PostitionChanged {
        duration: u32,
        position: u32,
        span: Span,
    },
}
