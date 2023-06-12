use std::{
    fs::{create_dir_all, read_to_string, File},
    io::Write,
    path::Path,
};

use async_trait::async_trait;
pub use clap_serde_derive::{self, clap, serde, ClapSerde};
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

pub fn init_config<T>(config_file_name: &str) -> T
where
    T: Default + ClapSerde + serde::Serialize + std::fmt::Debug,
{
    if let Some(config_dir) = dirs::config_dir() {
        let dir = Path::new(&config_dir).join("crabidy");
        if !dir.is_dir() {
            create_dir_all(&dir).expect("Could not create crabidy config directory");
        }
        let config_file_path = dir.join(config_file_name);
        if !config_file_path.is_file() {
            let config = T::default().merge_clap();
            let content = toml::to_string_pretty(&config).expect("Could not serialize config");
            let mut config_file =
                File::create(config_file_path).expect("Could not open config file for writing");
            config_file
                .write_all(content.as_bytes())
                .expect("Failed to write to file");
            return config;
        } else {
            let content = read_to_string(config_file_path).expect("Could not read config file");
            let parsed = toml::from_str::<<T as ClapSerde>::Opt>(&content).unwrap();
            let config: T = T::from(parsed).merge_clap();
            return config;
        }
    }
    T::default().merge_clap()
}
