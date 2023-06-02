mod rpc;

use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient,
    get_update_stream_response::Update as StreamUpdate, GetLibraryNodeRequest,
    InitResponse as InitialData, LibraryNode, PlayState, Queue, QueueTrack, Track, TrackPosition,
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
    widgets::{
        Block, BorderType, Borders, Gauge, LineGauge, List, ListItem, ListState, Paragraph, Wrap,
    },
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

use notify_rust::Notification;

const COLOR_PRIMARY: Color = Color::Rgb(129, 161, 193);
// const COLOR_PRIMARY_DARK: Color = Color::Rgb(94, 129, 172);
const COLOR_PRIMARY_DARK: Color = Color::Rgb(59, 66, 82);
const COLOR_SECONDARY: Color = Color::Rgb(180, 142, 173);
// const COLOR_RED: Color = Color::Rgb(191, 97, 106);
// const COLOR_BRIGHT: Color = Color::Rgb(216, 222, 233);

trait ListView {
    fn get_size(&self) -> usize;
    fn select(&mut self, idx: Option<usize>);
    fn selected(&self) -> Option<usize>;

    fn first(&mut self) {
        if self.is_empty() {
            return;
        }
        self.select(Some(0));
    }

    fn last(&mut self) {
        if self.is_empty() {
            return;
        }
        self.select(Some(self.get_size() - 1));
    }

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
            let next = if i < self.get_size().saturating_sub(15) {
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
            let prev = if i < 15 { 0 } else { i.saturating_sub(15) };
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

    fn update_selection(&mut self) {
        if self.is_empty() {
            self.select(None);
            return;
        }
        match self.selected() {
            None => {
                self.select(Some(0));
            }
            Some(selected) => {
                if selected > self.get_size().saturating_sub(1) {
                    self.select(Some(0));
                }
            }
        }
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
    uuid: String,
    title: String,
    kind: UiItemKind,
}

struct QueueView {
    // FIXME: implement skip on server, remove current
    current_position: usize,
    list: Vec<UiItem>,
    list_state: ListState,
}

impl ListView for QueueView {
    fn get_size(&self) -> usize {
        self.list.len()
    }

    fn select(&mut self, idx: Option<usize>) {
        self.list_state.select(idx);
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }
}

impl QueueView {
    fn skip(&self, tx: &Sender<MessageFromUi>) {
        if self.current_position < self.get_size() - 1 {
            tx.send(MessageFromUi::Next);
        }
    }
    fn play_selected(&self, tx: &Sender<MessageFromUi>) {
        if let Some(pos) = self.selected() {
            tx.send(MessageFromUi::SetCurrentTrack(pos));
        }
    }
    fn update_position(&mut self, pos: usize) {
        self.current_position = pos;
    }
    fn update_queue(&mut self, queue: Queue) {
        self.current_position = queue.current_position as usize;
        self.list = queue
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| UiItem {
                uuid: t.uuid.clone(),
                title: format!("{} - {}", t.artist, t.title),
                kind: UiItemKind::Track,
            })
            .collect();

        self.update_selection();
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
}

impl LibraryView {
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
            tx.send(MessageFromUi::ReplaceQueue(item.uuid.clone()));
        }
    }
    fn prev_selected(&self) -> usize {
        *self.positions.get(&self.uuid).unwrap_or(&0)
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
                })
                .collect();
        }

        self.update_selection();
    }
}

struct NowPlayingView {
    completion: Option<u32>,
    elapsed: Option<f64>,
    play_state: PlayState,
    track: Option<Track>,
}

impl NowPlayingView {
    fn update_play_state(&mut self, play_state: PlayState) {
        self.play_state = play_state;
    }
    fn update_position(&mut self, pos: TrackPosition) {}
    fn update_track(&mut self, active: Option<Track>) {
        if let Some(track) = &active {
            Notification::new()
                .summary("Crabidy playing")
                // FIXME: album
                .body(&format!("{} by {}", track.title, track.artist))
                .show()
                .unwrap();
        }
        self.track = active;
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
            uuid: "node:/".to_string(),
            list: Vec::new(),
            list_state: ListState::default(),
            positions: HashMap::new(),
            parent: None,
        };
        let queue = QueueView {
            current_position: 0,
            list: Vec::new(),
            list_state: ListState::default(),
        };
        let now_playing = NowPlayingView {
            completion: None,
            elapsed: None,
            play_state: PlayState::Unspecified,
            track: None,
        };
        App {
            focus: UiFocus::Library,
            library,
            now_playing,
            queue,
        }
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
    Init(InitialData),
    ReplaceLibraryNode(LibraryNode),
    Update(StreamUpdate),
}

