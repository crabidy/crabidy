mod app;
mod config;
mod rpc;

use std::{
    error::Error,
    io,
    sync::OnceLock,
    time::{Duration, Instant},
};

use crabidy_core::proto::crabidy::{get_update_stream_response::Update as StreamUpdate, PlayState};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use flume::{Receiver, Sender};

use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::select;
use tokio_stream::StreamExt;

use app::{App, MessageFromUi, MessageToUi, StatefulList, UiFocus};
use config::Config;
use rpc::RpcClient;

static CONFIG: OnceLock<Config> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CONFIG.get_or_init(|| crabidy_core::init_config("cbd-tui.toml"));

    let (ui_tx, rx): (Sender<MessageFromUi>, Receiver<MessageFromUi>) = flume::unbounded();
    let (tx, ui_rx): (Sender<MessageToUi>, Receiver<MessageToUi>) = flume::unbounded();

    // FIXME: unwrap
    tokio::spawn(async move { orchestrate(config, (tx, rx)).await.unwrap() });

    tokio::task::spawn_blocking(|| {
        run_ui(ui_tx, ui_rx);
    })
    .await?;

    Ok(())
}

async fn orchestrate<'a>(
    config: &'static Config,
    (tx, rx): (Sender<MessageToUi>, Receiver<MessageFromUi>),
) -> Result<(), Box<dyn Error>> {
    let mut rpc_client = rpc::RpcClient::connect(&config.server.address).await?;

    if let Some(root_node) = rpc_client.get_library_node("node:/").await? {
        tx.send(MessageToUi::ReplaceLibraryNode(root_node.clone()))?;
    }

    let init_data = rpc_client.init().await?;
    tx.send_async(MessageToUi::Init(init_data)).await?;

    loop {
        if let Err(er) = poll(&mut rpc_client, &rx, &tx).await {
            println!("ERROR");
        }
    }
}

async fn poll(
    rpc_client: &mut RpcClient,
    rx: &Receiver<MessageFromUi>,
    tx: &Sender<MessageToUi>,
) -> Result<(), Box<dyn Error>> {
    select! {
        Ok(msg) = &mut rx.recv_async() => {
            match msg {
                MessageFromUi::GetLibraryNode(uuid) => {
                    if let Some(node) = rpc_client.get_library_node(&uuid).await? {
                        tx.send(MessageToUi::ReplaceLibraryNode(node.clone()));
                    }
                },
                MessageFromUi::AppendTracks(uuids) => {
                    rpc_client.append_tracks(uuids).await?
                }
                MessageFromUi::QueueTracks(uuids) => {
                    rpc_client.queue_tracks(uuids).await?
                }
                MessageFromUi::InsertTracks(uuids, pos) => {
                    rpc_client.insert_tracks(uuids, pos).await?
                }
                MessageFromUi::RemoveTracks(positions) => {
                    rpc_client.remove_tracks(positions).await?
                }
                MessageFromUi::ReplaceQueue(uuids) => {
                    rpc_client.replace_queue(uuids).await?
                }
                MessageFromUi::NextTrack => {
                    rpc_client.next_track().await?
                }
                MessageFromUi::PrevTrack => {
                    rpc_client.prev_track().await?
                }
                MessageFromUi::RestartTrack => {
                    rpc_client.restart_track().await?
                }
                MessageFromUi::SetCurrentTrack(pos) => {
                    rpc_client.set_current_track(pos).await?
                }
                MessageFromUi::TogglePlay => {
                    rpc_client.toggle_play().await?
                }
                MessageFromUi::ChangeVolume(delta) => {
                    rpc_client.change_volume(delta).await?
                }
                MessageFromUi::ToggleMute => {
                    rpc_client.toggle_mute().await?
                }
                MessageFromUi::ToggleShuffle => {
                    rpc_client.toggle_shuffle().await?
                }
                MessageFromUi::ToggleRepeat => {
                    rpc_client.toggle_repeat().await?
                }
                MessageFromUi::ClearQueue(exclude_current) => {
                    rpc_client.clear_queue(exclude_current).await?
                }
            }
        }
        Some(resp) = rpc_client.update_stream.next() => {
            match resp {
                Ok(resp) => {
                    if let Some(update) = resp.update {
                        tx.send_async(MessageToUi::Update(update)).await?;
                    }
                }
                Err(_) => {
                    rpc_client.reconnect_update_stream().await;
                }

            }
        }
    }

    Ok(())
}

