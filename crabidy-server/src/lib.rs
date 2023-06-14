use crabidy_core::proto::crabidy::{Queue, Track};
use rand::{seq::SliceRandom, thread_rng};
use std::time::SystemTime;
use tracing::{debug, error};

#[derive(Clone, Debug)]
pub struct QueueManager {
    created_at: SystemTime,
    current_offset: usize,
    play_order: Vec<usize>,
    tracks: Vec<Track>,
    pub repeat: bool,
    pub shuffle: bool,
}

impl From<QueueManager> for Queue {
    fn from(queue_manager: QueueManager) -> Self {
        Self {
            timestamp: queue_manager
                .created_at
                .elapsed()
                .expect("failed to get elapsed time")
                .as_secs(),
            current_position: queue_manager.current_position() as u32,
            tracks: queue_manager.tracks,
        }
    }
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            created_at: SystemTime::now(),
            current_offset: 0,
            play_order: Vec::new(),
            tracks: Vec::new(),
            repeat: false,
            shuffle: false,
        }
    }
    pub fn current_position(&self) -> usize {
        if self.current_offset < self.play_order.len() {
            self.play_order[self.current_offset]
        } else {
            0
        }
    }

    pub fn shuffle_on(&mut self) {
        self.shuffle = true;
        self.shuffle_before(self.current_offset);
        self.shuffle_behind(self.current_offset);
    }

    pub fn shuffle_off(&mut self) {
        self.shuffle = false;
        self.play_order = (0..self.tracks.len()).collect();
    }

    pub fn shuffle_all(&mut self) {
        self.play_order.shuffle(&mut thread_rng());
    }

    pub fn shuffle_before(&mut self, pos: usize) {
        self.play_order[..pos].shuffle(&mut thread_rng());
    }

    pub fn shuffle_behind(&mut self, pos: usize) {
        self.play_order[pos + 1..].shuffle(&mut thread_rng());
    }

    pub fn current_track(&self) -> Option<Track> {
        if self.current_position() < self.tracks.len() {
            Some(self.tracks[self.current_position()].clone())
        } else {
            None
        }
    }

    pub fn next_track(&mut self) -> Option<Track> {
        let len = self.tracks.len();
        if self.current_offset < len - 1 {
            self.current_offset += 1;
            let current_pos = self.current_position();
            if current_pos < len {
                Some(self.tracks[current_pos].clone())
            } else {
                None
            }
        } else {
            debug!("no more tracks");
            if self.repeat {
                debug!("repeat");
                self.current_offset = 0;
                if self.shuffle {
                    self.shuffle_all();
                }
                return self.current_track();
            }
            None
        }
    }

    pub fn prev_track(&mut self) -> Option<Track> {
        if 0 < self.current_offset {
            self.current_offset -= 1;
            Some(self.tracks[self.current_position()].clone())
        } else {
            None
        }
    }

    pub fn set_current_position(&mut self, current_position: u32) -> bool {
        if current_position < self.tracks.len() as u32 {
            if self.shuffle {
                self.shuffle_all();
            }
            let Some(current_offset) = self
                .play_order
                .iter()
                .position(|&i| i == current_position as usize)
                else {
                    error!("invalid current position");
                    error!("queue: {:#?}", self);
                    return false
                };
            if self.shuffle {
                self.play_order.swap(0, current_offset);
                self.current_offset = 0;
            } else {
                self.current_offset = current_offset;
            }
            true
        } else {
            false
        }
    }

    pub fn replace_with_tracks(&mut self, tracks: &[Track]) -> Option<Track> {
        self.current_offset = 0;
        self.tracks = tracks.to_vec();
        self.play_order = (0..self.tracks.len()).collect();
        if self.shuffle {
            self.shuffle_all();
        }
        if 0 < self.tracks.len() as u32 {
            Some(self.tracks[self.current_position()].clone())
        } else {
            None
        }
    }

    pub fn append_tracks(&mut self, tracks: &[Track]) {
        let len = self.tracks.len();
        let order_additions: Vec<usize> = (len..len + tracks.len()).collect();
        self.play_order.extend(order_additions);
        self.tracks.extend(tracks.iter().cloned());
        if self.shuffle {
            self.shuffle_behind(self.current_offset);
        }
    }

    pub fn remove_tracks(&mut self, positions: &[u32]) -> Option<Track> {
        let mut play_next = false;
        for pos in positions {
            if (self.tracks.len() as u32) < *pos {
                return None;
            };
            if *pos == self.current_position() as u32 {
                play_next = true;
            }
            let Some(offset) = self
                .play_order
                .iter()
                .position(|&i| i == *pos as usize)
                else {
                    error!("invalid current position");
                    error!("queue: {:#?}", self);
                    return None
                };
            if offset < self.current_offset {
                self.current_offset -= 1;
            }
            self.tracks.remove(*pos as usize);
            self.play_order.remove(offset);
            self.play_order
                .iter_mut()
                .filter(|i| (*pos as usize) < **i)
                .for_each(|i| *i -= 1);
        }
        if play_next {
            self.current_track()
        } else {
            None
        }
    }

    pub fn insert_tracks(&mut self, position: u32, tracks: &[Track]) {
        let len = self.tracks.len();
        if len == 0 {
            self.replace_with_tracks(tracks);
            return;
        }
        let order_additions: Vec<usize> = (len..len + tracks.len()).collect();
        self.play_order.extend(order_additions);
        let tail: Vec<Track> = self
            .tracks
            .splice((position as usize + 1).., tracks.to_vec())
            .collect();
        self.tracks.extend(tail);
        let mut changed: Vec<usize> = Vec::new();
        // in shuffle mode, it might be that we played already postions which are behind
        // the insertion point and which postions are shifted by the lenght of the inserted
        // track
        for i in self
            .play_order
            .iter_mut()
            .take(self.current_offset)
            .filter(|i| (position as usize) < **i)
        {
            *i += len;
            changed.push(*i);
        }
        if !self.shuffle {
            // if we don't shuffle, there should be no positions alredy played behind the
            // current track
            assert!(changed.is_empty());
        }
        // the newly inserted indices need to replaced with the ones that we already handled
        self.play_order
            .iter_mut()
            .skip(self.current_offset)
            .for_each(|i| {
                if changed.contains(i) {
                    *i -= len;
                }
            });

        if self.shuffle {
            self.shuffle_behind(self.current_offset);
        }
    }

    pub fn queue_tracks(&mut self, tracks: &[Track]) {
        let pos = self.current_position();
        self.insert_tracks(pos as u32, tracks);
    }

    pub fn clear(&mut self, exclude_current: bool) -> bool {
        let current_track = self.current_track();
        self.current_offset = 0;
        self.tracks.clear();

        if exclude_current {
            if let Some(track) = current_track {
                self.tracks.push(track);
            }
        }

        !exclude_current
    }
}

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    fn random_delete_before() {}
    #[test]
    fn random_delete_track() {}
    #[test]
    fn random_delete_after() {}
    #[test]
    fn random_select_track() {}
}
