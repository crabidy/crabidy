use async_trait::async_trait;
use crabidy_core::proto::crabidy::{
    crabidy_service_server::{CrabidyService, CrabidyServiceServer},
    get_update_stream_response::Update as StreamUpdate,
    AppendRequest, AppendResponse, ChangeVolumeRequest, ChangeVolumeResponse,
    GetLibraryNodeRequest, GetLibraryNodeResponse, GetUpdateStreamRequest, GetUpdateStreamResponse,
    InitRequest, InitResponse, InsertRequest, InsertResponse, LibraryNode, LibraryNodeChild,
    NextRequest, NextResponse, PlayState, PrevRequest, PrevResponse, Queue, QueueRequest,
    QueueResponse, QueueTrack, RemoveRequest, RemoveResponse, ReplaceRequest, ReplaceResponse,
    RestartTrackRequest, RestartTrackResponse, SaveQueueRequest, SaveQueueResponse,
    SetCurrentRequest, SetCurrentResponse, StopRequest, StopResponse, ToggleMuteRequest,
    ToggleMuteResponse, TogglePlayRequest, TogglePlayResponse, ToggleShuffleRequest,
    ToggleShuffleResponse, Track,
};
use crabidy_core::{ProviderClient, ProviderError};
use futures::TryStreamExt;
use gstreamer_play::{Play, PlayMessage, PlayState as GstPlaystate, PlayVideoRenderer};
use tokio_stream::StreamExt;

