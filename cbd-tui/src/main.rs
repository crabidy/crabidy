mod graphql;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
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
    io, println,
    time::{Duration, Instant},
    vec,
};

use cynic::{http::ReqwestExt, QueryBuilder};
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

#[tokio::main]
async fn main() {
    task::spawn_blocking(run_ui).await.unwrap();
}

fn run_ui() {
    // let operation = graphql::queries::AllFilmsQuery::build(());
    //
    // let client = reqwest::Client::new();
    //
    // let response = client
    //     .post("https://swapi-graphql.netlify.app/.netlify/functions/index")
    //     .run_graphql(operation)
    //     .await
    //     .unwrap();
    //
    // if let Some(data) = response.data {
    //     if let Some(films) = data.all_films {
    //         if let Some(list) = films.films {
    //             list.iter().for_each(|mut f| {
    //                 if let Some(film) = f {
    //                     if let Some(title) = &film.title {
    //                         app.library.items.push(ListNode {
    //                             name: title.to_string(),
    //                             children: None,
    //                         })
    //                     }
    //                 }
    //             })
    //         }
    //     }
    // }

    // setup terminal
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    // create app and run it
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &mut app)).unwrap();

        if let Event::Key(key) = event::read().unwrap() {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => break,
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
