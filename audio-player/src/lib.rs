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
                    Ok(PlayerEngineCommand::Play(source_str)) => {
                        player.play(&source_str);
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
    pub async fn play(&self, source_str: &str) -> Result<()> {
        self.tx_engine
            .send(PlayerEngineCommand::Play(source_str.to_string()));
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
}
