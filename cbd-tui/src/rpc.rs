use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, get_queue_updates_response::QueueUpdateResult,
    GetLibraryNodeRequest, GetQueueUpdatesRequest, GetQueueUpdatesResponse, GetTrackUpdatesRequest,
    GetTrackUpdatesResponse, LibraryNode, LibraryNodeState, ReplaceWithNodeRequest,
    ReplaceWithNodeResponse, TogglePlayRequest,
};

use std::{
    collections::HashMap,
    error::Error,
    fmt, io, println, thread,
    time::{Duration, Instant},
    vec,
};
use tokio::task;
use tokio_stream::StreamExt;
use tonic::{transport::Channel, Request, Streaming};

#[derive(Debug)]
enum RpcClientError {
    NotFound,
}

impl fmt::Display for RpcClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcClientError::NotFound => write!(f, "Requested item not found"),
        }
    }
}

impl Error for RpcClientError {}

pub struct RpcClient {
    library_node_cache: HashMap<String, LibraryNode>,
    client: CrabidyServiceClient<Channel>,
}

impl RpcClient {
    pub async fn connect(addr: &'static str) -> Result<RpcClient, tonic::transport::Error> {
        let client = CrabidyServiceClient::connect(addr).await?;
        let library_node_cache: HashMap<String, LibraryNode> = HashMap::new();
        Ok(RpcClient {
            client,
            library_node_cache,
        })
    }

    pub async fn get_library_node(
        &mut self,
        uuid: &str,
    ) -> Result<Option<&LibraryNode>, Box<dyn Error>> {
        if self.library_node_cache.contains_key(uuid) {
            return Ok(self.library_node_cache.get(uuid));
        }

        let get_library_node_request = Request::new(GetLibraryNodeRequest {
            uuid: uuid.to_string(),
        });

        let response = self
            .client
            .get_library_node(get_library_node_request)
            .await?;

        if let Some(library_node) = response.into_inner().node {
            self.library_node_cache
                .insert(uuid.to_string(), library_node);
            // FIXME: is that necessary?
            return Ok(self.library_node_cache.get(uuid));
        }

        Err(Box::new(RpcClientError::NotFound))
    }

    pub async fn get_queue_updates_stream(
        &mut self,
    ) -> Result<Streaming<GetQueueUpdatesResponse>, Box<dyn Error>> {
        // FIXME: Adjust request params to what we need
        let get_queue_updates_request = Request::new(GetQueueUpdatesRequest { timestamp: 0 });

        let stream = self
            .client
            .get_queue_updates(get_queue_updates_request)
            .await?
            .into_inner();
        Ok(stream)
    }

    pub async fn get_track_updates_stream(
        &mut self,
    ) -> Result<Streaming<GetTrackUpdatesResponse>, Box<dyn Error>> {
        // FIXME: Adjust request params to what we need
        let get_queue_updates_request = Request::new(GetTrackUpdatesRequest {
            type_whitelist: Vec::new(),
            type_blacklist: Vec::new(),
            updates_skipped: 0,
        });

        let stream = self
            .client
            .get_track_updates(get_queue_updates_request)
            .await?
            .into_inner();
        Ok(stream)
    }

    pub async fn replace_queue_with(&mut self, uuid: &str) -> Result<(), Box<dyn Error>> {
        let replace_with_node_request = Request::new(ReplaceWithNodeRequest {
            uuid: uuid.to_string(),
        });

        let response = self
            .client
            .replace_with_node(replace_with_node_request)
            .await?;

        Ok(())
    }

    pub async fn toggle_play(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_play_request = Request::new(TogglePlayRequest {});

        let response = self
            .client
            .toggle_play(toggle_play_request)
            .await?;

        Ok(())
    }
}
