use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, get_queue_updates_response::QueueUpdateResult,
    GetLibraryNodeRequest, GetQueueUpdatesRequest, GetQueueUpdatesResponse, GetTrackUpdatesRequest,
    GetTrackUpdatesResponse, LibraryNode, LibraryNodeState, ReplaceWithNodeRequest,
    ReplaceWithNodeResponse, ReplaceWithTrackRequest, ReplaceWithTrackResponse,
    SetCurrentTrackRequest, SetCurrentTrackResponse, TogglePlayRequest,
};

use std::{
    collections::HashMap,
    error::Error,
    fmt, io, mem, println, thread,
    time::{Duration, Instant},
    vec,
};
use tokio::task;
use tokio_stream::StreamExt;
use tonic::{
    transport::{Channel, Endpoint},
    Request, Streaming,
};

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
    pub queue_updates_stream: Streaming<GetQueueUpdatesResponse>,
    pub track_updates_stream: Streaming<GetTrackUpdatesResponse>,
}

impl RpcClient {
    pub async fn connect(addr: &'static str) -> Result<RpcClient, Box<dyn Error>> {
        let endpoint = Endpoint::from_static(addr).connect_lazy();
        let mut client = CrabidyServiceClient::new(endpoint);

        let queue_updates_stream = Self::get_queue_updates_stream(&mut client).await;
        let track_updates_stream = Self::get_track_updates_stream(&mut client).await;
        let library_node_cache: HashMap<String, LibraryNode> = HashMap::new();

        Ok(RpcClient {
            client,
            library_node_cache,
            track_updates_stream,
            queue_updates_stream,
        })
    }

    async fn get_queue_updates_stream(
        client: &mut CrabidyServiceClient<Channel>,
    ) -> Streaming<GetQueueUpdatesResponse> {
        loop {
            let get_queue_updates_request = Request::new(GetQueueUpdatesRequest { timestamp: 0 });
            if let Ok(resp) = client.get_queue_updates(get_queue_updates_request).await {
                return resp.into_inner();
            } else {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    async fn get_track_updates_stream(
        client: &mut CrabidyServiceClient<Channel>,
    ) -> Streaming<GetTrackUpdatesResponse> {
        loop {
            let get_track_updates_request = Request::new(GetTrackUpdatesRequest {
                type_whitelist: Vec::new(),
                type_blacklist: Vec::new(),
                updates_skipped: 0,
            });
            if let Ok(resp) = client.get_track_updates(get_track_updates_request).await {
                return resp.into_inner();
            } else {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    pub async fn reconnect_queue_updates_stream(&mut self) {
        let queue_updates_stream = Self::get_queue_updates_stream(&mut self.client).await;
        mem::replace(&mut self.queue_updates_stream, queue_updates_stream);
    }

    pub async fn reconnect_track_updates_stream(&mut self) {
        let track_updates_stream = Self::get_track_updates_stream(&mut self.client).await;
        mem::replace(&mut self.track_updates_stream, track_updates_stream);
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
            return Ok(self.library_node_cache.get(uuid));
        }

        Err(Box::new(RpcClientError::NotFound))
    }

    pub async fn replace_queue_with_node(&mut self, uuid: &str) -> Result<(), Box<dyn Error>> {
        let replace_with_node_request = Request::new(ReplaceWithNodeRequest {
            uuid: uuid.to_string(),
        });

        let response = self
            .client
            .replace_with_node(replace_with_node_request)
            .await?;

        Ok(())
    }

    pub async fn replace_queue_with_track(&mut self, uuid: &str) -> Result<(), Box<dyn Error>> {
        let replace_with_track_request = Request::new(ReplaceWithTrackRequest {
            uuid: uuid.to_string(),
        });

        let response = self
            .client
            .replace_with_track(replace_with_track_request)
            .await?;

        Ok(())
    }

    pub async fn toggle_play(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_play_request = Request::new(TogglePlayRequest {});

        let response = self.client.toggle_play(toggle_play_request).await?;

        Ok(())
    }

    pub async fn set_current_track(&mut self, pos: usize) -> Result<(), Box<dyn Error>> {
        let set_current_track_request = Request::new(SetCurrentTrackRequest {
            position: pos as u32,
        });

        let response = self
            .client
            .set_current_track(set_current_track_request)
            .await?;

        Ok(())
    }
}
