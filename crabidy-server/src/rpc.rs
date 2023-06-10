use crate::{PlaybackMessage, ProviderMessage};
use crabidy_core::proto::crabidy::{
    crabidy_service_server::CrabidyService, get_update_stream_response::Update as StreamUpdate,
    AppendRequest, AppendResponse, ChangeVolumeRequest, ChangeVolumeResponse,
    GetLibraryNodeRequest, GetLibraryNodeResponse, GetUpdateStreamRequest, GetUpdateStreamResponse,
    InitRequest, InitResponse, InsertRequest, InsertResponse, NextRequest, NextResponse,
    PrevRequest, PrevResponse, QueueRequest, QueueResponse, RemoveRequest, RemoveResponse,
    ReplaceRequest, ReplaceResponse, RestartTrackRequest, RestartTrackResponse, SaveQueueRequest,
    SaveQueueResponse, SetCurrentRequest, SetCurrentResponse, StopRequest, StopResponse,
    ToggleMuteRequest, ToggleMuteResponse, TogglePlayRequest, TogglePlayResponse,
    ToggleRepeatRequest, ToggleRepeatResponse, ToggleShuffleRequest, ToggleShuffleResponse,
};
use futures::TryStreamExt;
use std::pin::Pin;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{debug, debug_span, error, instrument, trace, Instrument, Span};

#[derive(Debug)]
pub struct RpcService {
    update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
    playback_tx: flume::Sender<PlaybackMessage>,
    provider_tx: flume::Sender<ProviderMessage>,
}

