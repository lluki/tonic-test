use futures::Stream;
use std::pin::Pin;
use tonic::{transport::Server, Request, Response, Status};

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

pub mod echo {
    tonic::include_proto!("grpc.examples.echo");
}

use hello_world::{
    greeter_server::{Greeter, GreeterServer},
    HelloReply, HelloRequest,
};

use echo::{
    echo_server::{Echo, EchoServer},
    EchoRequest, EchoResponse,
};

type ResponseStream = Pin<Box<dyn Stream<Item = Result<EchoResponse, Status>> + Send>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();

    let greeter = GreeterServer::new(MyGreeter::default());
    let echo = EchoServer::new(MyEcho::default());

    Server::builder()
        .add_service(greeter)
        .add_service(echo)
        .serve(addr)
        .await?;

    Ok(())
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[derive(Default)]
pub struct MyEcho;

#[tonic::async_trait]
impl Echo for MyEcho {
    async fn unary_echo(
        &self,
        request: Request<EchoRequest>,
    ) -> Result<Response<EchoResponse>, Status> {
        let message = request.into_inner().message;
        Ok(Response::new(EchoResponse { message }))
    }

    type ServerStreamingEchoStream = ResponseStream;

    async fn server_streaming_echo(
        &self,
        _: Request<EchoRequest>,
    ) -> Result<Response<Self::ServerStreamingEchoStream>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn client_streaming_echo(
        &self,
        _: Request<tonic::Streaming<EchoRequest>>,
    ) -> Result<Response<EchoResponse>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    type BidirectionalStreamingEchoStream = ResponseStream;

    async fn bidirectional_streaming_echo(
        &self,
        _: Request<tonic::Streaming<EchoRequest>>,
    ) -> Result<Response<Self::BidirectionalStreamingEchoStream>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }
}
