use audio_player::{Player, PlayerMessage};

fn main() {
    let player = Player::default();

    player.play("https://www2.cs.uic.edu/~i101/SoundFiles/CantinaBand60.wav");

    loop {
        match player.messages.recv() {
            Ok(PlayerMessage::Duration(duration)) => {
                println!("DURATION: {:?}", duration);
            }
            Ok(PlayerMessage::Elapsed(el)) => {
                println!("ELAPSED: {:?}", el);
                if el.as_secs() >= 10 {
                    player.stop();
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