// FIXME: Rename this
enum MessageFromUi {
    GetLibraryNode(String),
    Next,
    ReplaceQueue(String),
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
                MessageFromUi::ReplaceQueue(uuid) => {
                    rpc_client.replace_queue(&uuid).await?
                }
                MessageFromUi::TogglePlay => {
                    rpc_client.toggle_play().await?
                }
                MessageFromUi::SetCurrentTrack(pos) => {
                    rpc_client.set_current_track(pos).await?
                }
                MessageFromUi::Next => {
                    // FIXME:
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

async fn orchestrate<'a>(
    (tx, rx): (Sender<MessageToUi>, Receiver<MessageFromUi>),
) -> Result<(), Box<dyn Error>> {
    let mut rpc_client = rpc::RpcClient::connect("http://localhost:50051").await?;

    if let Some(root_node) = rpc_client.get_library_node("node:/").await? {
        tx.send(MessageToUi::ReplaceLibraryNode(root_node.clone()));
    }

    let init_data = rpc_client.init().await?;
    tx.send_async(MessageToUi::Init(init_data)).await?;

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
    })
    .await;

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
                    _ => {}
                },
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
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('g')) => {
                            app.library.first();
                        }
                        (UiFocus::Library, KeyModifiers::SHIFT, KeyCode::Char('g')) => {
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
                            app.library.ascend(&tx);
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Char('l')) => {
                            app.library.dive(&tx);
                        }
                        (UiFocus::Library, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.library.queue_replace_with_item(&tx);
                        }
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Char('g')) => {
                            app.queue.first();
                        }
                        (UiFocus::Queue, KeyModifiers::SHIFT, KeyCode::Char('g')) => {
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
                        (UiFocus::Queue, KeyModifiers::NONE, KeyCode::Enter) => {
                            app.queue.play_selected(&tx);
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

    let library_focused = matches!(app.focus, UiFocus::Library);
    let queue_focused = matches!(app.focus, UiFocus::Queue);

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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if library_focused {
                    COLOR_PRIMARY
                } else {
                    COLOR_PRIMARY_DARK
                }))
                .title(app.library.title.clone()),
        )
        .highlight_style(
            Style::default()
                .bg(if library_focused {
                    COLOR_PRIMARY
                } else {
                    COLOR_PRIMARY_DARK
                })
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(library_list, main[0], &mut app.library.list_state);

    let right_side = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Max(10)].as_ref())
        .split(main[1]);

    let queue_items: Vec<ListItem> = app
        .queue
        .list
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let active = idx == app.queue.current_position;
            let title = if active {
                format!("> {}", item.title)
            } else {
                item.title.to_string()
            };
            let style = if active {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Span::from(title)).style(style)
        })
        .collect();

    let queue_list = List::new(queue_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if queue_focused {
                    COLOR_PRIMARY
                } else {
                    COLOR_PRIMARY_DARK
                }))
                .title("Queue"),
        )
        .highlight_style(Style::default().bg(if queue_focused {
            COLOR_PRIMARY
        } else {
            COLOR_PRIMARY_DARK
        }));

    f.render_stateful_widget(queue_list, right_side[0], &mut app.queue.list_state);

    let now_playing_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Max(8), Constraint::Max(1)])
        .split(right_side[1]);

    let media_info_text = if let Some(track) = &app.now_playing.track {
        let play_text = match &app.now_playing.play_state {
            PlayState::Loading => "▼",
            PlayState::Paused => "■",
            PlayState::Playing => "♫",
            _ => "",
        };
        let album_text = match &track.album {
            Some(album) => album.title.to_string(),
            None => "No album".to_string(),
        };
        vec![
            Spans::from(Span::raw("")),
            Spans::from(Span::raw(play_text)),
            Spans::from(vec![
                Span::styled(
                    track.title.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" by "),
                Span::styled(
                    track.artist.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Spans::from(Span::raw(album_text)),
        ]
    } else {
        vec![
            Spans::from(Span::raw("")),
            Spans::from(Span::raw("")),
            Spans::from(Span::raw("No track playing")),
        ]
    };

    let media_info_p = Paragraph::new(media_info_text)
        .block(
            Block::default()
                .title("Now playing")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(COLOR_SECONDARY)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(media_info_p, now_playing_layout[0]);

    let elapsed_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Max(15)])
        .split(now_playing_layout[1]);

    if let (Some(elapsed), Some(track)) = (app.now_playing.elapsed, &app.now_playing.track) {
        let progress = LineGauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(COLOR_SECONDARY).bg(Color::Black))
            .ratio(elapsed);
        f.render_widget(progress, elapsed_layout[0]);

        let c = app.now_playing.completion.unwrap_or(0);
        let l = track.duration.unwrap_or(0);
        let completion = format!(
            "{:0>1}:{:0>2}:{:0>2}/{:0>1}:{:0>2}:{:0>2}",
            (c / 60 / 60),
            (c / 60) % 60,
            c % 60,
            (l / 60 / 60),
            (l / 60) % 60,
            l % 60
        );

        let time_text = Span::raw(completion);
        let time_p = Paragraph::new(Spans::from(time_text));
        f.render_widget(time_p, elapsed_layout[1]);
    }
}
