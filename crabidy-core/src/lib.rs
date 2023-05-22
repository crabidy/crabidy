pub mod proto;

use async_trait::async_trait;



#[async_trait]
pub trait ProviderClient: std::fmt::Debug + Send + Sync {
    async fn init(raw_toml_settings: &str) -> Result<Self, ProviderError>
    where
        Self: Sized;
    fn settings(&self) -> String;
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError>;
    fn get_library_root(&self) -> proto::crabidy::LibraryNodeResponse;
    async fn get_library_node(
        &self,
        list_uuid: &str,
    ) -> Result<proto::crabidy::LibraryNodeResponse, ProviderError>;
}

#[derive(Clone, Debug, Hash)]
pub enum ProviderError {
    UnknownUser,
    CouldNotLogin,
    FetchError,
    MalformedUuid,
    Other,
}

impl proto::crabidy::LibraryNodeResponse {
    pub fn new() -> Self {
        Self {
            uuid: "/".to_string(),
            name: "/".to_string(),
            children: Vec::new(),
            parent: None,
            state: proto::crabidy::LibraryNodeState::Unspecified as i32,
            tracks: Vec::new(),
            is_queable: false,
        }
    }
}