use std::{
    fs,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tonic::{transport::Server, Request, Response, Result, Status};

fn poll_play_bus(bus: gstreamer::Bus, tx: flume::Sender<PlaybackMessage>) {
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match PlayMessage::parse(&msg) {
            Ok(PlayMessage::EndOfStream) => {
                tx.send(PlaybackMessage::Next).unwrap();
            }
            Ok(PlayMessage::StateChanged { state }) => {
                tx.send(PlaybackMessage::StateChanged { state }).unwrap();
            }
            Ok(PlayMessage::PositionUpdated { position }) => {}
            Ok(PlayMessage::Buffering { percent }) => {}
            Ok(PlayMessage::VolumeChanged { volume }) => {}
            Ok(PlayMessage::MuteChanged { muted }) => {}

            Ok(PlayMessage::MediaInfoUpdated { info }) => {}
            _ => println!("Unknown message: {:?}", msg),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    gstreamer::init()?;

    let (update_tx, _) = tokio::sync::broadcast::channel(2048);
    let orchestrator = ProviderOrchestrator::init("").await.unwrap();

    let playback = Playback::new(update_tx.clone(), orchestrator.provider_tx.clone());

    let bus = playback.play.message_bus();
    let playback_tx = playback.playback_tx.clone();

    std::thread::spawn(|| {
        poll_play_bus(bus, playback_tx);
    });

    let crabidy_service = RpcService::new(
        update_tx,
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
        let state = Mutex::new(PlayState::Stopped);
        let queue = Mutex::new(Queue {
            timestamp: 0,
            current_position: 0,
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
        let child = LibraryNodeChild::new("node:tidal".to_owned(), "tidal".to_owned());
        root_node.children.push(child);
        println!("Global root node {:?}", root_node);
        root_node
    }
    async fn get_lib_node(&self, uuid: &str) -> Result<LibraryNode, ProviderError> {
        println!("get_lib_node {}", uuid);
        if uuid == "node:/" {
            return Ok(self.get_lib_root());
        }
        if uuid == "node:tidal" {
            return Ok(self.tidal_client.get_lib_root());
        }
        self.tidal_client.get_lib_node(uuid).await
    }
}

#[derive(Debug)]
enum PlaybackMessage {
    Replace {
        uuids: Vec<String>,
    },
    Queue {
        uuids: Vec<String>,
    },
    Append {
        uuids: Vec<String>,
    },
    Remove {
        positions: Vec<u32>,
    },
    Insert {
        position: u32,
        uuids: Vec<String>,
    },
    SetCurrent {
        position: u32,
    },
    GetQueue {
        result_tx: flume::Sender<Queue>,
    },
    GetQueueTrack {
        result_tx: flume::Sender<QueueTrack>,
    },
    TogglePlay,
    ToggleShuffle,
    Stop,
    ChangeVolume {
        delta: f32,
    },
    ToggleMute,
    Next,
    Prev,
    StateChanged {
        state: GstPlaystate,
    },
}

#[derive(Debug)]
struct Playback {
    update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
    provider_tx: flume::Sender<ProviderMessage>,
    playback_tx: flume::Sender<PlaybackMessage>,
    playback_rx: flume::Receiver<PlaybackMessage>,
    queue: Mutex<Queue>,
    state: Mutex<GstPlaystate>,
    play: Play,
    creation: std::time::Instant,
}

impl Playback {
    fn new(
        update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
        provider_tx: flume::Sender<ProviderMessage>,
    ) -> Self {
        let (playback_tx, playback_rx) = flume::bounded(10);
        let queue = Mutex::new(Queue {
            timestamp: 0,
            current_position: 0,
            tracks: Vec::new(),
        });
        let state = Mutex::new(GstPlaystate::Stopped);
        let play = Play::new(None::<PlayVideoRenderer>);
        let creation = std::time::Instant::now();
        Self {
            update_tx,
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
                    PlaybackMessage::Replace { uuids } => {
                        println!("Replace {:?}", uuids);
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                println!("Track {}", uuid);
                                if let Ok(track) = self.get_track(&uuid).await {
                                    all_tracks.push(track);
                                }
                            } else {
                                println!("Node {}", uuid);
                                let tracks = self.flatten_node(&uuid).await;
                                all_tracks.extend(tracks);
                            }
                        }
                        let current = {
                            let mut queue = self.queue.lock().unwrap();
                            queue.set_current_position(0);
                            queue.replace_with_tracks(&all_tracks);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            if let Err(err) = queue_update_tx.send(update) {
                                println!("{:?}", err)
                            }
                            queue.current()
                        };
                        if let Some(track) = current {
                            self.play(track).await;
                        }
                    }

                    PlaybackMessage::Queue { uuids } => {
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).await {
                                    let mut queue = self.queue.lock().unwrap();
                                    queue.queue_tracks(&[track]);
                                    let queue_update_tx = self.update_tx.clone();
                                    let update = StreamUpdate::Queue(queue.clone());
                                    if let Err(err) = queue_update_tx.send(update) {
                                        println!("{:?}", err)
                                    };
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).await;
                                let mut queue = self.queue.lock().unwrap();
                                queue.queue_tracks(&tracks);
                                let queue_update_tx = self.update_tx.clone();
                                let update = StreamUpdate::Queue(queue.clone());
                                if let Err(err) = queue_update_tx.send(update) {
                                    println!("{:?}", err)
                                };
                            }
                        }
                    }

                    PlaybackMessage::Append { uuids } => {
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).await {
                                    let mut queue = self.queue.lock().unwrap();
                                    queue.append_tracks(&[track]);
                                    let queue_update_tx = self.update_tx.clone();
                                    let update = StreamUpdate::Queue(queue.clone());

                                    if let Err(err) = queue_update_tx.send(update) {
                                        println!("{:?}", err)
                                    };
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).await;
                                let mut queue = self.queue.lock().unwrap();
                                queue.append_tracks(&tracks);
                                let queue_update_tx = self.update_tx.clone();
                                let update = StreamUpdate::Queue(queue.clone());
                                queue_update_tx.send(update).unwrap();
                            }
                        }
                    }

                    //TODO handle deletion of current track
                    PlaybackMessage::Remove { positions } => {
                        let mut queue = self.queue.lock().unwrap();
                        queue.remove_tracks(&positions);
                        let queue_update_tx = self.update_tx.clone();
                        let update = StreamUpdate::Queue(queue.clone());
                        queue_update_tx.send(update).unwrap();
                    }

                    PlaybackMessage::Insert { position, uuids } => {
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).await {
                                    all_tracks.push(track);
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).await;
                                all_tracks.extend(tracks);
                            }
                        }
                        let mut queue = self.queue.lock().unwrap();
                        queue.insert_tracks(position, &all_tracks);
                        let queue_update_tx = self.update_tx.clone();
                        let update = StreamUpdate::Queue(queue.clone());
                        queue_update_tx.send(update).unwrap();
                    }

                    PlaybackMessage::SetCurrent {
                        position: queue_position,
                    } => {
                        let track = {
                            let mut queue = self.queue.lock().unwrap();
                            queue.set_current_position(queue_position);
                            queue.current()
                        };
                        let queue_update_tx = self.update_tx.clone();
                        let update = StreamUpdate::QueueTrack(QueueTrack {
                            queue_position,
                            track: track.clone(),
                        });

                        if let Err(err) = queue_update_tx.send(update) {
                            println!("{:?}", err)
                        };
                        if let Some(track) = track {
                            self.play(track).await;
                        }
                    }

                    PlaybackMessage::GetQueue { result_tx } => {
                        let queue = self.queue.lock().unwrap();
                        result_tx.send(queue.clone()).unwrap();
                    }

                    PlaybackMessage::GetQueueTrack { result_tx } => {
                        let current = self.get_queue_track().await;
                        result_tx.send(current).unwrap();
                    }

                    PlaybackMessage::TogglePlay => {
                        let mut state = self.state.lock().unwrap();
                        if *state == GstPlaystate::Playing {
                            self.play.pause();
                        } else {
                            self.play.play();
                        }
                    }

                    PlaybackMessage::Stop => {
                        self.play.stop();
                    }

                    PlaybackMessage::ChangeVolume { delta } => {
                        let volume = self.play.volume();
                        self.play.set_volume(volume + delta as f64);
                    }

                    PlaybackMessage::ToggleMute => {
                        let muted = self.play.is_muted();
                        self.play.set_mute(!muted);
                    }

                    PlaybackMessage::ToggleShuffle => {
                        todo!()
                    }

                    PlaybackMessage::Next => {
                        let (result, stop, pos) = {
                            let mut queue = self.queue.lock().unwrap();
                            let position = queue.current_position + 1;
                            let stop = !queue.set_current_position(position);
                            (queue.current(), stop, position)
                        };
                        let queue_update_tx = self.update_tx.clone();
                        let update = StreamUpdate::QueueTrack(QueueTrack {
                            queue_position: pos,
                            track: result.clone(),
                        });
                        if let Err(err) = queue_update_tx.send(update) {
                            println!("{:?}", err)
                        };

                        if let Some(track) = result {
                            self.play(track).await;
                        }
                        if stop {
                            self.stop_track()
                        }
                    }
                    PlaybackMessage::Prev => {
                        let (result, stop, pos) = {
                            let mut queue = self.queue.lock().unwrap();
                            let position = queue.current_position - 1;
                            let stop = !queue.set_current_position(position);
                            (queue.current(), stop, position)
                        };
                        let update = StreamUpdate::QueueTrack(QueueTrack {
                            queue_position: pos,
                            track: result.clone(),
                        });
                        let queue_update_tx = self.update_tx.clone();
                        if let Err(err) = queue_update_tx.send(update) {
                            println!("{:?}", err)
                        };

                        if let Some(track) = result {
                            self.play(track).await;
                        }
                        if stop {
                            self.stop_track()
                        }
                    }

                    PlaybackMessage::StateChanged { state } => {
                        *self.state.lock().unwrap() = state.clone();
                        let active_track_tx = self.update_tx.clone();
                        let active_track = self.get_queue_track().await;
                        let play_state = match state {
                            GstPlaystate::Playing => PlayState::Playing,
                            GstPlaystate::Paused => PlayState::Paused,
                            GstPlaystate::Stopped => PlayState::Stopped,
                            GstPlaystate::Buffering => PlayState::Loading,
                            _ => PlayState::Unspecified,
                        };
                        let update = StreamUpdate::PlayState(play_state as i32);
                        if let Err(err) = active_track_tx.send(update) {
                            println!("{:?}", err)
                        };
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
    async fn get_queue_track(&self) -> QueueTrack {
        let queue_position: u32;
        let result = {
            let mut queue = self.queue.lock().unwrap();
            queue_position = queue.current_position;
            queue.current()
        };
        let completion = 0;
        let gst_play_state = self.state.lock().unwrap();
        let play_state = match *gst_play_state {
            GstPlaystate::Stopped => PlayState::Stopped,
            GstPlaystate::Buffering => PlayState::Loading,
            GstPlaystate::Playing => PlayState::Playing,
            GstPlaystate::Paused => PlayState::Paused,
            _ => PlayState::Unspecified,
        };
        let play_state = play_state as i32;

        QueueTrack {
            queue_position,
            track: result,
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
            *state_guard = GstPlaystate::Playing;
        }
        self.play.stop();
        self.play.set_uri(Some(&urls[0]));
        self.play.play();
    }

    fn stop_track(&self) {
        {
            let mut state_guard = self.state.lock().unwrap();
            *state_guard = GstPlaystate::Stopped;
        }
        self.play.stop();
    }

    fn playpause(&self) {
        let mut state_guard = self.state.lock().unwrap();
        if *state_guard == GstPlaystate::Playing {
            *state_guard = GstPlaystate::Paused;
            self.play.pause();
        } else {
            *state_guard = GstPlaystate::Playing;
            self.play.play()
        }
    }
}

