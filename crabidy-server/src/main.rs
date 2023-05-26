use async_trait::async_trait;
use crabidy_core::proto::crabidy::{
    crabidy_service_server::{CrabidyService, CrabidyServiceServer},
    get_queue_updates_response::QueueUpdateResult,
    ActiveTrack, AppendNodeRequest, AppendNodeResponse, AppendTrackRequest, AppendTrackResponse,
    GetActiveTrackRequest, GetActiveTrackResponse, GetLibraryNodeRequest, GetLibraryNodeResponse,
    GetQueueRequest, GetQueueResponse, GetQueueUpdatesRequest, GetQueueUpdatesResponse,
    GetTrackRequest, GetTrackResponse, GetTrackUpdatesRequest, GetTrackUpdatesResponse,
    LibraryNode, LibraryNodeChild, Queue, QueueLibraryNodeRequest, QueueLibraryNodeResponse,
    QueuePositionChange, QueueTrackRequest, QueueTrackResponse, RemoveTracksRequest,
    RemoveTracksResponse, ReplaceWithNodeRequest, ReplaceWithNodeResponse, ReplaceWithTrackRequest,
    ReplaceWithTrackResponse, SaveQueueRequest, SaveQueueResponse, SetCurrentTrackRequest,
    SetCurrentTrackResponse, StopRequest, StopResponse, TogglePlayRequest, TogglePlayResponse,
    Track,
};
use crabidy_core::{ProviderClient, ProviderError};
use futures::TryStreamExt;
use gstreamer_play::{Play, PlayMessage, PlayState, PlayVideoRenderer};
use tokio_stream::StreamExt;

use std::{
    fs,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tonic::{transport::Server, Request, Response, Result, Status};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (queue_update_tx, queue_update_rx) = flume::bounded(10);
    let (active_track_tx, active_track_rx) = flume::bounded(1000);
    let orchestrator = ProviderOrchestrator::init("").await.unwrap();

    let playback = Playback::new(
        active_track_tx.clone(),
        queue_update_tx.clone(),
        orchestrator.provider_tx.clone(),
    );

    let bus = playback.play.message_bus();
    let playback_tx = playback.playback_tx.clone();

    bus.set_sync_handler(move |_, msg| {
        match PlayMessage::parse(msg) {
            Ok(PlayMessage::EndOfStream) => {
                playback_tx.send(PlaybackMessage::Next).unwrap();
            }
            Ok(PlayMessage::StateChanged { state }) => {
                playback_tx
                    .send(PlaybackMessage::StateChanged { state })
                    .unwrap();
            }
            Ok(PlayMessage::PositionUpdated { position }) => {}
            Ok(PlayMessage::Buffering { percent }) => {}
            Ok(PlayMessage::VolumeChanged { volume }) => {}
            Ok(PlayMessage::MuteChanged { muted }) => {}

            Ok(PlayMessage::MediaInfoUpdated { info }) => {}
            _ => println!("Unknown message: {:?}", msg),
        }
        gstreamer::BusSyncReply::Drop
    });
    let crabidy_service = RpcService::new(
        queue_update_rx,
        active_track_rx,
        playback.playback_tx.clone(),
        orchestrator.provider_tx.clone(),
    );
    orchestrator.run();
    playback.run();

    let addr = "[::1]:50051".parse()?;
    Server::builder()
        .add_service(CrabidyServiceServer::new(crabidy_service))
        .serve(addr)
        .await?;

    Ok(())
}

#[derive(Debug)]
enum ProviderMessage {
    GetNode {
        uuid: String,
        result_tx: flume::Sender<Result<LibraryNode, ProviderError>>,
    },
    GetTrack {
        uuid: String,
        result_tx: flume::Sender<Result<Track, ProviderError>>,
    },
    GetTrackUrls {
        uuid: String,
        result_tx: flume::Sender<Result<Vec<String>, ProviderError>>,
    },
    FlattenNode {
        uuid: String,
        result_tx: flume::Sender<Vec<Track>>,
    },
}

