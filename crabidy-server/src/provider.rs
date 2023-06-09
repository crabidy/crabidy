use crate::ProviderMessage;
use async_trait::async_trait;
use crabidy_core::{
    proto::crabidy::{LibraryNode, LibraryNodeChild, Track},
    ProviderClient, ProviderError,
};
use std::{fs, path::PathBuf, sync::Arc};
use tracing::{debug, error, instrument, warn, Instrument};

#[derive(Debug)]
pub struct ProviderOrchestrator {
    pub provider_tx: flume::Sender<ProviderMessage>,
    provider_rx: flume::Receiver<ProviderMessage>,
    // known_tracks: RwLock<HashMap<String, Track>>,
    // known_nodes: RwLock<HashMap<String, LibraryNode>>,
    tidal_client: Arc<tidaldy::Client>,
}

impl ProviderOrchestrator {
    pub fn run(self) {
        tokio::spawn(async move {
            while let Ok(msg) = self.provider_rx.recv_async().await {
                match msg {
                    ProviderMessage::GetLibraryNode {
                        uuid,
                        result_tx,
                        span,
                    } => {
                        let _e = span.enter();
                        let result = self.get_lib_node(&uuid).in_current_span().await;
                        result_tx
                            .send_async(result)
                            .in_current_span()
                            .await
                            .unwrap();
                    }
                    ProviderMessage::GetTrack {
                        uuid,
                        result_tx,
                        span,
                    } => {
                        let _e = span.enter();
                        let result = self.get_metadata_for_track(&uuid).in_current_span().await;
                        result_tx
                            .send_async(result)
                            .in_current_span()
                            .await
                            .unwrap();
                    }
                    ProviderMessage::GetTrackUrls {
                        uuid,
                        result_tx,
                        span,
                    } => {
                        let _e = span.enter();
                        let result = self.get_urls_for_track(&uuid).in_current_span().await;
                        result_tx
                            .send_async(result)
                            .in_current_span()
                            .await
                            .unwrap();
                    }
                    ProviderMessage::FlattenNode {
                        uuid,
                        result_tx,
                        span,
                    } => {
                        let _e = span.enter();
                        let result = self.flatten_node(&uuid).in_current_span().await;
                        result_tx
                            .send_async(result)
                            .in_current_span()
                            .await
                            .unwrap();
                    }
                }
            }
        });
    }
    #[instrument(skip(self))]
    async fn flatten_node(&self, node_uuid: &str) -> Vec<Track> {
        let mut tracks = Vec::with_capacity(1000);
        let mut nodes_to_go = Vec::with_capacity(100);
        nodes_to_go.push(node_uuid.to_string());
        while let Some(node_uuid) = nodes_to_go.pop() {
            let Ok(node) = self.get_lib_node(&node_uuid).in_current_span().await else {
                    continue
                };
            if node.is_queable {
                tracks.extend(node.tracks);
                nodes_to_go.extend(node.children.into_iter().map(|c| c.uuid))
            }
        }
        tracks
    }
}

#[async_trait]
impl ProviderClient for ProviderOrchestrator {
    #[instrument(skip(_s))]
    async fn init(_s: &str) -> Result<Self, ProviderError> {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("crabidy"))
            .unwrap_or(PathBuf::from("/tmp"));
        let dir_exists = tokio::fs::try_exists(&config_dir)
            .in_current_span()
            .await
            .map_err(|e| ProviderError::Config(e.to_string()))?;
        if !dir_exists {
            tokio::fs::create_dir(&config_dir)
                .in_current_span()
                .await
                .map_err(|e| ProviderError::Config(e.to_string()))?;
        }
        let config_file = config_dir.join("tidaly.toml");
        let raw_toml_settings = fs::read_to_string(&config_file).unwrap_or("".to_owned());
        let tidal_client = Arc::new(
            tidaldy::Client::init(&raw_toml_settings)
                .in_current_span()
                .await
                .unwrap(),
        );
        let new_toml_config = tidal_client.settings();
        if let Err(err) = tokio::fs::write(&config_file, new_toml_config)
            .in_current_span()
            .await
        {
            error!("Failed to write config file: {}", err);
        };
        let (provider_tx, provider_rx) = flume::bounded(100);
        Ok(Self {
            provider_rx,
            provider_tx,
            tidal_client,
        })
    }
    #[instrument(skip(self))]
    fn settings(&self) -> String {
        "".to_owned()
    }
    #[instrument(skip(self))]
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError> {
        self.tidal_client
            .get_urls_for_track(track_uuid)
            .in_current_span()
            .await
    }
    #[instrument(skip(self))]
    async fn get_metadata_for_track(&self, track_uuid: &str) -> Result<Track, ProviderError> {
        debug!("get_metadata_for_track");
        self.tidal_client
            .get_metadata_for_track(track_uuid)
            .in_current_span()
            .await
    }
    #[instrument(skip(self))]
    fn get_lib_root(&self) -> LibraryNode {
        let mut root_node = LibraryNode::new();
        let child = LibraryNodeChild::new("node:tidal".to_owned(), "tidal".to_owned(), false);
        root_node.children.push(child);
        root_node
    }
    #[instrument(skip(self))]
    async fn get_lib_node(&self, uuid: &str) -> Result<LibraryNode, ProviderError> {
        debug!("get_lib_node");
        if uuid == "node:/" {
            return Ok(self.get_lib_root());
        }
        if uuid == "node:tidal" {
            return Ok(self.tidal_client.get_lib_root());
        }
        self.tidal_client.get_lib_node(uuid).in_current_span().await
    }
}
