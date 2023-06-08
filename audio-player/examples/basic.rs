use std::{thread, time::Duration};

use audio_player::{Player, PlayerMessage};

#[tokio::main]
async fn main() {
    let player = Player::default();

    player
        .play("https://www2.cs.uic.edu/~i101/SoundFiles/gettysburg10.wav")
        .await
        .unwrap();

    loop {
        match player.messages.recv_async().await {
            Ok(PlayerMessage::Elapsed { duration, elapsed }) => {
                println!("ELAPSED: {:?}", elapsed);
            }
            Ok(PlayerMessage::EndOfStream) => {
                println!("END OF STREAM");
                player
                    .play("https://www2.cs.uic.edu/~i101/SoundFiles/preamble10.wav")
                    .await
                    .unwrap();
                break;
            }
            _ => {}
        }
    }

    loop {
        match player.messages.recv_async().await {
            Ok(PlayerMessage::Elapsed { duration, elapsed }) => {
                println!("ELAPSED: {:?}", elapsed);
            }
            Ok(PlayerMessage::EndOfStream) => {
                println!("END OF STREAM 2");
                break;
            }
            _ => {}
        }
    }
}
