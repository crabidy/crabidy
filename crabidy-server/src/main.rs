use anyhow::{Error, Result};
use async_trait::async_trait;
use crabidy_core::proto::crabidy::{
    crabidy_service_server::{CrabidyService, CrabidyServiceServer},
    AppendNodeRequest, AppendNodeResponse, AppendTrackRequest, AppendTrackResponse,
    GetActiveTrackRequest, GetActiveTrackResponse, GetLibraryNodeRequest, GetLibraryNodeResponse,
    GetQueueRequest, GetQueueResponse, GetQueueUpdatesRequest, GetQueueUpdatesResponse,
    GetTrackRequest, GetTrackResponse, GetTrackUpdatesRequest, GetTrackUpdatesResponse,
    LibraryNode, LibraryNodeState, Queue, QueueLibraryNodeRequest, QueueLibraryNodeResponse,
    QueueTrackRequest, QueueTrackResponse, RemoveTracksRequest, RemoveTracksResponse,
    ReplaceWithNodeRequest, ReplaceWithNodeResponse, ReplaceWithTrackRequest,
    ReplaceWithTrackResponse, SaveQueueRequest, SaveQueueResponse, SetCurrentTrackRequest,
    SetCurrentTrackResponse, StopRequest, StopResponse, TogglePlayRequest, TogglePlayResponse,
    TrackPlayState,
};
use crabidy_core::{ProviderClient, ProviderError};
use gstreamer_play::{Play, PlayMessage, PlayState, PlayVideoRenderer};
// use once_cell::sync::OnceCell;
use std::{collections::HashMap, fs, pin::Pin, sync::RwLock};
use tonic::{codegen::futures_core::Stream, transport::Server, Request, Response, Status};

// static CHANNEL: OnceCell<flume::Sender<Input>> = OnceCell::new();
// static ORCHESTRATOR_CHANNEL: OnceCell<flume::Sender<OrchestratorMessage>> = OnceCell::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let orchestrator = ClientOrchestrator::init("").await.unwrap();
    let tx = orchestrator.run();
    let crabidy_service = AppState::new(tx);

    let addr = "[::1]:50051".parse()?;
    Server::builder()
        .add_service(CrabidyServiceServer::new(crabidy_service))
        .serve(addr)
        .await?;

    Ok(())
}

enum OrchestratorMessage {
    GetNode {
        uuid: String,
        callback: flume::Sender<LibraryNode>,
    },
    GetTracksPlaybackUrls {
        uuid: String,
        callback: flume::Sender<Vec<String>>,
    },
}

#[derive(Debug)]
struct ClientOrchestrator {
    rx: flume::Receiver<OrchestratorMessage>,
    tx: flume::Sender<OrchestratorMessage>,
    tidal_client: tidaldy::Client,
}

impl ClientOrchestrator {
    fn run(self) -> flume::Sender<OrchestratorMessage> {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = self.rx.recv_async().await {
                match msg {
                    OrchestratorMessage::GetNode { uuid, callback } => {
                        let node = match uuid.as_str() {
                            "/" => self.get_library_root(),
                            _ => self.get_library_node(&uuid).await.unwrap(),
                        };
                        callback.send_async(node).await;
                    }
                    OrchestratorMessage::GetTracksPlaybackUrls { uuid, callback } => {
                        let urls = self.get_urls_for_track(&uuid).await.unwrap();
                        callback.send_async(urls).await;
                    }
                }
            }
        });
        tx
    }
}

#[async_trait]
impl ProviderClient for ClientOrchestrator {
    async fn init(_s: &str) -> Result<Self, ProviderError> {
        let raw_toml_settings = fs::read_to_string("/tmp/tidaldy.toml").unwrap_or("".to_owned());
        let tidal_client = tidaldy::Client::init(&raw_toml_settings).await.unwrap();
        let new_toml_config = tidal_client.settings();
        fs::write("/tmp/tidaldy.toml", new_toml_config).unwrap();
        let (tx, rx) = flume::unbounded();
        Ok(Self {
            rx,
            tx,
            tidal_client,
        })
    }
    fn settings(&self) -> String {
        "".to_owned()
    }
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError> {
        self.tidal_client.get_urls_for_track(track_uuid).await
    }
    fn get_library_root(&self) -> LibraryNode {
        let mut root_node = LibraryNode::new();
        root_node.children.push("tidal".to_owned());
        root_node
    }
    async fn get_library_node(&self, uuid: &str) -> Result<LibraryNode, ProviderError> {
        if uuid == "tidal" {
            return Ok(self.tidal_client.get_library_root());
        }
        self.tidal_client.get_library_node(uuid).await
    }
}