#[derive(Debug)]
struct ProviderOrchestrator {
    provider_tx: flume::Sender<ProviderMessage>,
    provider_rx: flume::Receiver<ProviderMessage>,
    // known_tracks: RwLock<HashMap<String, Track>>,
    // known_nodes: RwLock<HashMap<String, LibraryNode>>,
    tidal_client: Arc<tidaldy::Client>,
}

impl ProviderOrchestrator {
    fn run(self) {
        tokio::spawn(async move {
            while let Ok(msg) = self.provider_rx.recv_async().await {
                match msg {
                    ProviderMessage::GetNode { uuid, result_tx } => {
                        let result = self.get_lib_node(&uuid).await;
                        result_tx.send(result).unwrap();
                    }
                    ProviderMessage::GetTrack { uuid, result_tx } => {
                        let result = self.get_metadata_for_track(&uuid).await;
                        result_tx.send(result).unwrap();
                    }
                    ProviderMessage::GetTrackUrls { uuid, result_tx } => {
                        let result = self.get_urls_for_track(&uuid).await;
                        result_tx.send(result).unwrap();
                    }
                    ProviderMessage::FlattenNode { uuid, result_tx } => {
                        let result = self.flatten_node(&uuid).await;
                        result_tx.send(result).unwrap();
                    }
                }
            }
        });
    }
    async fn flatten_node(&self, node_uuid: &str) -> Vec<Track> {
        let mut tracks = Vec::with_capacity(1000);
        let mut nodes_to_go = Vec::with_capacity(100);
        nodes_to_go.push(node_uuid.to_string());
        while let Some(node_uuid) = nodes_to_go.pop() {
            let Ok(node) = self.get_lib_node(&node_uuid).await else {
                    continue
                };
            tracks.extend(node.tracks);
            nodes_to_go.extend(node.children.into_iter().map(|c| c.uuid))
        }
        tracks
    }
}

#[async_trait]
impl ProviderClient for ProviderOrchestrator {
    async fn init(_s: &str) -> Result<Self, ProviderError> {
        gstreamer::init().unwrap();
        let play = Play::new(None::<PlayVideoRenderer>);
        let state = Mutex::new(PlayState::Stopped);
        let queue = Mutex::new(Queue {
            timestamp: 0,
            current: 0,
            tracks: Vec::new(),
        });
        let raw_toml_settings = fs::read_to_string("/tmp/tidaldy.toml").unwrap_or("".to_owned());
        let tidal_client = Arc::new(tidaldy::Client::init(&raw_toml_settings).await.unwrap());
        let new_toml_config = tidal_client.settings();
        fs::write("/tmp/tidaldy.toml", new_toml_config).unwrap();
        // let known_tracks = HashMap::new();
        // let known_nodes = HashMap::new();
        let (provider_tx, provider_rx) = flume::bounded(100);
        Ok(Self {
            provider_rx,
            provider_tx,
            // known_tracks,
            // known_nodes,
            tidal_client,
        })
    }
    fn settings(&self) -> String {
        "".to_owned()
    }
    async fn get_urls_for_track(&self, track_uuid: &str) -> Result<Vec<String>, ProviderError> {
        self.tidal_client.get_urls_for_track(track_uuid).await
    }
    async fn get_metadata_for_track(&self, track_uuid: &str) -> Result<Track, ProviderError> {
        self.tidal_client.get_metadata_for_track(track_uuid).await
    }
    fn get_lib_root(&self) -> LibraryNode {
        let mut root_node = LibraryNode::new();
        let child = LibraryNodeChild::new("tidal".to_owned(), "tidal".to_owned());
        root_node.children.push(child);
        root_node
    }
    async fn get_lib_node(&self, uuid: &str) -> Result<LibraryNode, ProviderError> {
        if uuid == "/" {
            return Ok(self.get_lib_root());
        }
        if uuid == "tidal" {
            return Ok(self.tidal_client.get_lib_root());
        }
        self.tidal_client.get_lib_node(uuid).await
    }
}

