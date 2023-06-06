use futures_channel::mpsc::UnboundedSender;
use gate::multiplier;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use tungstenite::protocol::Message;

mod gate;

type _Tx = UnboundedSender<Message>;
type _PeerMap = Arc<Mutex<HashMap<SocketAddr, _Tx>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let addr = "0.0.0.0:8080";
    let lis = TcpListener::bind(addr).await.unwrap();
    log::info!("tcp listen on {}", addr);

    let transmitter = Arc::new(gate::transmit::Transmitter::new());
    loop {
        let (tcp_stream, socket_addr) = lis.accept().await?;
        let transmitter = transmitter.clone();
        tokio::spawn(async move {
            multiplier::handle_tcp_stream(transmitter, tcp_stream, socket_addr).await;
        });
    }
}
