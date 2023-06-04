use futures_channel::mpsc::UnboundedSender;
use gate::handle::{TcpHandler, WsHandler};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use tungstenite::protocol::Message;

mod gate;

type _Tx = UnboundedSender<Message>;
type _PeerMap = Arc<Mutex<HashMap<SocketAddr, _Tx>>>;

const WEBSOCKET_UPGRADE: &str = "Upgrade: websocket";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let lis = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    log::info!("tcp listen on {}", "0.0.0.0:8080");

    let transmitter = Arc::new(gate::transmit::Transmitter::new());
    let ws_handler = Arc::new(WsHandler::new(transmitter.clone()));
    let tcp_handler = Arc::new(TcpHandler::new(transmitter.clone()));

    loop {
        let (tcp_stream, socket_addr) = lis.accept().await?;
        let wh = ws_handler.clone();
        let th = tcp_handler.clone();
        tokio::spawn(async move {
            let mut buffer: [u8; 1024] = [0u8; 1024];
            if tcp_stream.peek(&mut buffer).await.is_err() {
                return;
            }
            // Convert the request headers to a string
            let request = String::from_utf8_lossy(&buffer);
            if request.contains(WEBSOCKET_UPGRADE) {
                wh.handle_stream(tcp_stream, socket_addr).await;
            } else {
                th.handle_stream(tcp_stream, socket_addr).await;
            }
        });
    }
}
