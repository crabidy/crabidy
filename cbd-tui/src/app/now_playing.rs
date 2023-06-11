use std::{ops::Div, time::Duration};

use notify_rust::Notification;

use crabidy_core::proto::crabidy::{PlayState, QueueModifiers, Track, TrackPosition};

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

use super::COLOR_SECONDARY;

pub struct NowPlaying {
    play_state: PlayState,
    duration: Option<Duration>,
    modifiers: QueueModifiers,
    position: Option<Duration>,
    track: Option<Track>,
}

impl Default for NowPlaying {
    fn default() -> Self {
        NowPlaying {
            play_state: PlayState::Unspecified,
            duration: None,
            modifiers: QueueModifiers::default(),
            position: None,
            track: None,
        }
    }
}

impl NowPlaying {
    pub fn update_play_state(&mut self, play_state: PlayState) {
        self.play_state = play_state;
    }
    pub fn update_position(&mut self, pos: TrackPosition) {
        self.position = Some(Duration::from_millis(pos.position.into()));
        self.duration = Some(Duration::from_millis(pos.duration.into()));
    }
    pub fn update_track(&mut self, active: Option<Track>) {
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
    pub fn update_modifiers(&mut self, mods: &QueueModifiers) {
        self.modifiers = mods.clone();
    }

    pub fn render<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let now_playing_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Max(8), Constraint::Max(1)])
            .split(area);

        let media_info_text = if let Some(track) = &self.track {
            let play_text = match self.play_state {
                PlayState::Loading => "▼",
                PlayState::Paused => "■",
                PlayState::Playing => "♫",
                _ => "",
            };
            let album_text = match &track.album {
                Some(album) => album.title.to_string(),
                None => "No album".to_string(),
            };
            let mods = format!(
                "Shuffle: {}, Repeat {}",
                self.modifiers.shuffle, self.modifiers.repeat
            );
            vec![
                Spans::from(Span::raw(mods)),
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

        if let (Some(position), Some(duration), Some(track)) =
            (self.position, self.duration, &self.track)
        {
            let pos = position.as_secs();
            let dur = duration.as_secs();

            let completion_size = if dur < 3600 { 12 } else { 15 };

            let elapsed_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(10), Constraint::Max(completion_size)])
                .split(now_playing_layout[1]);

            let ratio = if duration.is_zero() {
                0.0
            } else {
                position.as_secs_f64().div(duration.as_secs_f64())
            };

            let progress = LineGauge::default()
                .label("")
                .block(Block::default().borders(Borders::NONE))
                .gauge_style(Style::default().fg(COLOR_SECONDARY).bg(Color::Black))
                .ratio(ratio);
            f.render_widget(progress, elapsed_layout[0]);

            let pos_min = (pos / 60) % 60;
            let pos_secs = pos % 60;
            let dur_min = (dur / 60) % 60;
            let dur_secs = dur % 60;

            let completion_text = if dur < 3600 {
                format!(
                    "{:0>2}:{:0>2}/{:0>2}:{:0>2}",
                    pos_min, pos_secs, dur_min, dur_secs,
                )
            } else {
                let pos_hours = pos_secs / 60 / 60;
                let dur_hours = dur_secs / 60 / 60;
                format!(
                    "{:0>1}:{:0>2}:{:0>2}/{:0>1}:{:0>2}:{:0>2}",
                    pos_hours, pos_min, pos_secs, dur_hours, dur_min, dur_secs,
                )
            };

            let time_text = Span::raw(completion_text);
            let time_p = Paragraph::new(Spans::from(time_text));
            f.render_widget(time_p, elapsed_layout[1]);
        }
    }
}
