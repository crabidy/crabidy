use crabidy_core::proto::crabidy::{
    crabidy_service_client::CrabidyServiceClient, AppendRequest, ChangeVolumeRequest,
    ClearQueueRequest, GetLibraryNodeRequest, GetUpdateStreamRequest, GetUpdateStreamResponse,
    InitRequest, InitResponse, InsertRequest, LibraryNode, NextRequest, PrevRequest, QueueRequest,
    RemoveRequest, ReplaceRequest, RestartTrackRequest, SetCurrentRequest, ToggleMuteRequest,
    TogglePlayRequest, ToggleRepeatRequest, ToggleShuffleRequest,
};

use std::{collections::HashMap, error::Error, fmt, time::Duration};

use tonic::{
    transport::{Channel, Endpoint},
    Request, Streaming,
};

// FIXME: use anyhow + thiserror
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
    pub update_stream: Streaming<GetUpdateStreamResponse>,
}

impl RpcClient {
    pub async fn connect(addr: &'static str) -> Result<RpcClient, Box<dyn Error>> {
        let endpoint = Endpoint::from_static(addr).connect_lazy();
        let mut client = CrabidyServiceClient::new(endpoint);

        let update_stream = Self::get_update_stream(&mut client).await;
        let library_node_cache: HashMap<String, LibraryNode> = HashMap::new();

        Ok(RpcClient {
            client,
            library_node_cache,
            update_stream,
        })
    }

    async fn get_update_stream(
        client: &mut CrabidyServiceClient<Channel>,
    ) -> Streaming<GetUpdateStreamResponse> {
        loop {
            let get_update_stream_request = Request::new(GetUpdateStreamRequest {});
            if let Ok(resp) = client.get_update_stream(get_update_stream_request).await {
                return resp.into_inner();
            } else {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    pub async fn reconnect_update_stream(&mut self) {
        self.update_stream = Self::get_update_stream(&mut self.client).await;
    }

    pub async fn init(&mut self) -> Result<InitResponse, Box<dyn Error>> {
        let init_request = Request::new(InitRequest {});
        let response = self.client.init(init_request).await?;
        Ok(response.into_inner())
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

    pub async fn append_tracks(&mut self, uuids: Vec<String>) -> Result<(), Box<dyn Error>> {
        let append_request = Request::new(AppendRequest { uuids });
        self.client.append(append_request).await?;
        Ok(())
    }

    pub async fn queue_tracks(&mut self, uuids: Vec<String>) -> Result<(), Box<dyn Error>> {
        let queue_request = Request::new(QueueRequest { uuids });
        self.client.queue(queue_request).await?;
        Ok(())
    }

    pub async fn insert_tracks(
        &mut self,
        uuids: Vec<String>,
        pos: usize,
    ) -> Result<(), Box<dyn Error>> {
        let insert_request = Request::new(InsertRequest {
            uuids,
            position: pos as u32,
        });
        self.client.insert(insert_request).await?;
        Ok(())
    }

    pub async fn remove_tracks(&mut self, positions: Vec<usize>) -> Result<(), Box<dyn Error>> {
        let remove_request = Request::new(RemoveRequest {
            positions: positions.iter().map(|p| *p as u32).collect(),
        });
        self.client.remove(remove_request).await?;
        Ok(())
    }

    pub async fn clear_queue(&mut self, exclude_current: bool) -> Result<(), Box<dyn Error>> {
        let clear_queue_request = Request::new(ClearQueueRequest { exclude_current });
        self.client.clear_queue(clear_queue_request).await?;
        Ok(())
    }

    pub async fn replace_queue(&mut self, uuids: Vec<String>) -> Result<(), Box<dyn Error>> {
        let replace_request = Request::new(ReplaceRequest { uuids });
        self.client.replace(replace_request).await?;
        Ok(())
    }

    pub async fn next_track(&mut self) -> Result<(), Box<dyn Error>> {
        let next_request = Request::new(NextRequest {});
        self.client.next(next_request).await?;
        Ok(())
    }

    pub async fn prev_track(&mut self) -> Result<(), Box<dyn Error>> {
        let prev_request = Request::new(PrevRequest {});
        self.client.prev(prev_request).await?;
        Ok(())
    }

    pub async fn restart_track(&mut self) -> Result<(), Box<dyn Error>> {
        let restart_track_request = Request::new(RestartTrackRequest {});
        self.client.restart_track(restart_track_request).await?;
        Ok(())
    }

    pub async fn set_current_track(&mut self, pos: usize) -> Result<(), Box<dyn Error>> {
        let set_current_request = Request::new(SetCurrentRequest {
            position: pos as u32,
        });
        self.client.set_current(set_current_request).await?;
        Ok(())
    }

    pub async fn toggle_play(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_play_request = Request::new(TogglePlayRequest {});
        self.client.toggle_play(toggle_play_request).await?;
        Ok(())
    }

    pub async fn toggle_shuffle(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_shuffle_request = Request::new(ToggleShuffleRequest {});
        self.client.toggle_shuffle(toggle_shuffle_request).await?;
        Ok(())
    }

    pub async fn toggle_repeat(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_repeat_request = Request::new(ToggleRepeatRequest {});
        self.client.toggle_repeat(toggle_repeat_request).await?;
        Ok(())
    }

    pub async fn change_volume(&mut self, delta: f32) -> Result<(), Box<dyn Error>> {
        let change_volume_request = Request::new(ChangeVolumeRequest { delta });
        self.client.change_volume(change_volume_request).await?;
        Ok(())
    }

    pub async fn toggle_mute(&mut self) -> Result<(), Box<dyn Error>> {
        let toggle_mute_request = Request::new(ToggleMuteRequest {});
        self.client.toggle_mute(toggle_mute_request).await?;
        Ok(())
    }
}
