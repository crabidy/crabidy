mod rpc;

use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, get_queue_updates_response::QueueUpdateResult,
    ActiveTrack, GetLibraryNodeRequest, GetQueueUpdatesRequest, GetQueueUpdatesResponse,
    GetTrackUpdatesResponse, LibraryNode, LibraryNodeState, Queue,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        ModifierKeyCode,
    },
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
    cell::RefCell,
    collections::HashMap,
    error::Error,
    fmt, io, println, thread,
    time::{Duration, Instant},
    vec,
};
use tokio::{fs, select, signal, task};
use tokio_stream::StreamExt;
use tonic::{transport::Channel, Request, Status, Streaming};

trait ListView {
    fn get_size(&self) -> usize;
    fn select(&mut self, idx: Option<usize>);
    fn selected(&self) -> Option<usize>;
    fn prev_selected(&self) -> usize;

    fn next(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let next = if i == self.get_size() - 1 { 0 } else { i + 1 };
            self.select(Some(next));
        } else {
            self.select(Some(0));
        }
    }

    fn prev(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let prev = if i == 0 { self.get_size() - 1 } else { i - 1 };
            self.select(Some(prev));
        } else {
            self.select(Some(0));
        }
    }

    fn down(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let next = if i < self.get_size() - 15 {
                i + 15
            } else {
                self.get_size() - 1
            };
            self.select(Some(next));
        } else {
            self.select(Some(0));
        }
    }

    fn up(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let prev = if i < 15 { 0 } else { i - 15 };
            self.select(Some(prev));
        } else {
            self.select(Some(0));
        }
    }

    fn is_selected(&self) -> bool {
        self.selected().is_some()
    }

    fn is_empty(&self) -> bool {
        self.get_size() == 0
    }
}

#[derive(Clone, Copy)]
enum UiFocus {
    Library,
    Queue,
}

#[derive(Clone, Copy)]
enum UiItemKind {
    Node,
    Track,
}

struct UiItem {
    active: bool,
    uuid: String,
    title: String,
    kind: UiItemKind,
}

struct QueueView {
    list: Vec<UiItem>,
    list_state: ListState,
    prev_selected: usize,
}

impl ListView for QueueView {
    fn get_size(&self) -> usize {
        self.list.len()
    }

    fn select(&mut self, idx: Option<usize>) {
        if let Some(pos) = idx {
            self.prev_selected = pos;
        }
        self.list_state.select(idx);
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn prev_selected(&self) -> usize {
        self.prev_selected
    }
}

impl QueueView {
    fn check_focus(&mut self, focus: UiFocus) {
        if !self.is_selected() && matches!(focus, UiFocus::Queue) {
            self.select(Some(self.prev_selected()));
        } else if self.is_selected() && !matches!(focus, UiFocus::Queue) {
            self.select(None);
        }
    }
    fn skip(&self, tx: &Sender<MessageFromUi>) {
        if let Some(pos) = self.selected() {
            if pos < self.get_size() - 1 {
                tx.send(MessageFromUi::SetCurrentTrack(pos + 1));
            }
        }
    }
    fn play_selected(&self, tx: &Sender<MessageFromUi>) {
        if let Some(pos) = self.selected() {
            tx.send(MessageFromUi::SetCurrentTrack(pos));
        }
    }
    fn update(&mut self, queue: Queue) {
        self.list = queue
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| UiItem {
                uuid: t.uuid.clone(),
                title: format!("{} - {}", t.artist, t.title),
                kind: UiItemKind::Track,
                active: i == queue.current as usize,
            })
            .collect();
    }
}

struct LibraryView {
    title: String,
    uuid: String,
    list: Vec<UiItem>,
    list_state: ListState,
    parent: Option<String>,
    positions: HashMap<String, usize>,
}

impl ListView for LibraryView {
    fn get_size(&self) -> usize {
        self.list.len()
    }

