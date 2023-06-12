mod library;
mod list;
mod now_playing;
mod queue;

use flume::Sender;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::Color,
    Frame,
};

use crabidy_core::proto::crabidy::{
    get_update_stream_response::Update as StreamUpdate, InitResponse as InitialData, LibraryNode,
};

pub use list::StatefulList;

use library::Library;
use now_playing::NowPlaying;
use queue::Queue;

#[derive(Clone, Copy)]
pub enum UiFocus {
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
    marked: bool,
    is_queable: bool,
}

pub const COLOR_PRIMARY: Color = Color::Rgb(129, 161, 193);
// const COLOR_PRIMARY_DARK: Color = Color::Rgb(94, 129, 172);
pub const COLOR_PRIMARY_DARK: Color = Color::Rgb(59, 66, 82);
pub const COLOR_SECONDARY: Color = Color::Rgb(180, 142, 173);
pub const COLOR_RED: Color = Color::Rgb(191, 97, 106);
pub const COLOR_GREEN: Color = Color::Rgb(163, 190, 140);
// const COLOR_ORANGE: Color = Color::Rgb(208, 135, 112);
// const COLOR_BRIGHT: Color = Color::Rgb(216, 222, 233);

// FIXME: Rename this
pub enum MessageToUi {
    Init(InitialData),
    ReplaceLibraryNode(LibraryNode),
    Update(StreamUpdate),
}

// FIXME: Rename this
pub enum MessageFromUi {
    GetLibraryNode(String),
    AppendTracks(Vec<String>),
    QueueTracks(Vec<String>),
    InsertTracks(Vec<String>, usize),
    RemoveTracks(Vec<usize>),
    ReplaceQueue(Vec<String>),
    ClearQueue(bool),
    NextTrack,
    PrevTrack,
    RestartTrack,
    SetCurrentTrack(usize),
    TogglePlay,
    ChangeVolume(f32),
    ToggleMute,
    ToggleShuffle,
    ToggleRepeat,
}

pub struct App {
    pub focus: UiFocus,
    pub library: Library,
    pub now_playing: NowPlaying,
    pub queue: Queue,
}

impl App {
    pub fn new(tx: Sender<MessageFromUi>) -> App {
        let library = Library::new(tx.clone());
        let queue = Queue::new(tx);
        let now_playing = NowPlaying::default();
        App {
            focus: UiFocus::Library,
            library,
            now_playing,
            queue,
        }
    }

    pub fn cycle_active(&mut self) {
        self.focus = match (self.focus, self.queue.is_empty()) {
            (UiFocus::Library, false) => UiFocus::Queue,
            (UiFocus::Library, true) => UiFocus::Library,
            (UiFocus::Queue, _) => UiFocus::Library,
        };
    }

    pub fn render<B: Backend>(&mut self, f: &mut Frame<B>) {
        let full_screen = f.size();

        let library_focused = matches!(self.focus, UiFocus::Library);
        let queue_focused = matches!(self.focus, UiFocus::Queue);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(f.size());

        self.library.render(f, main[0], library_focused);

        let right_side = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Max(10)].as_ref())
            .split(main[1]);

        self.queue.render(f, right_side[0], queue_focused);
        self.now_playing.render(f, right_side[1]);
    }
}