fn run_ui(tx: Sender<MessageFromUi>, rx: Receiver<MessageToUi>) {
    // setup terminal
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    // create app and run it
    let mut app = App::new(tx.clone());
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        for message in rx.try_iter() {
            match message {
                MessageToUi::ReplaceLibraryNode(node) => {
                    app.library.update(node);
                }
                MessageToUi::Init(init_data) => {
                    if let Some(queue) = init_data.queue {
                        app.queue.update_queue(queue);
                    }
                    if let Some(track) = init_data.queue_track {
                        app.now_playing.update_track(track.track);
                        app.queue.update_position(track.queue_position as usize);
                    }
                    if let Some(ps) = PlayState::from_i32(init_data.play_state) {
                        app.now_playing.update_play_state(ps);
                    }
                    if let Some(mods) = init_data.mods {
                        app.now_playing.update_modifiers(&mods);
                    }
                }
                MessageToUi::Update(update) => match update {
                    StreamUpdate::Queue(queue) => {
                        app.queue.update_queue(queue);
                    }
                    StreamUpdate::QueueTrack(track) => {
                        app.now_playing.update_track(track.track);
                        app.queue.update_position(track.queue_position as usize);
                    }
                    StreamUpdate::Position(pos) => app.now_playing.update_position(pos),
                    StreamUpdate::PlayState(play_state) => {
                        if let Some(ps) = PlayState::from_i32(play_state) {
                            app.now_playing.update_play_state(ps);
                        }
                    }
                    StreamUpdate::Mods(mods) => {
                        app.now_playing.update_modifiers(&mods);
                    }
                    StreamUpdate::Mute(_) => { /* FIXME: implement */ }
                    StreamUpdate::Volume(_) => { /* FIXME: implement */ }
                },
            }
        }

        terminal.draw(|f| app.render(f));

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                if key.kind == KeyEventKind::Press {
                    match (app.focus, key.modifiers, key.code) {
                        (_, KeyModifiers::NONE, KeyCode::Char('q')) => {
                            break;
                        }
                        (_, KeyModifiers::NONE, KeyCode::Tab) => app.cycle_active(),
                        (_, KeyModifiers::NONE, KeyCode::Char(' ')) => {
                            tx.send(MessageFromUi::TogglePlay);
                        }
                        (_, KeyModifiers::NONE, KeyCode::Char('r')) => {
                            tx.send(MessageFromUi::RestartTrack);
                        }
                        (_, KeyModifiers::SHIFT, KeyCode::Char('J')) => {
                            tx.send(MessageFromUi::ChangeVolume(-0.1));
                        }
                        (_, KeyModifiers::SHIFT, KeyCode::Char('K')) => {
                            tx.send(MessageFromUi::ChangeVolume(0.1));
                        }
                        (_, KeyModifiers::NONE, KeyCode::Char('m')) => {
                            tx.send(MessageFromUi::ToggleMute);
                        }
                        (_, KeyModifiers::NONE, KeyCode::Char('z')) => {
                            tx.send(MessageFromUi::ToggleShuffle);
                        }
                        (_, KeyModifiers::NONE, KeyCode::Char('x')) => {
                            tx.send(MessageFromUi::ToggleRepeat);
                        }
                        (_, KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                            app.queue.play_next();
                        }
                        (_, KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                            app.queue.play_prev();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('g')) => {
                            app.library.first();
                        }
                        (UiFocus::Library, KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                            app.library.last();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('j')) => {
                            app.library.next();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('k')) => {
                            app.library.prev();
                        }
                        (UiFocus::Library, KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                            app.library.down();
                        }
                        (UiFocus::Library, KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                            app.library.up();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('h')) => {
                            app.library.ascend();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('l')) => {
                            app.library.dive();
                        }
                        (UiFocus::Library, KeyModifiers::SHIFT, KeyCode::Char('L')) => {
                            app.library.queue_queue();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('a')) => {
                            app.library.queue_append();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.library.queue_replace();
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('s')) => {
                            app.library.toggle_mark();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('p')) => {
                            if let Some(selected) = app.queue.selected() {
                                app.library.queue_insert(selected);
                            }
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('g')) => {
                            app.queue.first();
                        }
                        (UiFocus::Queue, KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                            app.queue.last();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('j')) => {
                            app.queue.next();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('k')) => {
                            app.queue.prev();
                        }
                        (UiFocus::Queue, KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                            app.queue.down();
                        }
                        (UiFocus::Queue, KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                            app.queue.up();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('o')) => {
                            app.queue.select_current();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.queue.play_selected();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('d')) => {
                            app.queue.remove_track();
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('c')) => {
                            tx.send(MessageFromUi::ClearQueue(true));
                        }
                        (UiFocus::Queue, KeyModifiers::SHIFT, KeyCode::Char('C')) => {
                            tx.send(MessageFromUi::ClearQueue(false));
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // restore terminal
    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();
}
