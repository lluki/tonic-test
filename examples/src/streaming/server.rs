pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use futures::Stream;
use std::sync::Mutex;
use std::{error::Error, io::ErrorKind, net::ToSocketAddrs, pin::Pin};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use pb::{EchoRequest, EchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<EchoResponse, Status>> + Send>>;

fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

#[derive(Debug)]
pub struct EchoServer {
    cmd_tx: Sender<Result<EchoRequest, Status>>,
    // While the server is initializing, this holds the Response receiver.
    // After initialization, it will hold None
    resp_rx: Mutex<Option<Receiver<Result<EchoResponse, Status>>>>,
}

#[tonic::async_trait]
impl pb::echo_server::Echo for EchoServer {
    async fn unary_echo(&self, _: Request<EchoRequest>) -> EchoResult<EchoResponse> {
        Err(Status::unimplemented("not implemented"))
    }

    type ServerStreamingEchoStream = ResponseStream;

    async fn server_streaming_echo(
        &self,
        _req: Request<EchoRequest>,
    ) -> EchoResult<Self::ServerStreamingEchoStream> {
        Err(Status::unimplemented("not implemented"))
    }

    async fn client_streaming_echo(
        &self,
        _: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<EchoResponse> {
        Err(Status::unimplemented("not implemented"))
    }

    type BidirectionalStreamingEchoStream = ResponseStream;

    async fn bidirectional_streaming_echo(
        &self,
        req: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<Self::BidirectionalStreamingEchoStream> {
        println!("EchoServer::bidirectional_streaming_echo");

        let mut in_stream = req.into_inner();
        let cmd_tx = self.cmd_tx.clone();

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => cmd_tx.send(Ok(v)).await.expect("working rx"),
                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                eprintln!("\tclient disconnected: broken pipe");
                                break;
                            }
                        }

                        match cmd_tx.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("\tstream ended");
        });

        // `take` whats inside self.resp_rx
        let mut locked = self.resp_rx.lock();
        let opt: &mut Option<_> = locked.as_deref_mut().unwrap();
        let resp_rx = std::mem::take(opt).unwrap();
        let out_stream: ReceiverStream<Result<EchoResponse, Status>> = ReceiverStream::new(resp_rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::BidirectionalStreamingEchoStream
        ))
    }
}

pub struct EchoServerClient {
    cmd_rx: Receiver<Result<EchoRequest, Status>>,
    resp_tx: Sender<Result<EchoResponse, Status>>,
}

impl EchoServerClient {
    async fn send(&self, resp: EchoResponse) -> () {
        self.resp_tx.send(Ok(resp)).await.unwrap()
    }

    async fn recv(&mut self) -> Result<EchoRequest, Status> {
        self.cmd_rx.recv().await.unwrap()
    }
}

async fn connect() -> Result<EchoServerClient, Box<dyn std::error::Error>> {
    let (resp_tx, resp_rx) = mpsc::channel(4);
    let (cmd_tx, cmd_rx) = mpsc::channel(4);
    let server = EchoServer {
        cmd_tx,
        resp_rx: Mutex::new(Some(resp_rx)),
    };

    tokio::spawn(async move {
        Server::builder()
            .add_service(pb::echo_server::EchoServer::new(server))
            .serve("[::1]:50051".to_socket_addrs().unwrap().next().unwrap())
            .await
            .unwrap();
    });

    println!("After Server::builder!");

    Ok(EchoServerClient { cmd_rx, resp_tx })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = connect().await?;

    loop {
        let req = client.recv().await?;
        println!("Received {:?}", req);
        client
            .send(EchoResponse {
                message: format!("~~~###!!! {:?} !!!###~~~", req.message),
            })
            .await;
    }

    Ok(())
}
