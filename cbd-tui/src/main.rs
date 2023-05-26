mod rpc;

use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, get_queue_updates_response::QueueUpdateResult,
    GetLibraryNodeRequest, GetQueueUpdatesRequest, GetQueueUpdatesResponse,
    GetTrackUpdatesResponse, LibraryNode, LibraryNodeState,
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use flume::{Receiver, Sender};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Corner, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use rpc::RpcClient;
use std::{
    collections::HashMap,
    error::Error,
    fmt, io, println, thread,
    time::{Duration, Instant},
    vec,
};
use tokio::{select, signal, task};
use tokio_stream::StreamExt;
// use
use tonic::{transport::Channel, Request, Streaming};

struct StatefulList<T> {
    state: ListState,
    items: Vec<T>,
    prev_selected: usize,
}

impl<T> StatefulList<T> {
    fn default() -> Self {
        let mut state = ListState::default();
        Self {
            state,
            items: Vec::default(),
            prev_selected: 0,
        }
    }

    fn next(&mut self) {
        if let Some(i) = self.state.selected() {
            let next = if i == self.items.len() - 1 { 0 } else { i + 1 };
            self.state.select(Some(next));
        } else {
            self.state.select(Some(0));
        }
    }

    fn prev(&mut self) {
        if let Some(i) = self.state.selected() {
            let prev = if i == 0 { self.items.len() - 1 } else { i - 1 };
            self.state.select(Some(prev));
        } else {
            self.state.select(Some(0));
        }
    }

    fn is_focused(&self) -> bool {
        self.state.selected().is_some()
    }

    fn focus(&mut self) {
        if self.is_focused() {
            return;
        }
        self.state.select(Some(self.prev_selected));
    }

    fn blur(&mut self) {
        if !self.is_focused() {
            return;
        }
        if let Some(i) = self.state.selected() {
            self.prev_selected = i;
        } else {
            self.prev_selected = 0;
        }
        self.state.select(None);
    }

    fn get_selected(&self) -> Option<&T> {
        if let Some(idx) = self.state.selected() {
            return Some(&self.items[idx]);
        }
        None
    }
}

struct UiItem {
    uuid: String,
    title: String,
}

struct LibraryView {
    title: String,
    uuid: String,
    list: StatefulList<UiItem>,
    parent: Option<String>,
}

impl LibraryView {
    fn update(&mut self, node: LibraryNode) {
        if node.tracks.is_empty() && node.children.is_empty() {
            return;
        }
        // if children empty and tracks empty return
        self.uuid = node.uuid;
        self.title = node.name;
        self.parent = node.parent;

        if !node.tracks.is_empty() {
            self.list.items = node
                .tracks
                .iter()
                .map(|t| UiItem {
                    uuid: t.uuid.clone(),
                    title: t.title.clone(),
                })
                .collect();
        } else {
            // if tracks not empty use tracks instead
            self.list.items = node
                .children
                .iter()
                .map(|c| UiItem {
                    uuid: c.to_string(),
                    title: c.to_string(),
                })
                .collect();
        }
    }
}

struct App {
    library: LibraryView,
    queue: StatefulList<UiItem>,
}

impl App {
    fn new() -> App {
        let mut library = LibraryView {
            title: "Library".to_string(),
            uuid: "/".to_string(),
            list: StatefulList::default(),
            parent: None,
        };
        library.list.focus();
        let mut queue = StatefulList::default();
        App { library, queue }
    }

    fn cycle_active(&mut self) {
        if self.library.list.is_focused() {
            self.library.list.blur();
            self.queue.focus();
        } else {
            self.library.list.focus();
            self.queue.blur();
        }
    }
}

// FIXME: Rename this
enum MessageToUi {
    ReplaceLibraryNode(LibraryNode),
    QueueStreamUpdate(QueueUpdateResult),
    TrackStreamUpdate(GetTrackUpdatesResponse),
}

// FIXME: Rename this
enum MessageFromUi {
    Quit,
    GetLibraryNode(String),
}

