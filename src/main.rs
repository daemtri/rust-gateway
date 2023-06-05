use futures_channel::mpsc::UnboundedSender;
use gate::handle::{TcpStreamHandler, WebsocketStreamHandler};
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

    let addr = "0.0.0.0:8080";
    let lis = TcpListener::bind(addr).await.unwrap();
    log::info!("tcp listen on {}", addr);

    let transmitter = Arc::new(gate::transmit::Transmitter::new());
    loop {
        let (tcp_stream, socket_addr) = lis.accept().await?;
        let transmitter = transmitter.clone();
        tokio::spawn(async move {
            let mut buffer: [u8; 1024] = [0u8; 1024];
            if tcp_stream.peek(&mut buffer).await.is_err() {
                return;
            }
            // Convert the request headers to a string
            let request = String::from_utf8_lossy(&buffer);
            if request.contains(WEBSOCKET_UPGRADE) {
                log::info!("收到新的websocket协议连接请求: {}", socket_addr);
                WebsocketStreamHandler::new(transmitter)
                    .handle_stream(tcp_stream, socket_addr)
                    .await;
            } else {
                log::info!("收到新的tcp协议连接请求: {}", socket_addr);
                TcpStreamHandler::new(transmitter)
                    .handle_stream(tcp_stream, socket_addr)
                    .await;
            }
        });
    }
}
