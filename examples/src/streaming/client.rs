pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use self::pb::EchoResponse;
use pb::{echo_client::EchoClient, EchoRequest};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

struct MyClient {
    cmd_tx: tokio::sync::mpsc::Sender<EchoRequest>,
    resp_rx: tonic::Streaming<pb::EchoResponse>,
}

impl MyClient {
    async fn connect() -> Result<MyClient, Box<dyn std::error::Error>> {
        let mut client = EchoClient::connect("http://[::1]:50051").await.unwrap();

        let (tx, rx) = mpsc::channel(4);
        let resp_rx = client
            .bidirectional_streaming_echo(ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();

        Ok(MyClient {
            cmd_tx: tx,
            resp_rx,
        })
    }

    async fn send(&self, cmd: EchoRequest) -> Result<(), Box<dyn std::error::Error>> {
        Ok(self.cmd_tx.send(cmd).await?)
    }

    async fn recv(&mut self) -> Result<EchoResponse, Box<dyn std::error::Error>> {
        if let Some(r) = self.resp_rx.next().await {
            Ok(r.unwrap())
        } else {
            panic!("Server closed stream?");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = MyClient::connect().await?;

    client
        .send(EchoRequest {
            message: "msg1: hiho".into(),
        })
        .await?;

    client
        .send(EchoRequest {
            message: "msg2: ooo".into(),
        })
        .await?;

    loop {
        let x = client.recv().await?;
        println!("Received {}", x.message);
    }

    Ok(())
}