#[derive(Debug)]
enum PlaybackMessage {
    ReplaceWithTrack {
        uuid: String,
    },
    ReplaceWithNode {
        uuid: String,
    },
    QueueTrack {
        uuid: String,
    },
    QueueNode {
        uuid: String,
    },
    ClearQueue,
    GetQueue {
        result_tx: flume::Sender<Queue>,
    },
    AppendTrack {
        uuid: String,
    },
    AppendNode {
        uuid: String,
    },
    RemoveTracks {
        positions: Vec<u32>,
    },
    SetCurrent {
        position: u32,
    },
    GetCurrent {
        result_tx: flume::Sender<ActiveTrack>,
    },
    Next,
    PlayPause,
    Stop,
    StateChanged {
        state: PlayState,
    },
}

#[derive(Debug)]
struct Playback {
    active_track_tx: flume::Sender<ActiveTrack>,
    queue_update_tx: flume::Sender<Queue>,
    provider_tx: flume::Sender<ProviderMessage>,
    playback_tx: flume::Sender<PlaybackMessage>,
    playback_rx: flume::Receiver<PlaybackMessage>,
    queue: Mutex<Queue>,
    state: Mutex<PlayState>,
    play: Play,
    creation: std::time::Instant,
}

impl Playback {
    fn new(
        active_track_tx: flume::Sender<ActiveTrack>,
        queue_update_tx: flume::Sender<Queue>,
        provider_tx: flume::Sender<ProviderMessage>,
    ) -> Self {
        let (playback_tx, playback_rx) = flume::bounded(10);
        let queue = Mutex::new(Queue {
            timestamp: 0,
            current: 0,
            tracks: Vec::new(),
        });
        let state = Mutex::new(PlayState::Stopped);
        let play = Play::new(None::<PlayVideoRenderer>);
        let creation = std::time::Instant::now();
        Self {
            active_track_tx,
            queue_update_tx,
            provider_tx,
            playback_tx,
            playback_rx,
            queue,
            state,
            play,
            creation,
        }
    }
    fn run(self) {
        tokio::spawn(async move {
            while let Ok(message) = self.playback_rx.recv_async().await {
                match message {
                    PlaybackMessage::ReplaceWithTrack { uuid } => {
                        if let Ok(track) = self.get_track(&uuid).await {
                            {
                                let mut queue = self.queue.lock().unwrap();
                                queue.replace_with_tracks(&[track.clone()]);
                                let queue_update_tx = self.queue_update_tx.clone();
                                queue_update_tx.send(queue.clone()).unwrap();
                            }
                            self.play(track).await;
                        }
                    }

                    PlaybackMessage::ReplaceWithNode { uuid } => {
                        let tracks = self.flatten_node(&uuid).await;
                        {
                            let mut queue = self.queue.lock().unwrap();
                            queue.replace_with_tracks(&tracks);
                            let queue_update_tx = self.queue_update_tx.clone();
                            queue_update_tx.send(queue.clone()).unwrap();
                        }
                        if !tracks.is_empty() {
                            self.play(tracks[0].clone()).await;
                        }
                    }

                    PlaybackMessage::QueueTrack { uuid } => {
                        if let Ok(track) = self.get_track(&uuid).await {
                            let mut queue = self.queue.lock().unwrap();
                            queue.queue_tracks(&[track]);
                            let queue_update_tx = self.queue_update_tx.clone();
                            queue_update_tx.send(queue.clone()).unwrap();
                        }
                    }
                    PlaybackMessage::QueueNode { uuid } => {
                        let tracks = self.flatten_node(&uuid).await;
                        let mut queue = self.queue.lock().unwrap();
                        queue.queue_tracks(&tracks);
                        let queue_update_tx = self.queue_update_tx.clone();
                        queue_update_tx.send(queue.clone()).unwrap();
                    }

                    PlaybackMessage::GetQueue { result_tx } => {
                        let queue = self.queue.lock().unwrap();
                        result_tx.send(queue.clone()).unwrap();
                    }
                    PlaybackMessage::AppendTrack { uuid } => {
                        if let Ok(track) = self.get_track(&uuid).await {
                            let mut queue = self.queue.lock().unwrap();
                            queue.append_tracks(&[track]);
                            let queue_update_tx = self.queue_update_tx.clone();
                            queue_update_tx.send(queue.clone()).unwrap();
                        }
                    }
                    PlaybackMessage::AppendNode { uuid } => {
                        let tracks = self.flatten_node(&uuid).await;
                        let mut queue = self.queue.lock().unwrap();
                        queue.append_tracks(&tracks);
                        let queue_update_tx = self.queue_update_tx.clone();
                        queue_update_tx.send(queue.clone()).unwrap();
                    }

                    PlaybackMessage::ClearQueue => {
                        let mut queue = self.queue.lock().unwrap();
                        queue.replace_with_tracks(&vec![]);
                        self.stop_track();
                        let queue_update_tx = self.queue_update_tx.clone();
                        queue_update_tx.send(queue.clone()).unwrap();
                    }

                    //TODO handle deletion of current track
                    PlaybackMessage::RemoveTracks { positions } => {
                        let mut queue = self.queue.lock().unwrap();
                        queue.remove_tracks(&positions);
                        let queue_update_tx = self.queue_update_tx.clone();
                        queue_update_tx.send(queue.clone()).unwrap();
                    }

                    PlaybackMessage::SetCurrent { position } => {
                        let result = {
                            let mut queue = self.queue.lock().unwrap();
                            queue.set_current(position);
                            let queue_update_tx = self.queue_update_tx.clone();
                            queue_update_tx.send(queue.clone()).unwrap();
                            queue.current()
                        };

                        if let Some(track) = result {
                            self.play(track).await;
                        }
                    }
                    PlaybackMessage::GetCurrent { result_tx } => {
                        let current = self.get_active_track().await;
                        result_tx.send(current).unwrap();
                    }
                    PlaybackMessage::Next => {
                        let (result, stop) = {
                            let mut queue = self.queue.lock().unwrap();
                            let position = queue.current + 1;
                            let stop = !queue.set_current(position);
                            let queue_update_tx = self.queue_update_tx.clone();
                            queue_update_tx.send(queue.clone()).unwrap();
                            (queue.current(), stop)
                        };

                        if let Some(track) = result {
                            self.play(track).await;
                        }
                        if stop {
                            self.stop_track()
                        }
                    }

                    PlaybackMessage::PlayPause => {
                        let mut state = self.state.lock().unwrap();
                        if *state == PlayState::Playing {
                            self.play.pause();
                            // *state = PlayState::Paused
                        } else {
                            self.play.play();
                            // *state = PlayState::Playing
                        }
                    }
                    PlaybackMessage::Stop => {
                        self.play.stop();
                        // *self.state.lock().unwrap() = PlayState::Stopped;
                    }
                    PlaybackMessage::StateChanged { state } => {
                        *self.state.lock().unwrap() = state.clone();
                        let active_track_tx = self.active_track_tx.clone();
                        let active_track = self.get_active_track().await;
                        active_track_tx.send(active_track).unwrap();
                    }
                }
            }
        });
    }
    async fn flatten_node(&self, uuid: &str) -> Vec<Track> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let Ok(_) = tx.send_async(ProviderMessage::FlattenNode {
            uuid: uuid.to_string(),
            result_tx,
        }).await else {
            return Vec::new();
        };
        let Ok(tracks) = result_rx
            .recv_async()
            .await else {
                return Vec::new();
            };
        tracks
    }
    async fn get_track(&self, uuid: &str) -> Result<Track, ProviderError> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        tx.send_async(ProviderMessage::GetTrack {
            uuid: uuid.to_string(),
            result_tx,
        })
        .await
        .map_err(|_| ProviderError::InternalError)?;
        result_rx
            .recv_async()
            .await
            .map_err(|_| ProviderError::InternalError)?
    }
    async fn get_urls_for_track(&self, uuid: &str) -> Result<Vec<String>, ProviderError> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        tx.send_async(ProviderMessage::GetTrackUrls {
            uuid: uuid.to_string(),
            result_tx,
        })
        .await
        .map_err(|_| ProviderError::InternalError)?;
        result_rx
            .recv_async()
            .await
            .map_err(|_| ProviderError::InternalError)?
    }
    async fn get_active_track(&self) -> ActiveTrack {
        let result = {
            let mut queue = self.queue.lock().unwrap();
            queue.current()
        };
        let completion = 0;
        let gst_play_state = self.state.lock().unwrap();
        let play_state = match *gst_play_state {
            PlayState::Stopped => crabidy_core::proto::crabidy::TrackPlayState::Stopped,
            PlayState::Buffering => crabidy_core::proto::crabidy::TrackPlayState::Loading,
            PlayState::Playing => crabidy_core::proto::crabidy::TrackPlayState::Playing,
            PlayState::Paused => crabidy_core::proto::crabidy::TrackPlayState::Paused,
            _ => crabidy_core::proto::crabidy::TrackPlayState::Unspecified,
        };
        let play_state = play_state as i32;

        ActiveTrack {
            track: result,
            completion,
            play_state,
        }
    }

    async fn play(&self, track: Track) {
        let Ok(urls) = self.get_urls_for_track(&track.uuid).await else {
            let playback_tx = self.playback_tx.clone();
            playback_tx.send(PlaybackMessage::Next).unwrap();
            return
        };
        {
            let mut state_guard = self.state.lock().unwrap();
            *state_guard = PlayState::Playing;
        }
        self.play.set_uri(Some(&urls[0]));
        self.play.play();
    }

    fn stop_track(&self) {
        {
            let mut state_guard = self.state.lock().unwrap();
            *state_guard = PlayState::Stopped;
        }
        self.play.stop();
    }

    fn playpause(&self) {
        let mut state_guard = self.state.lock().unwrap();
        if *state_guard == PlayState::Playing {
            *state_guard = PlayState::Paused;
            self.play.pause();
        } else {
            *state_guard = PlayState::Playing;
            self.play.play()
        }
    }
}

