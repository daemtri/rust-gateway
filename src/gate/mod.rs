use std::sync::Arc;
use tokio::net::TcpListener;

pub mod handle;
pub mod multiplier;
pub mod network;
pub mod transmit;

pub async fn tcp_serve(
    lis: TcpListener,
    transmitter: Arc<transmit::Transmitter>,
) -> anyhow::Result<()> {
    loop {
        let (tcp_stream, socket_addr) = lis.accept().await.unwrap();
        let transmitter = transmitter.clone();
        tokio::spawn(async move {
            multiplier::handle_tcp_stream(transmitter, tcp_stream, socket_addr).await;
        });
    }
}
