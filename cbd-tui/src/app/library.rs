use std::collections::HashMap;

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

use crabidy_core::proto::crabidy::LibraryNode;

use super::{
    MessageFromUi, StatefulList, UiItem, UiItemKind, COLOR_GREEN, COLOR_PRIMARY, COLOR_PRIMARY_DARK,
};

pub struct Library {
    title: String,
    uuid: String,
    list: Vec<UiItem>,
    list_state: ListState,
    parent: Option<String>,
    positions: HashMap<String, usize>,
    tx: Sender<MessageFromUi>,
}

impl Library {
    pub fn new(tx: Sender<MessageFromUi>) -> Self {
        Self {
            title: "Library".to_string(),
            uuid: "node:/".to_string(),
            list: Vec::new(),
            list_state: ListState::default(),
            positions: HashMap::new(),
            parent: None,
            tx,
        }
    }
    pub fn get_selected(&self) -> Option<Vec<String>> {
        if self.list.iter().any(|i| i.marked) {
            return Some(
                self.list
                    .iter()
                    .filter(|i| i.marked)
                    .map(|i| i.uuid.to_string())
                    .collect(),
            );
        }
        if let Some(idx) = self.list_state.selected() {
            return Some(vec![self.list[idx].uuid.to_string()]);
        }
        None
    }
    pub fn ascend(&mut self) {
        if let Some(parent) = self.parent.as_ref() {
            self.tx.send(MessageFromUi::GetLibraryNode(parent.clone()));
        }
    }
    pub fn dive(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            let item = &self.list[idx];
            if let UiItemKind::Node = item.kind {
                self.tx
                    .send(MessageFromUi::GetLibraryNode(item.uuid.clone()));
            }
        }
    }
    pub fn queue_append(&mut self) {
        if let Some(items) = self.get_selected() {
            match self.tx.send(MessageFromUi::AppendTracks(items)) {
                Ok(_) => self.remove_marks(),
                Err(_) => { /* FIXME: warn */ }
            }
        }
    }
    pub fn queue_queue(&mut self) {
        if let Some(items) = self.get_selected() {
            match self.tx.send(MessageFromUi::QueueTracks(items)) {
                Ok(_) => self.remove_marks(),
                Err(_) => { /* FIXME: warn */ }
            }
        }
    }
    pub fn queue_replace(&mut self) {
        if let Some(items) = self.get_selected() {
            match self.tx.send(MessageFromUi::ReplaceQueue(items)) {
                Ok(_) => self.remove_marks(),
                Err(_) => { /* FIXME: warn */ }
            }
        }
    }
    pub fn queue_insert(&mut self, pos: usize) {
        if let Some(items) = self.get_selected() {
            match self.tx.send(MessageFromUi::InsertTracks(items, pos)) {
                Ok(_) => self.remove_marks(),
                Err(_) => { /* FIXME: warn */ }
            }
        }
    }
    pub fn prev_selected(&self) -> usize {
        *self.positions.get(&self.uuid).unwrap_or(&0)
    }
    pub fn toggle_mark(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            let mut item = &mut self.list[idx];
            if !item.is_queable {
                return;
            }
            item.marked = !item.marked;
        }
    }
    pub fn remove_marks(&mut self) {
        if self.list.iter().any(|i| i.marked) {
            self.list
                .iter_mut()
                .filter(|i| i.marked)
                .for_each(|i| i.marked = false);
        }
    }
    pub fn update(&mut self, node: LibraryNode) {
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
                    marked: false,
                    is_queable: true,
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
                    marked: false,
                    is_queable: c.is_queable,
                })
                .collect();
        }

        self.update_selection();
    }

    pub fn render<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, focused: bool) {
        let library_items: Vec<ListItem> = self
            .list
            .iter()
            .map(|i| {
                let text = if i.marked {
                    format!("* {}", i.title)
                } else {
                    i.title.to_string()
                };
                let style = if i.marked {
                    Style::default()
                        .fg(COLOR_GREEN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                return ListItem::new(Span::from(text)).style(style);
            })
            .collect();

        let library_list = List::new(library_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(if focused {
                        COLOR_PRIMARY
                    } else {
                        COLOR_PRIMARY_DARK
                    }))
                    .title(self.title.clone()),
            )
            .highlight_style(
                Style::default()
                    .bg(if focused {
                        COLOR_PRIMARY
                    } else {
                        COLOR_PRIMARY_DARK
                    })
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(library_list, area, &mut self.list_state);
    }
}

impl StatefulList for Library {
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