    fn select(&mut self, idx: Option<usize>) {
        if let Some(pos) = idx {
            self.positions
                .entry(self.uuid.clone())
                .and_modify(|e| *e = pos)
                .or_insert(pos);
        }
        self.list_state.select(idx);
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn prev_selected(&self) -> usize {
        *self.positions.get(&self.uuid).unwrap_or(&0)
    }
}

impl LibraryView {
    fn check_focus(&mut self, focus: UiFocus) {
        if !self.is_selected() && matches!(focus, UiFocus::Library) {
            self.select(Some(self.prev_selected()));
        } else if self.is_selected() && !matches!(focus, UiFocus::Library) {
            self.select(None);
        }
    }
    fn get_selected(&self) -> Option<&UiItem> {
        if let Some(idx) = self.list_state.selected() {
            return Some(&self.list[idx]);
        }
        None
    }
    fn ascend(&mut self, tx: &Sender<MessageFromUi>) {
        if let Some(parent) = self.parent.as_ref() {
            tx.send(MessageFromUi::GetLibraryNode(parent.clone()));
        }
    }
    fn dive(&mut self, tx: &Sender<MessageFromUi>) {
        if let Some(item) = self.get_selected() {
            if let UiItemKind::Node = item.kind {
                tx.send(MessageFromUi::GetLibraryNode(item.uuid.clone()));
            }
        }
    }
    fn queue_replace_with_item(&mut self, tx: &Sender<MessageFromUi>) {
        if let Some(item) = self.get_selected() {
            tx.send(MessageFromUi::ReplaceWithItem(item.uuid.clone(), item.kind));
        }
    }
    fn update(&mut self, node: LibraryNode) {
        if node.tracks.is_empty() && node.children.is_empty() {
            return;
        }

        // if children empty and tracks empty return
        self.uuid = node.uuid;
        self.title = node.title;
        self.parent = node.parent;
        self.select(Some(self.prev_selected()));

        if !node.tracks.is_empty() {
            self.list = node
                .tracks
                .iter()
                .map(|t| UiItem {
                    uuid: t.uuid.clone(),
                    title: format!("{} - {}", t.artist, t.title),
                    kind: UiItemKind::Track,
                    active: false,
                })
                .collect();
        } else {
            // if tracks not empty use tracks instead
            self.list = node
                .children
                .iter()
                .map(|c| UiItem {
                    uuid: c.uuid.clone(),
                    title: c.title.clone(),
                    kind: UiItemKind::Node,
                    active: false,
                })
                .collect();
        }
    }
}

struct NowPlayingView {
    text: String,
}

impl NowPlayingView {
    fn update(&mut self, active_track: ActiveTrack) {
        if let Some(track_info) = active_track.track {
            self.text = format!("Playing {} - {}", track_info.title, active_track.play_state);
        }
    }
}

struct App {
    focus: UiFocus,
    library: LibraryView,
    now_playing: NowPlayingView,
    queue: QueueView,
}

impl App {
    fn new() -> App {
        let mut library = LibraryView {
            title: "Library".to_string(),
            uuid: "/".to_string(),
            list: Vec::new(),
            list_state: ListState::default(),
            positions: HashMap::new(),
            parent: None,
        };
        let queue = QueueView {
            list: Vec::new(),
            list_state: ListState::default(),
            prev_selected: 0,
        };
        let now_playing = NowPlayingView {
            text: "Not playing".to_string(),
        };
        App {
            focus: UiFocus::Library,
            library,
            now_playing,
            queue,
        }
    }

    fn check_focus(&mut self) {
        self.library.check_focus(self.focus);
        self.queue.check_focus(self.focus);
    }