#[derive(Debug)]
struct AppState {
    known_nodes: RwLock<HashMap<String, LibraryNode>>,
    orchestrator_tx: flume::Sender<OrchestratorMessage>,
}

impl AppState {
    fn new(orchestrator_tx: flume::Sender<OrchestratorMessage>) -> Self {
        Self {
            known_nodes: RwLock::new(HashMap::new()),
            orchestrator_tx,
        }
    }
}

#[tonic::async_trait]
impl CrabidyService for AppState {
    type GetQueueUpdatesStream =
        futures::stream::Iter<std::vec::IntoIter<Result<GetQueueUpdatesResponse, Status>>>;
    type GetTrackUpdatesStream =
        futures::stream::Iter<std::vec::IntoIter<Result<GetTrackUpdatesResponse, Status>>>;

    async fn get_library_node(
        &self,
        request: Request<GetLibraryNodeRequest>,
    ) -> Result<Response<GetLibraryNodeResponse>, Status> {
        println!("Got a library node request: {:?}", request);
        let node_uuid = request.into_inner().uuid;
        let (tx, rx) = flume::bounded(1);
        self.orchestrator_tx
            .send_async(OrchestratorMessage::GetNode {
                uuid: node_uuid,
                callback: tx,
            })
            .await
            .unwrap();
        let node = rx.recv_async().await.unwrap();
        let resp = GetLibraryNodeResponse { node: Some(node) };
        Ok(Response::new(resp))
    }
    async fn get_track(
        &self,
        request: Request<GetTrackRequest>,
    ) -> Result<Response<GetTrackResponse>, Status> {
        println!("Got a track request: {:?}", request);

        let req = request.into_inner();

        let reply = GetTrackResponse { track: None };
        Ok(Response::new(reply))
    }

