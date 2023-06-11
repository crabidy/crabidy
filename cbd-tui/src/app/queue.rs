use flume::{Receiver, Sender};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Corner, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Gauge, LineGauge, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame,
};

use crabidy_core::proto::crabidy::Queue as QueueData;

use super::{
    MessageFromUi, StatefulList, UiItem, UiItemKind, COLOR_PRIMARY, COLOR_PRIMARY_DARK, COLOR_RED,
};

pub struct Queue {
    current_position: usize,
    list: Vec<UiItem>,
    list_state: ListState,
    tx: Sender<MessageFromUi>,
}

impl Queue {
    pub fn new(tx: Sender<MessageFromUi>) -> Self {
        Self {
            current_position: 0,
            list: Vec::new(),
            list_state: ListState::default(),
            tx,
        }
    }
    pub fn play_next(&self) {
        self.tx.send(MessageFromUi::NextTrack);
    }
    pub fn play_prev(&self) {
        self.tx.send(MessageFromUi::PrevTrack);
    }
    pub fn play_selected(&self) {
        if let Some(pos) = self.selected() {
            self.tx.send(MessageFromUi::SetCurrentTrack(pos));
        }
    }
    pub fn remove_track(&mut self) {
        if let Some(pos) = self.selected() {
            // FIXME: mark multiple tracks on queue and remove them
            self.tx.send(MessageFromUi::RemoveTracks(vec![pos]));
        }
    }
    pub fn update_position(&mut self, pos: usize) {
        self.current_position = pos;
    }
    pub fn update_queue(&mut self, queue: QueueData) {
        self.current_position = queue.current_position as usize;
        self.list = queue
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| UiItem {
                uuid: t.uuid.clone(),
                title: format!("{} - {}", t.artist, t.title),
                kind: UiItemKind::Track,
                marked: false,
                is_queable: false,
            })
            .collect();

        self.update_selection();
    }

    pub fn render<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, focused: bool) {
        let queue_items: Vec<ListItem> = self
            .list
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let active = idx == self.current_position;

                let title = if active {
                    format!("> {}", item.title)
                } else {
                    item.title.to_string()
                };
                let style = if active {
                    Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD)
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
                    .border_style(Style::default().fg(if focused {
                        COLOR_PRIMARY
                    } else {
                        COLOR_PRIMARY_DARK
                    }))
                    .title("Queue"),
            )
            .highlight_style(Style::default().bg(if focused {
                COLOR_PRIMARY
            } else {
                COLOR_PRIMARY_DARK
            }));

        f.render_stateful_widget(queue_list, area, &mut self.list_state);
    }
}

impl StatefulList for Queue {
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
