mod decoder;
mod player_engine;

use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use decoder::MediaInfo;
use flume::{Receiver, Sender};

pub use player_engine::PlayerMessage;
use player_engine::{PlayerEngine, PlayerEngineCommand};

// TODO:
// * Emit buffering
// * Emit errors

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
                        let res = player.play(&source_str);
                        tx.send(res);
                    }
                    Ok(PlayerEngineCommand::Pause) => {
                        player.pause();
                    }
                    Ok(PlayerEngineCommand::Unpause) => {
                        player.unpause();
                    }
                    Ok(PlayerEngineCommand::Stop) => {
                        player.stop();
                    }
                    Ok(PlayerEngineCommand::TogglePlay) => {
                        player.toggle_play();
                    }
                    Ok(PlayerEngineCommand::Eos) => {
                        player.handle_eos();
                    }
                    Err(e) => {
                        // FIXME: debug!(e);
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
    // FIXME: this could check if the player started playing using a channel
    // Then it would be async (wait for Playing for example)
    pub async fn play(&self, source_str: &str) -> Result<MediaInfo> {
        let (tx, rx) = flume::bounded(1);
        self.tx_engine
            .send(PlayerEngineCommand::Play(source_str.to_string(), tx));
        if let Ok(res) = rx.recv_async().await {
            return res;
        }
        // FIXME: add error type
        Err(anyhow!("Player channel error"))
    }

    pub async fn elpased(&self) -> Duration {
        // FIXME: implement
        Duration::default()
    }

    pub async fn duration(&self) -> Duration {
        // FIXME: implement
        Duration::default()
    }

    pub async fn volume(&self) -> f32 {
        // FIXME: implement
        0.0
    }

    pub async fn set_volume(&self) -> Result<()> {
        // FIXME: implement
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        self.tx_engine.send(PlayerEngineCommand::Pause);
        Ok(())
    }

    pub async fn unpause(&self) -> Result<()> {
        self.tx_engine.send(PlayerEngineCommand::Unpause);
        Ok(())
    }

    pub async fn toggle_play(&self) -> Result<()> {
        self.tx_engine.send(PlayerEngineCommand::TogglePlay);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.tx_engine.send(PlayerEngineCommand::Stop);
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        // FIXME: implement
        Ok(())
    }
}