    async fn queue_track(
        &self,
        request: tonic::Request<QueueTrackRequest>,
    ) -> std::result::Result<tonic::Response<QueueTrackResponse>, tonic::Status> {
        let reply = QueueTrackResponse {};
        Ok(Response::new(reply))
    }
    async fn queue_library_node(
        &self,
        request: tonic::Request<QueueLibraryNodeRequest>,
    ) -> std::result::Result<tonic::Response<QueueLibraryNodeResponse>, tonic::Status> {
        let reply = QueueLibraryNodeResponse {};
        Ok(Response::new(reply))
    }
    async fn replace_with_track(
        &self,
        request: tonic::Request<ReplaceWithTrackRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceWithTrackResponse>, tonic::Status> {
        let reply = ReplaceWithTrackResponse {};
        Ok(Response::new(reply))
    }
    async fn replace_with_node(
        &self,
        request: tonic::Request<ReplaceWithNodeRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceWithNodeResponse>, tonic::Status> {
        let reply = ReplaceWithNodeResponse {};
        Ok(Response::new(reply))
    }
    async fn append_track(
        &self,
        request: tonic::Request<AppendTrackRequest>,
    ) -> std::result::Result<tonic::Response<AppendTrackResponse>, tonic::Status> {
        let reply = AppendTrackResponse {};
        Ok(Response::new(reply))
    }
    async fn append_node(
        &self,
        request: tonic::Request<AppendNodeRequest>,
    ) -> std::result::Result<tonic::Response<AppendNodeResponse>, tonic::Status> {
        let reply = AppendNodeResponse {};
        Ok(Response::new(reply))
    }
    async fn remove_tracks(
        &self,
        request: tonic::Request<RemoveTracksRequest>,
    ) -> std::result::Result<tonic::Response<RemoveTracksResponse>, tonic::Status> {
        let reply = RemoveTracksResponse {};
        Ok(Response::new(reply))
    }
    async fn set_current_track(
        &self,
        request: tonic::Request<SetCurrentTrackRequest>,
    ) -> std::result::Result<tonic::Response<SetCurrentTrackResponse>, tonic::Status> {
        let reply = SetCurrentTrackResponse {};
        Ok(Response::new(reply))
    }
    async fn get_queue_updates(
        &self,
        request: tonic::Request<GetQueueUpdatesRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetQueueUpdatesStream>, tonic::Status> {
        let queue_vec: Vec<Result<GetQueueUpdatesResponse, Status>> = vec![];
        let output_stream = futures::stream::iter(queue_vec.into_iter());
        Ok(Response::new(output_stream))
    }
    async fn get_queue(
        &self,
        request: tonic::Request<GetQueueRequest>,
    ) -> std::result::Result<tonic::Response<GetQueueResponse>, tonic::Status> {
        let reply = GetQueueResponse { queue: None };
        Ok(Response::new(reply))
    }
    async fn save_queue(
        &self,
        request: tonic::Request<SaveQueueRequest>,
    ) -> std::result::Result<tonic::Response<SaveQueueResponse>, tonic::Status> {
        let reply = SaveQueueResponse {};
        Ok(Response::new(reply))
    }
    /// Playback
    async fn toggle_play(
        &self,
        request: tonic::Request<TogglePlayRequest>,
    ) -> std::result::Result<tonic::Response<TogglePlayResponse>, tonic::Status> {
        let reply = TogglePlayResponse {};
        Ok(Response::new(reply))
    }
    async fn stop(
        &self,
        request: tonic::Request<StopRequest>,
    ) -> std::result::Result<tonic::Response<StopResponse>, tonic::Status> {
        let reply = StopResponse {};
        Ok(Response::new(reply))
    }
    async fn get_active_track(
        &self,
        request: tonic::Request<GetActiveTrackRequest>,
    ) -> std::result::Result<tonic::Response<GetActiveTrackResponse>, tonic::Status> {
        let reply = GetActiveTrackResponse {
            track: None,
            play_state: TrackPlayState::Stopped as i32,
            completion: 0,
        };
        Ok(Response::new(reply))
    }
    async fn get_track_updates(
        &self,
        request: tonic::Request<GetTrackUpdatesRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetTrackUpdatesStream>, tonic::Status> {
        let track_vec: Vec<Result<GetTrackUpdatesResponse, Status>> = vec![];
        let output_stream = futures::stream::iter(track_vec.into_iter());
        Ok(Response::new(output_stream))
    }
}

// #[derive(Debug)]
// enum Input {
//     PlayTrack {
//         track_id: String,
//     },
//     StopTrack {
//         track_id: String,
//     },
//     GetTrack {
//         track_id: String,
//         response: tokio::sync::oneshot::Sender<tidaldy::Track>,
//     },
//     GetPlaylistList {
//         response: tokio::sync::oneshot::Sender<Vec<tidaldy::PlaylistAndFavorite>>,
//     },
//     TrackOver,
// }

// async fn run() -> Result<(), Error> {
//     gstreamer::init().unwrap();

//     let play = Play::new(None::<PlayVideoRenderer>);
//     let bus = play.message_bus();
//     let (tx, rx) = flume::bounded(64);
//     let bus_tx = tx.clone();
//     bus.set_sync_handler(move |_, msg| {
//         match PlayMessage::parse(msg) {
//             Ok(PlayMessage::EndOfStream) => {}
//             Ok(PlayMessage::StateChanged { state }) => {
//                 println!("State changed: {:?}", state);
//             }
//             Ok(PlayMessage::PositionUpdated { position }) => {
//                 println!("Position updated: {:?}", position);
//             }
//             _ => {}
//         }
//         gstreamer::BusSyncReply::Drop
//     });
//     let mut state = PlayState::Stopped;
//     CHANNEL.set(tx).unwrap();

//     while let Ok(input) = rx.recv_async().await {
//         match (&mut state, input) {
//             (_, Input::TrackOver) => {
//                 state = PlayState::Stopped;
//                 println!("Track stopped");
//             }
//             (_, Input::StopTrack { track_id }) => {
//                 println!("Stopping track {}", track_id);
//                 play.stop();
//                 state = PlayState::Stopped;
//             }
//             (_, Input::GetTrack { track_id, response }) => {
//                 let track = client.get_track(track_id).await.unwrap();
//                 response.send(track).unwrap();
//             }
//             (_, Input::GetPlaylistList { response }) => {
//                 println!("Getting playlists");
//                 let user_id = client.get_user_id().unwrap();
//                 println!("Getting playlists for user {}", user_id);
//                 let list = client
//                     .get_users_playlists_and_favorite_playlists(&user_id)
//                     .await
//                     .unwrap();
//                 response.send(list).unwrap();
//             }
//             (PlayState::Stopped, Input::PlayTrack { track_id }) => {
//                 println!("Playing track {}", track_id);
//                 let track_playback = client.get_track_playback(&track_id).await.unwrap();
//                 let manifest = track_playback.get_manifest().unwrap();
//                 play.set_uri(Some(&manifest.urls[0]));
//                 play.play();
//                 state = PlayState::Playing;
//             }
//             (PlayState::Paused, Input::PlayTrack { track_id }) => {
//                 println!("Unpausing track {}", track_id);
//                 play.play();
//                 state = PlayState::Playing;
//             }
//             (PlayState::Playing, Input::PlayTrack { track_id }) => {
//                 println!("Pausing track {}", track_id);
//                 play.pause();
//                 state = PlayState::Paused;
//             }
//             _ => {}
//         }
//     }
//     print!("done");
//     Ok(())
// }
