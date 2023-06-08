use audio_player::{Player, PlayerMessage};

#[tokio::main]
async fn main() {
    let player = Player::default();

    player
        .play("https://www2.cs.uic.edu/~i101/SoundFiles/CantinaBand60.wav")
        .await;

    loop {
        match player.messages.recv_async().await {
            Ok(PlayerMessage::Duration { duration }) => {
                println!("DURATION: {:?}", duration);
            }
            Ok(PlayerMessage::Elapsed { duration, elapsed }) => {
                println!("ELAPSED: {:?}", elapsed);
                if elapsed.as_secs() >= 10 {
                    player.stop().await;
                }
            }
            Ok(PlayerMessage::Stopped) => {
                println!("STOPPED");
                break;
            }
            _ => {}
        }
    }
}
