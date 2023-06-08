mod decoder;
mod player_engine;

use std::thread;
use std::time::Duration;

use anyhow::Result;
pub use decoder::MediaInfo;
use flume::{Receiver, Sender};

pub use player_engine::PlayerMessage;
use player_engine::{PlayerEngine, PlayerEngineCommand};
use tracing::warn;

// TODO:
// * Emit buffering

pub enum PlayerError {}

pub struct Player {
    pub messages: Receiver<PlayerMessage>,
    tx_engine: Sender<PlayerEngineCommand>,
}

impl Default for Player {
    fn default() -> Self {
        let (tx_engine, rx_engine) = flume::bounded(10);
        let (tx_player, messages): (Sender<PlayerMessage>, Receiver<PlayerMessage>) =
            flume::bounded(10);

        let tx_decoder = tx_engine.clone();

        thread::spawn(move || {
            let mut player = PlayerEngine::new(tx_decoder, tx_player);
            loop {
                match rx_engine.recv() {
                    Ok(PlayerEngineCommand::Play(source_str, tx)) => {
                        tx.send(player.play(&source_str))
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::Pause(tx)) => {
                        tx.send(player.pause())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::Unpause(tx)) => {
                        tx.send(player.unpause())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::Stop(tx)) => {
                        tx.send(player.stop())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::TogglePlay(tx)) => {
                        tx.send(player.toggle_play())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::Restart(tx)) => {
                        tx.send(player.restart())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::GetDuration(tx)) => {
                        tx.send(player.duration())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::GetElapsed(tx)) => {
                        tx.send(player.elapsed())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::GetVolume(tx)) => {
                        tx.send(player.volume())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::GetPaused(tx)) => {
                        tx.send(player.is_paused())
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::SetVolume(volume, tx)) => {
                        tx.send(player.set_volume(volume))
                            .unwrap_or_else(|e| warn!("Send error {}", e));
                    }
                    Ok(PlayerEngineCommand::SetElapsed(elapsed)) => {
                        player.handle_elapsed(elapsed);
                    }
                    Ok(PlayerEngineCommand::Eos) => {
                        player.handle_eos();
                    }
                    Err(e) => {
                        warn!("Recv error {}", e);
                    }
                }
            }
        });

        Self {
            messages,
            tx_engine,
        }
    }
}

impl Player {
    pub async fn play(&self, source_str: &str) -> Result<MediaInfo> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine
            .send(PlayerEngineCommand::Play(source_str.to_string(), tx))?;
        rx.recv_async().await?
    }

    pub async fn restart(&self) -> Result<MediaInfo> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::Restart(tx))?;
        rx.recv_async().await?
    }

    pub async fn elpased(&self) -> Result<Duration> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::GetElapsed(tx))?;
        rx.recv_async().await?
    }

    pub async fn duration(&self) -> Result<Duration> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::GetDuration(tx))?;
        rx.recv_async().await?
    }

    pub async fn volume(&self) -> Result<f32> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::GetVolume(tx))?;
        rx.recv_async().await?
    }

    pub async fn is_paused(&self) -> Result<bool> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::GetPaused(tx))?;
        rx.recv_async().await?
    }

    pub async fn set_volume(&self, volume: f32) -> Result<f32> {
        let (tx, rx) = flume::bounded(1);
        let vol = volume.clamp(0.0, 1.1);
        self.tx_engine
            .send(PlayerEngineCommand::SetVolume(vol, tx))?;
        rx.recv_async().await?
    }

    pub async fn pause(&self) -> Result<()> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::Pause(tx))?;
        rx.recv_async().await?
    }

    pub async fn unpause(&self) -> Result<()> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::Unpause(tx))?;
        rx.recv_async().await?
    }

    pub async fn toggle_play(&self) -> Result<bool> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::TogglePlay(tx))?;
        rx.recv_async().await?
    }

    pub async fn stop(&self) -> Result<()> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine.send(PlayerEngineCommand::Stop(tx))?;
        rx.recv_async().await?
    }
}