#[derive(Debug)]
struct RpcService {
    update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
    playback_tx: flume::Sender<PlaybackMessage>,
    provider_tx: flume::Sender<ProviderMessage>,
}

impl RpcService {
    fn new(
        update_rx: tokio::sync::broadcast::Sender<StreamUpdate>,
        playback_tx: flume::Sender<PlaybackMessage>,
        provider_tx: flume::Sender<ProviderMessage>,
    ) -> Self {
        Self {
            update_tx: update_rx,
            playback_tx,
            provider_tx,
        }
    }
}

#[tonic::async_trait]
impl CrabidyService for RpcService {
    type GetUpdateStreamStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<GetUpdateStreamResponse, Status>> + Send>>;

    async fn init(&self, request: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        todo!()
    }

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

    async fn queue(
        &self,
        request: tonic::Request<QueueRequest>,
    ) -> std::result::Result<tonic::Response<QueueResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::Queue {
                uuids: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;

        let reply = QueueResponse {};
        Ok(Response::new(reply))
    }

    async fn replace(
        &self,
        request: tonic::Request<ReplaceRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::Replace {
                uuids: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = ReplaceResponse {};
        Ok(Response::new(reply))
    }

    async fn append(
        &self,
        request: tonic::Request<AppendRequest>,
    ) -> std::result::Result<tonic::Response<AppendResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::Append {
                uuids: req.uuid.clone(),
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = AppendResponse {};
        Ok(Response::new(reply))
    }

    async fn remove(
        &self,
        request: tonic::Request<RemoveRequest>,
    ) -> std::result::Result<tonic::Response<RemoveResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::Remove {
                positions: req.positions,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = RemoveResponse {};
        Ok(Response::new(reply))
    }

    async fn insert(
        &self,
        request: tonic::Request<InsertRequest>,
    ) -> std::result::Result<tonic::Response<InsertResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::Insert {
                position: req.position,
                uuids: req.uuid,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = InsertResponse {};
        Ok(Response::new(reply))
    }

    async fn set_current(
        &self,
        request: tonic::Request<SetCurrentRequest>,
    ) -> std::result::Result<tonic::Response<SetCurrentResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        let req = request.into_inner();
        playback_tx
            .send_async(PlaybackMessage::SetCurrent {
                position: req.position,
            })
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = SetCurrentResponse {};
        Ok(Response::new(reply))
    }

