pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use futures::Stream;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
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
    resp_tx: Arc<Mutex<Option<Sender<Result<EchoResponse, Status>>>>>,
    //resp_rx: Mutex<Receiver<Result<EchoResponse, Status>>>,
}

#[tonic::async_trait]
impl pb::echo_server::Echo for EchoServer {
    async fn unary_echo(&self, _: Request<EchoRequest>) -> EchoResult<EchoResponse> {
        Err(Status::unimplemented("not implemented"))
    }

    type ServerStreamingEchoStream = ResponseStream;

    async fn server_streaming_echo(
        &self,
        req: Request<EchoRequest>,
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

        let cmd_tx2 = self.cmd_tx.clone();

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => cmd_tx2.send(Ok(v)).await.expect("working rx"),
                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                eprintln!("\tclient disconnected: broken pipe");
                                break;
                            }
                        }

                        match cmd_tx2.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("\tstream ended");
        });

        // Create response channel and shove receiving end into self
        let (resp_tx, resp_rx) = mpsc::channel(4);
        let out_stream: ReceiverStream<Result<EchoResponse, Status>> = ReceiverStream::new(resp_rx);

        let mut x = self.resp_tx.lock().expect("Lock");
        *x = Some(resp_tx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::BidirectionalStreamingEchoStream
        ))
    }
}

impl EchoServer {
    async fn send(self, resp: EchoResponse) -> () {
        let tx = self.resp_tx.lock().expect("Lock2");
        match tx.deref() {
            Some(tx) => tx.send(Ok(resp)).await.unwrap(),
            None => panic!("resp_tx is None?!"),
        };
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resp_tx = Arc::new(Mutex::new(None));
    let resp_tx2 = resp_tx.clone();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(4);
    let server = EchoServer { cmd_tx, resp_tx };

    tokio::spawn(async move {
        Server::builder()
            .add_service(pb::echo_server::EchoServer::new(server))
            .serve("[::1]:50051".to_socket_addrs().unwrap().next().unwrap())
            .await
            .unwrap();
    });

    println!("After Server::builder!");

    //server.send(EchoResponse {
    //    message: "Oh this is a random response!".into(),
    //});

    loop {
        // this should go into recv()
        let receive = cmd_rx.recv().await;
        let req = receive.unwrap().unwrap();
        println!("Received {:?}", req);

        // this should go into send()
        let resp = EchoResponse {
            message: "Oh this is a random response!".into(),
        };
        let tx = resp_tx2.lock().expect("Lock2");
        match tx.deref() {
            Some(tx) => tx.send(Ok(resp)).await.unwrap(),
            None => panic!("resp_tx is None?!"),
        };
    }

    Ok(())
}