impl RpcService {
    pub fn new(
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

    #[instrument(skip(self, _request))]
    async fn init(&self, _request: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        debug!("Received init request");
        let playback_tx = self.playback_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let span = debug_span!("play-chan");
        if let Err(err) = playback_tx
            .send_async(PlaybackMessage::Init { result_tx, span })
            .in_current_span()
            .await
        {
            error!("{:?}", err);
            return Err(Status::internal("Sending Init via internal channel failed"));
        }
        let response = result_rx
            .recv_async()
            .in_current_span()
            .await
            .map_err(|e| {
                error!("{:?}", e);
                Status::internal("Failed to receive response from provider channel")
            })?;
        Ok(Response::new(response))
    }

    #[instrument(skip(self, request), fields(uuid))]
    async fn get_library_node(
        &self,
        request: Request<GetLibraryNodeRequest>,
    ) -> Result<Response<GetLibraryNodeResponse>, Status> {
        let uuid = request.into_inner().uuid;
        Span::current().record("uuid", &uuid);
        debug!("Received get_library_node request");
        let provider_tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let span = debug_span!("prov-chan");
        provider_tx
            .send_async(ProviderMessage::GetLibraryNode {
                uuid,
                result_tx,
                span,
            })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let result = result_rx
            .recv_async()
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to receive response from provider channel"))?;
        match result {
            Ok(node) => Ok(Response::new(GetLibraryNodeResponse { node: Some(node) })),
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }

    #[instrument(skip(self, request), fields(uuids))]
    async fn queue(
        &self,
        request: tonic::Request<QueueRequest>,
    ) -> std::result::Result<tonic::Response<QueueResponse>, tonic::Status> {
        let uuids = request.into_inner().uuids.clone();
        Span::current().record("uuids", format!("{:?}", uuids));
        debug!("Received queue request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Queue { uuids, span })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;

        let reply = QueueResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(uuids))]
    async fn replace(
        &self,
        request: tonic::Request<ReplaceRequest>,
    ) -> std::result::Result<tonic::Response<ReplaceResponse>, tonic::Status> {
        let uuids = request.into_inner().uuids.clone();
        Span::current().record("uuids", format!("{:?}", uuids));
        debug!("Received replace request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Replace { uuids, span })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = ReplaceResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(uuids))]
    async fn append(
        &self,
        request: tonic::Request<AppendRequest>,
    ) -> std::result::Result<tonic::Response<AppendResponse>, tonic::Status> {
        let uuids = request.into_inner().uuids.clone();
        Span::current().record("uuids", format!("{:?}", uuids));
        debug!("Received append request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Append { uuids, span })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = AppendResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(positions))]
    async fn remove(
        &self,
        request: tonic::Request<RemoveRequest>,
    ) -> std::result::Result<tonic::Response<RemoveResponse>, tonic::Status> {
        let positions = request.into_inner().positions;
        Span::current().record("positions", format!("{:?}", positions));
        debug!("Received remove request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Remove { positions, span })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = RemoveResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(uuids, position))]
    async fn insert(
        &self,
        request: tonic::Request<InsertRequest>,
    ) -> std::result::Result<tonic::Response<InsertResponse>, tonic::Status> {
        let req = request.into_inner();
        let uuids = req.uuids.clone();
        let position = req.position;
        Span::current().record("uuids", format!("{:?}", uuids));
        Span::current().record("position", position);
        debug!("Received insert request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Insert {
                position: req.position,
                uuids,
                span,
            })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = InsertResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(position))]
    async fn set_current(
        &self,
        request: tonic::Request<SetCurrentRequest>,
    ) -> std::result::Result<tonic::Response<SetCurrentResponse>, tonic::Status> {
        let position = request.into_inner().position;
        Span::current().record("position", position);
        debug!("Received set_current request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::SetCurrent { position, span })
            .in_current_span()
            .await
            .map_err(|_| Status::internal("Failed to send request via channel"))?;
        let reply = SetCurrentResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn toggle_shuffle(
        &self,
        _request: tonic::Request<ToggleShuffleRequest>,
    ) -> std::result::Result<tonic::Response<ToggleShuffleResponse>, tonic::Status> {
        debug!("Received toggle_shuffle request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::ToggleShuffle { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = ToggleShuffleResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn toggle_repeat(
        &self,
        _request: tonic::Request<ToggleRepeatRequest>,
    ) -> std::result::Result<tonic::Response<ToggleRepeatResponse>, tonic::Status> {
        debug!("Received toggle_repeat request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::ToggleRepeat { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = ToggleRepeatResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn get_update_stream(
        &self,
        _request: tonic::Request<GetUpdateStreamRequest>,
    ) -> std::result::Result<tonic::Response<Self::GetUpdateStreamStream>, tonic::Status> {
        debug!("Received get_update_stream request");
        let update_rx = self.update_tx.subscribe();
        let update_stream = tokio_stream::wrappers::BroadcastStream::new(update_rx);

        let output_stream = update_stream.into_stream().map(|update_result| {
            trace!("Got update: {:?}", update_result);
            match update_result {
                Ok(update) => Ok(GetUpdateStreamResponse {
                    update: Some(update),
                }),
                Err(_) => Err(tonic::Status::new(
                    tonic::Code::Unknown,
                    "Internal channel error",
                )),
            }
        });

        Ok(Response::new(Box::pin(output_stream)))
    }
    #[instrument(skip(self, _request))]
    async fn save_queue(
        &self,
        _request: tonic::Request<SaveQueueRequest>,
    ) -> std::result::Result<tonic::Response<SaveQueueResponse>, tonic::Status> {
        debug!("Received save_queue request");
        let reply = SaveQueueResponse {};
        Ok(Response::new(reply))
    }

    /// Playback
    #[instrument(skip(self, _request))]
    async fn toggle_play(
        &self,
        _request: tonic::Request<TogglePlayRequest>,
    ) -> std::result::Result<tonic::Response<TogglePlayResponse>, tonic::Status> {
        debug!("Received toggle_play request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::TogglePlay { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = TogglePlayResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn stop(
        &self,
        _request: tonic::Request<StopRequest>,
    ) -> std::result::Result<tonic::Response<StopResponse>, tonic::Status> {
        debug!("Received stop request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Stop { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = StopResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, request), fields(delta))]
    async fn change_volume(
        &self,
        request: tonic::Request<ChangeVolumeRequest>,
    ) -> std::result::Result<tonic::Response<ChangeVolumeResponse>, tonic::Status> {
        let delta = request.into_inner().delta;
        Span::current().record("delta", delta);
        debug!("Received change_volume request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::ChangeVolume { delta, span })
            .in_current_span()
            .await
            .unwrap();
        let reply = ChangeVolumeResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn toggle_mute(
        &self,
        _request: tonic::Request<ToggleMuteRequest>,
    ) -> std::result::Result<tonic::Response<ToggleMuteResponse>, tonic::Status> {
        debug!("Received toggle_mute request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::ToggleMute { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = ToggleMuteResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn next(
        &self,
        _request: tonic::Request<NextRequest>,
    ) -> std::result::Result<tonic::Response<NextResponse>, tonic::Status> {
        debug!("Received next request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Next { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = NextResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn prev(
        &self,
        _request: tonic::Request<PrevRequest>,
    ) -> std::result::Result<tonic::Response<PrevResponse>, tonic::Status> {
        debug!("Received prev request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::Prev { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = PrevResponse {};
        Ok(Response::new(reply))
    }

    #[instrument(skip(self, _request))]
    async fn restart_track(
        &self,
        _request: tonic::Request<RestartTrackRequest>,
    ) -> std::result::Result<tonic::Response<RestartTrackResponse>, tonic::Status> {
        debug!("Received restart_track request");
        let playback_tx = self.playback_tx.clone();
        let span = debug_span!("play-chan");
        playback_tx
            .send_async(PlaybackMessage::RestartTrack { span })
            .in_current_span()
            .await
            .unwrap();
        let reply = RestartTrackResponse {};
        Ok(Response::new(reply))
    }
}
