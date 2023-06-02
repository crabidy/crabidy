pub mod proto;

use async_trait::async_trait;
use proto::crabidy::{LibraryNode, LibraryNodeChild, Queue, Track};

#[async_trait]
pub trait ProviderClient: std::fmt::Debug + Send + Sync {
    async fn init(raw_toml_settings: &str) -> Result<Self, ProviderError>
    where
        Self: Sized;
    fn settings(&self) -> String;
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError>;
    async fn get_metadata_for_track(&self, track_uuid: &str) -> Result<Track, ProviderError>;
    fn get_lib_root(&self) -> LibraryNode;
    async fn get_lib_node(&self, list_uuid: &str) -> Result<LibraryNode, ProviderError>;
}

#[derive(Clone, Debug, Hash)]
pub enum ProviderError {
    UnknownUser,
    CouldNotLogin,
    FetchError,
    MalformedUuid,
    InternalError,
    Other,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl LibraryNode {
    pub fn new() -> Self {
        Self {
            uuid: "node:/".to_string(),
            title: "/".to_string(),
            children: Vec::new(),
            parent: None,
            tracks: Vec::new(),
            is_queable: false,
        }
    }
}

impl LibraryNodeChild {
    pub fn new(uuid: String, title: String) -> Self {
        Self { uuid, title }
    }
}

pub enum QueueError {
    NotQueable,
}

impl Queue {
    pub fn current(&self) -> Option<Track> {
        if self.current_position < self.tracks.len() as u32 {
            Some(self.tracks[self.current_position as usize].clone())
        } else {
            None
        }
    }

    pub fn next_track(&mut self) -> Option<Track> {
        if self.current_position < self.tracks.len() as u32 {
            self.current_position += 1;
            Some(self.tracks[self.current_position as usize].clone())
        } else {
            None
        }
    }

    pub fn set_current_position(&mut self, current_position: u32) -> bool {
        if current_position < self.tracks.len() as u32 {
            self.current_position = current_position;
            true
        } else {
            false
        }
    }

    pub fn replace_with_tracks(&mut self, tracks: &[Track]) {
        self.current_position = 0;
        self.tracks = tracks.to_vec();
    }

    pub fn append_tracks(&mut self, tracks: &[Track]) {
        self.tracks.extend(tracks.iter().cloned());
    }

    pub fn queue_tracks(&mut self, tracks: &[Track]) {
        let tail: Vec<Track> = self
            .tracks
            .splice((self.current_position as usize).., tracks.to_vec())
            .collect();
        self.tracks.extend(tail);
    }

    pub fn remove_tracks(&mut self, positions: &[u32]) {
        for pos in positions {
            if *pos < self.tracks.len() as u32 {
                self.tracks.remove(*pos as usize);
            }
        }
    }

    pub fn insert_tracks(&mut self, position: u32, tracks: &[Track]) {
        let tail: Vec<Track> = self
            .tracks
            .splice((position as usize).., tracks.to_vec())
            .collect();
        self.tracks.extend(tail);
    }
}