async fn orchestrate<'a>(
    (tx, rx): (Sender<MessageToUi>, Receiver<MessageFromUi>),
) -> Result<(), Box<dyn Error>> {
    let mut rpc_client = rpc::RpcClient::connect("http://[::1]:50051").await?;

    if let Some(root_node) = rpc_client.get_library_node("/").await? {
        // FIXME: Is it ok to clone here?
        tx.send(MessageToUi::ReplaceLibraryNode(root_node.clone()));
    }

    // FIXME: stream failures, do we need to re-establish the stream?
    let mut queue_update_stream = rpc_client.get_queue_updates_stream().await?;
    let mut track_update_stream = rpc_client.get_track_updates_stream().await?;

    loop {
        select! {
            Ok(msg) = &mut rx.recv_async() => {
                match msg {
                    MessageFromUi::Quit => {
                        break Ok(());
                    },
                    MessageFromUi::GetLibraryNode(uuid) => {
                        if let Some(node) = rpc_client.get_library_node(&uuid).await? {
                            tx.send(MessageToUi::ReplaceLibraryNode(node.clone()));
                        }
                    }
                }
            }
            Some(Ok(resp)) = queue_update_stream.next() => {
                if let Some(res) = resp.queue_update_result {
                    tx.send_async(MessageToUi::QueueStreamUpdate(res)).await;
                }
            }
            Some(Ok(resp)) = track_update_stream.next() => {
                tx.send(MessageToUi::TrackStreamUpdate(resp));
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui_tx, rx): (Sender<MessageFromUi>, Receiver<MessageFromUi>) = flume::unbounded();
    let (tx, ui_rx): (Sender<MessageToUi>, Receiver<MessageToUi>) = flume::unbounded();

    thread::spawn(|| {
        run_ui(ui_tx, ui_rx);
    });

    // FIXME: unwrap
    tokio::spawn(async move { orchestrate((tx, rx)).await.unwrap() });

    signal::ctrl_c().await.unwrap();

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
    let mut app = App::new();
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        for message in rx.try_iter() {
            match message {
                MessageToUi::ReplaceLibraryNode(node) => {
                    app.library.update(node);
                }
                MessageToUi::QueueStreamUpdate(queue_update) => match queue_update {
                    QueueUpdateResult::Full(queue) => {}
                    QueueUpdateResult::PositionChange(pos) => {
                        app.queue.items.push(UiItem {
                            uuid: pos.timestamp.to_string(),
                            title: pos.timestamp.to_string(),
                        });
                    }
                },
                _ => {}
            }
        }

        terminal.draw(|f| ui(f, &mut app)).unwrap();

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout).unwrap() {
            if let Event::Key(key) = event::read().unwrap() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            tx.send(MessageFromUi::Quit);
                            break;
                        }
                        KeyCode::Char('j') => {
                            if app.library.list.is_focused() {
                                app.library.list.next();
                            } else {
                                app.queue.next();
                            }
                        }
                        KeyCode::Char('k') => {
                            if app.library.list.is_focused() {
                                app.library.list.prev();
                            } else {
                                app.queue.prev();
                            }
                        }
                        KeyCode::Tab => app.cycle_active(),
                        KeyCode::Char('h') => {
                            if let Some(parent) = app.library.parent.as_ref() {
                                tx.send(MessageFromUi::GetLibraryNode(parent.clone()));
                            }
                        }
                        KeyCode::Char('l') => {
                            if let Some(item) = app.library.list.get_selected() {
                                tx.send(MessageFromUi::GetLibraryNode(item.uuid.clone()));
                            }
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

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(size);

    let library_items: Vec<ListItem> = app
        .library
        .list
        .items
        .iter()
        // FIXME: why to_string() ??
        .map(|i| ListItem::new(Span::from(i.title.to_string())))
        .collect();

    let library_list = List::new(library_items)
        .block(Block::default().borders(Borders::ALL).title(app.library.title.clone()))
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(library_list, main[0], &mut app.library.list.state);

    let now_playing = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Max(10)].as_ref())
        .split(main[1]);

    let queue_items: Vec<ListItem> = app
        .queue
        .items
        .iter()
        // FIXME: why to_string() ??
        .map(|i| ListItem::new(Span::from(i.title.to_string())))
        .collect();

    let queue_list = List::new(queue_items)
        .block(Block::default().borders(Borders::ALL).title("Queue"))
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(queue_list, now_playing[0], &mut app.queue.state);

    let media_info = Block::default()
        .title("Now playing")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    f.render_widget(media_info, now_playing[1]);
}