#[derive(Debug)]
struct RpcService {
    queue_update_rx: flume::Receiver<Queue>,
    active_track_rx: flume::Receiver<ActiveTrack>,
    playback_tx: flume::Sender<PlaybackMessage>,
    provider_tx: flume::Sender<ProviderMessage>,
}

impl RpcService {
    fn new(
        queue_update_rx: flume::Receiver<Queue>,
        active_track_rx: flume::Receiver<ActiveTrack>,
        playback_tx: flume::Sender<PlaybackMessage>,
        provider_tx: flume::Sender<ProviderMessage>,
    ) -> Self {
        Self {
            queue_update_rx,
            active_track_rx,
            playback_tx,
            provider_tx,
        }
    }
}

#[tonic::async_trait]
impl CrabidyService for RpcService {
    type GetQueueUpdatesStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<GetQueueUpdatesResponse, Status>> + Send>>;

    type GetTrackUpdatesStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<GetTrackUpdatesResponse, Status>> + Send>>;

    async fn get_library_node(
        &self,
        request: Request<GetLibraryNodeRequest>,
    ) -> Result<Response<GetLibraryNodeResponse>, Status> {
        let provider_tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);

        provider_tx
            .send_async(ProviderMessage::GetNode {
                uuid: request.into_inner().uuid,
                result_tx,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let result = result_rx
            .recv_async()
            .await
            .map_err(|_| Status::internal("Failed to receive response from provider channel"))?;
        match result {
            Ok(node) => Ok(Response::new(GetLibraryNodeResponse { node: Some(node) })),
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }
    async fn get_track(
        &self,
        request: Request<GetTrackRequest>,
    ) -> Result<Response<GetTrackResponse>, Status> {
        let provider_tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);

        provider_tx
            .send_async(ProviderMessage::GetTrack {
                uuid: request.into_inner().uuid,
                result_tx,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let result = result_rx
            .recv_async()
            .await
            .map_err(|_| Status::internal("Failed to receive response from provider channel"))?;
        match result {
            Ok(track) => Ok(Response::new(GetTrackResponse { track: Some(track) })),
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }

    async fn queue_track(
        &self,
        request: tonic::Request<QueueTrackRequest>,
    ) -> std::result::Result<tonic::Response<QueueTrackResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::QueueTrack {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;

        let reply = QueueTrackResponse {};
        Ok(Response::new(reply))
    }

    async fn queue_library_node(
        &self,
        request: tonic::Request<QueueLibraryNodeRequest>,
    ) -> std::result::Result<tonic::Response<QueueLibraryNodeResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::QueueNode {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = QueueLibraryNodeResponse {};
        Ok(Response::new(reply))
    }

    async fn replace_with_track(
        &self,
        request: tonic::Request<ReplaceWithTrackRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceWithTrackResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::ReplaceWithTrack {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = ReplaceWithTrackResponse {};
        Ok(Response::new(reply))
    }

    async fn replace_with_node(
        &self,
        request: tonic::Request<ReplaceWithNodeRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceWithNodeResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::ReplaceWithNode {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = ReplaceWithNodeResponse {};
        Ok(Response::new(reply))
    }

    async fn append_track(
        &self,
        request: tonic::Request<AppendTrackRequest>,
    ) -> std::result::Result<tonic::Response<AppendTrackResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::AppendTrack {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = AppendTrackResponse {};
        Ok(Response::new(reply))
    }

    async fn append_node(
        &self,
        request: tonic::Request<AppendNodeRequest>,
    ) -> std::result::Result<tonic::Response<AppendNodeResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::AppendNode {
                uuid: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = AppendNodeResponse {};
        Ok(Response::new(reply))
    }

    async fn remove_tracks(
        &self,
        request: tonic::Request<RemoveTracksRequest>,
    ) -> std::result::Result<tonic::Response<RemoveTracksResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::RemoveTracks {
                positions: req.positions,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = RemoveTracksResponse {};
        Ok(Response::new(reply))
    }

    async fn set_current_track(
        &self,
        request: tonic::Request<SetCurrentTrackRequest>,
    ) -> std::result::Result<tonic::Response<SetCurrentTrackResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::SetCurrent {
                position: req.position,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = SetCurrentTrackResponse {};
        Ok(Response::new(reply))
    }

    async fn get_queue_updates(
        &self,
        request: tonic::Request<GetQueueUpdatesRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetQueueUpdatesStream>, tonic::Status> {
        let update_rx = self.queue_update_rx.clone();

        let output_stream = update_rx.into_stream().map(map_queue);
        Ok(Response::new(Box::pin(output_stream)))
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
        let playback_tx = self.playback_tx.clone();
        playback_tx
            .send_async(PlaybackMessage::PlayPause)
            .await
            .unwrap();
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
            active_track: None,
            // track: None,
            // play_state: TrackPlayState::Stopped as i32,
            // completion: 0,
        };
        Ok(Response::new(reply))
    }

    async fn get_track_updates(
        &self,
        request: tonic::Request<GetTrackUpdatesRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetTrackUpdatesStream>, tonic::Status> {
        let update_rx = self.active_track_rx.clone();

        let output_stream = update_rx.into_stream().map(|active_track| {
            Ok(GetTrackUpdatesResponse {
                active_track: Some(active_track),
            })
        });
        Ok(Response::new(Box::pin(output_stream)))
    }
}

fn map_queue(queue: Queue) -> Result<GetQueueUpdatesResponse, Status> {
    Ok(GetQueueUpdatesResponse {
        queue_update_result: Some(QueueUpdateResult::Full(queue)),
    })
}
