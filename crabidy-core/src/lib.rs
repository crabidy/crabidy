pub mod proto;

use async_trait::async_trait;



#[async_trait]
pub trait ProviderClient: std::fmt::Debug + Send + Sync {
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError>;
    fn get_root_list(&self) -> ItemList;
    async fn get_item_list(&self, list_uuid: &str, depth: usize)
        -> Result<ItemList, ProviderError>;
}

#[derive(Clone, Debug, Hash)]
pub enum ProviderError {
    UnknownUser,
    FetchError,
}

#[derive(Clone, Debug)]
pub struct ItemList {
    pub name: String,
    pub uuid: String,
    pub parent: String,
    pub tracks: Option<Vec<Track>>,
    pub children: Vec<ItemList>,
    pub is_queable: bool,
    pub ephemeral: bool,
    pub is_partial: bool,
}

impl ItemList {
    pub fn new() -> Self {
        Self {
            name: "/".to_string(),
            uuid: "/".to_string(),
            parent: "".to_string(),
            tracks: None,
            children: Vec::new(),
            is_queable: false,
            ephemeral: false,
            is_partial: true,
        }
    }
    pub fn replace_sublist(&mut self, sublist: &Self) {
        if self.uuid == sublist.uuid {
            self.name = sublist.name.clone();
            self.tracks = sublist.tracks.clone();
            self.children = sublist.children.clone();
            self.is_queable = sublist.is_queable;
            self.ephemeral = sublist.ephemeral;
            self.is_partial = sublist.is_partial;
            return;
        }
        for child in self.children.iter_mut() {
            child.replace_sublist(sublist);
        }
    }
    pub fn flatten(&self) -> Vec<Track> {
        let mut tracks = Vec::new();
        if let Some(own_tracks) = &self.tracks {
            tracks.extend(own_tracks.clone());
        }
        for child in self.children.iter() {
            tracks.extend(child.flatten());
        }
        tracks
    }
}

#[derive(Clone, Debug)]
struct ItemListFilter {
    uuid_filter: Option<String>,
    name_filter: Option<String>,
    provider_filter: Option<String>,
}

#[derive(Clone, Debug)]
enum PlayState {
    Buffering,
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub title: String,
    pub uuid: String,
    pub duration: Option<i32>,
    pub album: Option<Album>,
    pub artist: Option<Artist>,
    pub provider: String,
}

#[derive(Clone, Debug)]
pub struct InputTrack {
    pub uuid: String,
    pub provider: String,
}

#[derive(Clone, Debug)]
pub struct Album {
    pub title: String,
    pub release_date: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Artist {
    pub name: String,
}

#[derive(Clone, Debug, Hash)]
pub enum QueueError {
    ItemListNotQueuable,
}

#[derive(Clone, Debug)]
pub struct Queue {
    pub tracks: Vec<Track>,
    current: i32,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            tracks: vec![],
            current: 0,
        }
    }

    pub fn clear(&mut self) {
        self.current = 0;
        self.tracks = vec![];
    }

    pub fn replace_with_track(&mut self, track: Track) {
        self.current = 0;
        self.tracks = vec![track];
    }

    pub fn replace_with_item_list(&mut self, item_list: &ItemList) -> Result<(), QueueError> {
        if !item_list.is_queable {
            return Err(QueueError::ItemListNotQueuable);
        };
        self.current = 0;
        self.tracks = item_list.flatten();
        Ok(())
    }

    pub fn set_current(&mut self, current: usize) -> bool {
        if current < self.tracks.len() {
            self.current = current as i32;
            true
        } else {
            false
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.current as usize)
    }

    pub fn next(&mut self) -> Option<&Track> {
        if (self.current as usize) < self.tracks.len() {
            self.current += 1;
            Some(&self.tracks[(self.current - 1) as usize])
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<&Track> {
        if self.current > 0 {
            self.current -= 1;
            Some(&self.tracks[self.current as usize])
        } else {
            None
        }
    }

    pub fn append_track(&mut self, track: Track) {
        self.tracks.push(track);
    }

    pub fn append_playlist(&mut self, playlist: &[Track]) {
        self.tracks.extend(playlist.to_vec());
    }

    pub fn queue_track(&mut self, track: Track) {
        self.tracks.insert(self.current as usize, track);
    }

    pub fn queue_playlist(&mut self, playlist: &[Track]) {
        let tail: Vec<Track> = self
            .tracks
            .splice((self.current as usize).., playlist.to_vec())
            .collect();
        self.tracks.extend(tail);
    }
}

#[derive(Clone, Debug)]
pub struct ActiveTrack {
    track: Option<Track>,
    completion: i32,
    play_state: PlayState,
}

impl ActiveTrack {
    pub fn new() -> Self {
        Self {
            track: None,
            completion: 0,
            play_state: PlayState::Stopped,
        }
    }
}