    async fn get_update_stream(
        &self,
        request: tonic::Request<GetUpdateStreamRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetUpdateStreamStream>, tonic::Status> {
        let update_rx = self.update_tx.subscribe();
        let update_stream = tokio_stream::wrappers::BroadcastStream::new(update_rx);

        let output_stream = update_stream
            .into_stream()
            .map(|update_result| match update_result {
                Ok(update) => Ok(GetUpdateStreamResponse {
                    update: Some(update),
                }),
                Err(_) => Err(tonic::Status::new(
                    tonic::Code::Unknown,
                    "Internal channel error",
                )),
            });

        Ok(Response::new(Box::pin(output_stream)))
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
            .send_async(PlaybackMessage::TogglePlay)
            .await
            .unwrap();
        let reply = TogglePlayResponse {};
        Ok(Response::new(reply))
    }

    async fn toggle_shuffle(
        &self,
        request: tonic::Request<ToggleShuffleRequest>,
    ) -> std::result::Result<tonic::Response<ToggleShuffleResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx
            .send_async(PlaybackMessage::ToggleShuffle)
            .await
            .unwrap();
        let reply = ToggleShuffleResponse {};
        Ok(Response::new(reply))
    }

    async fn stop(
        &self,
        request: tonic::Request<StopRequest>,
    ) -> std::result::Result<tonic::Response<StopResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx.send_async(PlaybackMessage::Stop).await.unwrap();
        let reply = StopResponse {};
        Ok(Response::new(reply))
    }

    async fn change_volume(
        &self,
        request: tonic::Request<ChangeVolumeRequest>,
    ) -> std::result::Result<tonic::Response<ChangeVolumeResponse>, tonic::Status> {
        let delta = request.into_inner().delta;
        let playback_tx = self.playback_tx.clone();
        playback_tx
            .send_async(PlaybackMessage::ChangeVolume { delta })
            .await
            .unwrap();
        let reply = ChangeVolumeResponse {};
        Ok(Response::new(reply))
    }

    async fn toggle_mute(
        &self,
        request: tonic::Request<ToggleMuteRequest>,
    ) -> std::result::Result<tonic::Response<ToggleMuteResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx
            .send_async(PlaybackMessage::ToggleMute)
            .await
            .unwrap();
        let reply = ToggleMuteResponse {};
        Ok(Response::new(reply))
    }

    async fn next(
        &self,
        request: tonic::Request<NextRequest>,
    ) -> std::result::Result<tonic::Response<NextResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx.send_async(PlaybackMessage::Next).await.unwrap();
        let reply = NextResponse {};
        Ok(Response::new(reply))
    }

    async fn prev(
        &self,
        request: tonic::Request<PrevRequest>,
    ) -> std::result::Result<tonic::Response<PrevResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx.send_async(PlaybackMessage::Prev).await.unwrap();
        let reply = PrevResponse {};
        Ok(Response::new(reply))
    }

    async fn restart_track(
        &self,
        request: tonic::Request<RestartTrackRequest>,
    ) -> std::result::Result<tonic::Response<RestartTrackResponse>, tonic::Status> {
        let playback_tx = self.playback_tx.clone();
        playback_tx.send_async(PlaybackMessage::Prev).await.unwrap();
        let reply = RestartTrackResponse {};
        Ok(Response::new(reply))
    }
}

fn is_track(uuid: &str) -> bool {
    uuid.starts_with("track:")
}

fn is_node(uuid: &str) -> bool {
    uuid.starts_with("node:")
}
