pub mod proto;

use async_trait::async_trait;
use futures::Stream;
use juniper::{
    graphql_object, graphql_subscription, FieldError, FieldResult, GraphQLEnum, GraphQLInputObject,
    GraphQLObject, RootNode,
};
use std::{collections::HashMap, pin::Pin, sync::Arc};

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError>;
    async fn get_item_lists_full(&self) -> Result<Vec<ItemList>, ProviderError>;
    async fn get_item_lists_partial(
        &self,
        list_uuid: String,
    ) -> Result<Vec<ItemList>, ProviderError>;
    async fn get_item_list_partial_without_children(
        &self,
        list_uuid: String,
    ) -> Result<ItemList, ProviderError>;
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
    pub provider: String,
    pub tracks: Option<Vec<Track>>,
    pub children: Option<Vec<ItemList>>,
    pub is_queable: bool,
    without_children: bool,
}

#[graphql_object(context = Context)]
impl ItemList {
    fn name(&self) -> &str {
        &self.name
    }
    fn uuid(&self) -> &str {
        &self.uuid
    }
    fn provider(&self) -> &str {
        &self.provider
    }
    //TODO: Be smarter than clone here
    fn tracks(&self, refresh: bool) -> Option<Vec<Track>> {
        self.tracks.clone()
    }
    //TODO: Be smarter than clone here
    fn children(&self, refresh: bool) -> Option<Vec<ItemList>> {
        self.children.clone()
    }
    fn is_queable(&self) -> bool {
        self.is_queable
    }
}

impl ItemList {
    pub fn new() -> Self {
        Self {
            name: "root".to_string(),
            uuid: "root".to_string(),
            provider: "root".to_string(),
            tracks: None,
            children: None,
            is_queable: false,
            without_children: true,
        }
    }
    pub fn flatten(&self) -> Vec<Track> {
        let mut tracks = Vec::new();
        if let Some(own_tracks) = &self.tracks {
            tracks.extend(own_tracks.clone());
        }
        if let Some(childs) = &self.children {
            for child in childs {
                tracks.extend(child.flatten());
            }
        }
        tracks
    }
}

#[derive(Clone, Debug, GraphQLEnum)]
enum PlayState {
    Buffering,
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Debug, GraphQLObject)]
pub struct Track {
    pub title: String,
    pub uuid: String,
    pub duration: Option<i32>,
    pub album: Option<Album>,
    pub artist: Option<Artist>,
    pub provider: String,
}

#[derive(Clone, Debug, GraphQLInputObject)]
pub struct InputTrack {
    pub uuid: String,
    pub provider: String,
}

#[derive(Clone, Debug, GraphQLObject)]
pub struct Album {
    pub title: String,
    pub release_date: Option<String>,
}

#[derive(Clone, Debug, GraphQLObject)]
pub struct Artist {
    pub name: String,
}

#[derive(Clone, Debug, Hash)]
pub enum QueueError {
    ItemListNotQueuable,
}

#[derive(Clone, Debug, GraphQLObject)]
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
pub struct Mutation;

impl Mutation {
    pub fn new() -> Self {
        Self
    }
}

#[graphql_object(context = Context)]
impl Mutation {
    fn playpause(&self, track: InputTrack) -> FieldResult<ActiveTrack> {
        Ok(ActiveTrack::new())
    }
    fn stop(&self, track: InputTrack) -> FieldResult<ActiveTrack> {
        Ok(ActiveTrack::new())
    }
    fn previous(&self, track: InputTrack) -> FieldResult<ActiveTrack> {
        Ok(ActiveTrack::new())
    }
    fn next(&self, track: InputTrack) -> FieldResult<ActiveTrack> {
        Ok(ActiveTrack::new())
    }
    fn seek(&self, track: InputTrack, millis: i32) -> FieldResult<ActiveTrack> {
        Ok(ActiveTrack::new())
    }
    fn append(&self, tracks: Vec<InputTrack>) -> FieldResult<Success> {
        Ok(Success::Appending)
    }
    fn queue(&self, tracks: Vec<InputTrack>) -> FieldResult<Success> {
        Ok(Success::Queuing)
    }
    fn replace(&self, tracks: Vec<InputTrack>) -> FieldResult<Success> {
        Ok(Success::Replacing)
    }
    fn delete(&self, track: InputTrack) -> FieldResult<Success> {
        Ok(Success::Deleting)
    }
    fn clear(&self, track: InputTrack) -> FieldResult<Success> {
        Ok(Success::Clearing)
    }
}

#[derive(Clone, Debug, GraphQLEnum)]
enum Success {
    Appending,
    Replacing,
    Queuing,
    Deleting,
    Clearing,
}

#[derive(Clone, Debug, GraphQLObject)]
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

type ActiveTrackStream = Pin<Box<dyn Stream<Item = Result<ActiveTrack, FieldError>> + Send>>;
type QueueStream = Pin<Box<dyn Stream<Item = Result<Queue, FieldError>> + Send>>;

#[derive(Clone, Debug)]
pub struct Subscription {
    queue: Queue,
    active_track: Option<ActiveTrack>,
}

#[graphql_subscription(context = Context)]
impl Subscription {
    async fn queue() -> QueueStream {
        let stream = futures::stream::iter(vec![Ok(Queue::new())]);
        Box::pin(stream)
    }
    async fn active_track() -> ActiveTrackStream {
        let stream = futures::stream::iter(vec![Ok(ActiveTrack::new())]);
        Box::pin(stream)
    }
}

impl Subscription {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            active_track: None,
        }
    }
}

pub struct Context {
    clients: HashMap<String, Arc<dyn ProviderClient>>,
    queue: Queue,
}

impl Context {
    pub fn new(clients: HashMap<String, Arc<dyn ProviderClient>>) -> Self {
        let queue = Queue::new();
        Self { clients, queue }
    }
}

impl juniper::Context for Context {}

pub type Schema = RootNode<'static, ItemList, Mutation, Subscription>;
