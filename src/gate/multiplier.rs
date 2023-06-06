use super::handle::{TcpStreamHandler, WebsocketStreamHandler};
use super::transmit::Transmitter;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;

const WEBSOCKET_UPGRADE: &str = "Upgrade: websocket";

pub async fn handle_tcp_stream(
    transmitter: Arc<Transmitter>,
    tcp_stream: TcpStream,
    socket_addr: SocketAddr,
) {
    let mut buffer: [u8; 1024] = [0u8; 1024];
    if tcp_stream.peek(&mut buffer).await.is_err() {
        return;
    }
    // Convert the request headers to a string
    // TODO: 这里第一条消息就要判断超时
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
}