    fn cycle_active(&mut self) {
        self.focus = match (self.focus, self.queue.is_empty()) {
            (UiFocus::Library, false) => UiFocus::Queue,
            (UiFocus::Library, true) => UiFocus::Library,
            (UiFocus::Queue, _) => UiFocus::Library,
        };
    }
}

// FIXME: Rename this
enum MessageToUi {
    ReplaceLibraryNode(LibraryNode),
    QueueStreamUpdate(QueueUpdateResult),
    TrackStreamUpdate(ActiveTrack),
}

// FIXME: Rename this
enum MessageFromUi {
    GetLibraryNode(String),
    ReplaceWithItem(String, UiItemKind),
    SetCurrentTrack(usize),
    TogglePlay,
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
                MessageFromUi::ReplaceWithItem(uuid, kind) => {
                    match kind {
                        UiItemKind::Node => {
                            rpc_client.replace_queue_with_node(&uuid).await?
                        }
                        UiItemKind::Track => {
                            rpc_client.replace_queue_with_track(&uuid).await?
                        }
                    }
                }
                MessageFromUi::TogglePlay => {
                    rpc_client.toggle_play().await?
                }
                MessageFromUi::SetCurrentTrack(pos) => {
                    rpc_client.set_current_track(pos).await?
                }

            }
        }
        Some(resp) = rpc_client.queue_updates_stream.next() => {
            match resp {
                Ok(resp) => {
                    if let Some(res) = resp.queue_update_result {
                        tx.send_async(MessageToUi::QueueStreamUpdate(res)).await?;
                    }
                }
                Err(_) => {
                    rpc_client.reconnect_queue_updates_stream().await;
                }

            }
        }
        Some(resp) = rpc_client.track_updates_stream.next() => {
            match resp {
                Ok(resp) => {
                    if let Some(active_track) = resp.active_track {
                        tx.send_async(MessageToUi::TrackStreamUpdate(active_track)).await?;
                    }
                }
                Err(_) => {
                    rpc_client.reconnect_track_updates_stream().await;
                }
            }
        }
    }

    Ok(())
}

async fn orchestrate<'a>(
    (tx, rx): (Sender<MessageToUi>, Receiver<MessageFromUi>),
) -> Result<(), Box<dyn Error>> {
    let mut rpc_client = rpc::RpcClient::connect("http://127.0.0.1:50051").await?;

    if let Some(root_node) = rpc_client.get_library_node("/").await? {
        tx.send(MessageToUi::ReplaceLibraryNode(root_node.clone()));
    }

    loop {
        poll(&mut rpc_client, &rx, &tx).await.ok();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui_tx, rx): (Sender<MessageFromUi>, Receiver<MessageFromUi>) = flume::unbounded();
    let (tx, ui_rx): (Sender<MessageToUi>, Receiver<MessageToUi>) = flume::unbounded();

    // FIXME: unwrap
    tokio::spawn(async move { orchestrate((tx, rx)).await.ok() });

    tokio::task::spawn_blocking(|| {
        run_ui(ui_tx, ui_rx);
    }).await;

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
                    QueueUpdateResult::Full(queue) => {
                        app.queue.update(queue);
                    }
                    QueueUpdateResult::PositionChange(pos) => {}
                },
                MessageToUi::TrackStreamUpdate(active_track) => {
                    app.now_playing.update(active_track);
                }
            }
        }

        terminal.draw(|f| ui(f, &mut app)).unwrap();

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout).unwrap() {
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
                        (_, KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                            app.queue.skip(&tx);
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
                            app.library.ascend(&tx);
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('l')) => {
                            app.library.dive(&tx);
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.library.queue_replace_with_item(&tx);
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
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.queue.play_selected(&tx);
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.check_focus();
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
        .iter()
        .map(|i| ListItem::new(Span::from(i.title.to_string())))
        .collect();

    let library_list = List::new(library_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.library.title.clone()),
        )
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(library_list, main[0], &mut app.library.list_state);

    let now_playing = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Max(10)].as_ref())
        .split(main[1]);

    let queue_items: Vec<ListItem> = app
        .queue
        .list
        .iter()
        .map(|i| {
            let color = if i.active { Color::Red } else { Color::Reset };
            ListItem::new(Span::from(i.title.to_string())).style(Style::default().fg(color))
        })
        .collect();

    let queue_list = List::new(queue_items)
        .block(Block::default().borders(Borders::ALL).title("Queue"))
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(queue_list, now_playing[0], &mut app.queue.list_state);

    let media_info = Block::default()
        .title("Now playing")
        .borders(Borders::ALL)
        .style(Style::default());

    let now_playing_text = Paragraph::new(app.now_playing.text.to_string())
        .block(media_info)
        .alignment(Alignment::Center);
    // f.render_widget(media_info, now_playing[1]);
    f.render_widget(now_playing_text, now_playing[1]);
}
