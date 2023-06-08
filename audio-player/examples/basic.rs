use std::{thread, time::Duration};

use audio_player::{Player, PlayerMessage};

#[tokio::main]
async fn main() {
    let player = Player::default();
    let messages = player.messages.clone();

    // Make sure we read all the messages in time
    thread::spawn(move || loop {
        match messages.recv() {
            Ok(PlayerMessage::Playing) => {
                println!("PLAYING NEW TRACK");
            }
            Ok(PlayerMessage::Duration { duration }) => {
                println!("DURATION: {:?}", duration);
            }
            Ok(PlayerMessage::Elapsed {
                duration: _,
                elapsed,
            }) => {
                println!("ELAPSED: {:?}", elapsed);
            }
            Ok(PlayerMessage::Stopped) => {
                println!("STOPPED");
                break;
            }
            _ => {}
        }
    });

    player
        .play("https://www2.cs.uic.edu/~i101/SoundFiles/CantinaBand60.wav")
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(5)).await;

    player
        .play("https://www2.cs.uic.edu/~i101/SoundFiles/PinkPanther60.wav")
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(60)).await;
}
