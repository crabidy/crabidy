use std::vec;

use crabidy_core::proto::crabidy::{
    library_service_server::{LibraryService, LibraryServiceServer},
    GetLibraryNodeRequest, GetLibraryNodeResponse, GetTrackRequest, GetTrackResponse, LibraryNode,
    LibraryNodeState,
};
use tonic::{transport::Server, Request, Response, Status};

#[derive(Debug, Default)]
pub struct Library {}

#[tonic::async_trait]
impl LibraryService for Library {
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let crabidy_service = Library::default();

    Server::builder()
        .add_service(LibraryServiceServer::new(crabidy_service))
        .serve(addr)
        .await?;

    Ok(())
}
