use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, GetLibraryNodeRequest,
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
use std::{
    error::Error,
    io, println, thread,
    time::{Duration, Instant},
    vec,
};
use tonic::Request;

use tokio::task;

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
}

struct ListNode {
    name: String,
    children: Option<Vec<ListNode>>,
}

struct App {
    library: StatefulList<ListNode>,
    queue: StatefulList<ListNode>,
}

impl App {
    fn new() -> App {
        let mut library = StatefulList::default();
        library.items.push(ListNode {
            name: "Tidal".to_owned(),
            children: None,
        });
        library.items.push(ListNode {
            name: "YouTube".to_owned(),
            children: None,
        });
        library.focus();
        let mut queue = StatefulList::default();
        queue.items.push(ListNode {
            name: "GLaDOS - Still Alive".to_owned(),
            children: None,
        });
        queue.items.push(ListNode {
            name: "Floppotron - Star Wars Theme".to_owned(),
            children: None,
        });
        queue.items.push(ListNode {
            name: "Starcop - Starcop".to_owned(),
            children: None,
        });
        queue.items.push(ListNode {
            name: "Efence - Airglow".to_owned(),
            children: None,
        });
        App { library, queue }
    }

    fn cycle_active(&mut self) {
        if self.library.is_focused() {
            self.library.blur();
            self.queue.focus();
        } else {
            self.library.focus();
            self.queue.blur();
        }
    }
}

enum Message {
    Quit,
    LibraryData(String),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui_tx, rx): (Sender<Message>, Receiver<Message>) = flume::unbounded();
    let (tx, ui_rx): (Sender<Message>, Receiver<Message>) = flume::unbounded();

    thread::spawn(|| {
        run_ui(ui_tx, ui_rx);
    });

    let mut client = CrabidyServiceClient::connect("http://[::1]:50051").await?;

    let request = Request::new(GetLibraryNodeRequest {
        uuid: "/".to_string(),
    });

    let response = client.get_library_node(request).await?;

    if let Some(node) = response.into_inner().node {
        node.children.iter().for_each(|c| {
            tx.send(Message::LibraryData(c.to_string()));
        })
    }

    loop {
        match rx.recv() {
            Ok(Message::Quit) => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn run_ui(tx: Sender<Message>, rx: Receiver<Message>) {
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
            if let Message::LibraryData(title) = message {
                app.library.items.push(ListNode {
                    name: title,
                    children: None,
                });
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
                            tx.send(Message::Quit);
                            break;
                        }
                        KeyCode::Char('j') => {
                            if app.library.is_focused() {
                                app.library.next();
                            } else {
                                app.queue.next();
                            }
                        }
                        KeyCode::Char('k') => {
                            if app.library.is_focused() {
                                app.library.prev();
                            } else {
                                app.queue.prev();
                            }
                        }
                        KeyCode::Tab => app.cycle_active(),
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
        .items
        .iter()
        // FIXME: why to_string() ??
        .map(|i| ListItem::new(Span::from(i.name.to_string())))
        .collect();

    let library_list = List::new(library_items)
        .block(Block::default().borders(Borders::ALL).title("Library"))
        .highlight_style(
            Style::default()
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(library_list, main[0], &mut app.library.state);

    let now_playing = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Max(10)].as_ref())
        .split(main[1]);

    let queue_items: Vec<ListItem> = app
        .queue
        .items
        .iter()
        // FIXME: why to_string() ??
        .map(|i| ListItem::new(Span::from(i.name.to_string())))
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
