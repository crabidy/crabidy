use async_trait::async_trait;
use proto::crabidy::{LibraryNode, LibraryNodeChild, Track};

pub mod proto;

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
    Config(String),
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
    pub fn new(uuid: String, title: String, is_queable: bool) -> Self {
        Self {
            uuid,
            title,
            is_queable,
        }
    }
}

pub enum QueueError {
    NotQueable,
}
