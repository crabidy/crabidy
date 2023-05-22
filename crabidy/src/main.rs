use std::{pin::Pin, vec};

use crabidy_core::proto::crabidy::{
    crabidy_service_server::{CrabidyService, CrabidyServiceServer},
    AppendNodeRequest, AppendTrackRequest, EmptyRequest, EmptyResponse, GetActiveTrackResponse,
    GetLibraryNodeRequest, GetLibraryNodeResponse, GetQueueResponse, GetQueueUpdatesRequest,
    GetQueueUpdatesResponse, GetTrackRequest, GetTrackResponse, GetTrackUpdatesRequest,
    GetTrackUpdatesResponse, LibraryNode, LibraryNodeState, QueueLibraryNodeRequest,
    QueueTrackRequest, RemoveTracksRequest, ReplaceWithNodeRequest, ReplaceWithTrackRequest,
    SaveQueueRequest, SetCurrentTrackRequest,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{codegen::futures_core::Stream, transport::Server, Request, Response, Status};

type GetQueueUpdatesStream =
    Pin<Box<dyn Stream<Item = Result<GetQueueUpdatesResponse, Status>> + Send>>;
type GetTrackUpdatesStream =
    Pin<Box<dyn Stream<Item = Result<GetTrackUpdatesResponse, Status>> + Send>>;

#[derive(Debug, Default)]
pub struct Service {}

#[tonic::async_trait]
impl CrabidyService for Service {
    type GetQueueUpdatesStream = GetQueueUpdatesStream;
    type GetTrackUpdatesStream = GetTrackUpdatesStream;

    async fn get_library_node(
        &self,
        request: Request<GetLibraryNodeRequest>,
    ) -> Result<Response<GetLibraryNodeResponse>, Status> {
        println!("Got a library node request: {:?}", request);

        let req = request.into_inner();

        let reply = GetLibraryNodeResponse {
            node: Some(LibraryNode {
                uuid: "1".to_owned(),
                name: "Tidal".to_owned(),
                children: vec![
                    "Berlin, Texas".to_owned(),
                    "Spacefarer".to_owned(),
                    "Italo Disco".to_owned(),
                ],
                parent: None,
                tracks: vec![],
                state: LibraryNodeState::Unspecified.into(),
                is_queable: true,
            }),
        };
        Ok(Response::new(reply))
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
        request: Request<QueueTrackRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn queue_library_node(
        &self,
        request: Request<QueueLibraryNodeRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn replace_with_track(
        &self,
        request: Request<ReplaceWithTrackRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn replace_with_node(
        &self,
        request: Request<ReplaceWithNodeRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn append_track(
        &self,
        request: Request<AppendTrackRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn append_node(
        &self,
        request: Request<AppendNodeRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn remove_tracks(
        &self,
        request: Request<RemoveTracksRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn set_current_track(
        &self,
        request: Request<SetCurrentTrackRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn get_queue_updates(
        &self,
        request: Request<GetQueueUpdatesRequest>,
    ) -> Result<Response<Self::GetQueueUpdatesStream>, Status> {
        let reply = GetQueueUpdatesResponse {
            queue_update_result: None,
        };
        // https://giphy.com/gifs/dog-mechanic-i-have-no-idea-what-im-doing-VXCPgZwEP7f1e
        let (tx, rx) = mpsc::channel(128);
        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::GetQueueUpdatesStream
        ))
    }
    async fn get_queue(
        &self,
        request: Request<EmptyRequest>,
    ) -> Result<Response<GetQueueResponse>, Status> {
        let reply = GetQueueResponse { queue: None };
        Ok(Response::new(reply))
    }
    async fn save_queue(
        &self,
        request: Request<SaveQueueRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn toggle_play(
        &self,
        request: Request<EmptyRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn stop(
        &self,
        request: Request<EmptyRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let reply = EmptyResponse {};
        Ok(Response::new(reply))
    }
    async fn get_active_track(
        &self,
        request: Request<EmptyRequest>,
    ) -> Result<Response<GetActiveTrackResponse>, Status> {
        let reply = GetActiveTrackResponse {
            track: None,
            play_state: 0,
            completion: 0,
        };
        Ok(Response::new(reply))
    }
    async fn get_track_updates(
        &self,
        request: Request<GetTrackUpdatesRequest>,
    ) -> Result<Response<Self::GetTrackUpdatesStream>, Status> {
        let reply = GetTrackUpdatesResponse {
            track: None,
            play_state: 0,
            completion: 0,
        };
        // https://giphy.com/gifs/dog-mechanic-i-have-no-idea-what-im-doing-VXCPgZwEP7f1e
        let (tx, rx) = mpsc::channel(128);
        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::GetTrackUpdatesStream
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let crabidy_service = Service::default();

    Server::builder()
        .add_service(CrabidyServiceServer::new(crabidy_service))
        .serve(addr)
        .await?;

    Ok(())
}
