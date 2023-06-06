use futures_channel::mpsc::UnboundedSender;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::signal;
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

    tokio::select! {
        _ = gate::tcp_serve(lis,transmitter) => {
            Ok(())
        }
        _ = signal::ctrl_c() => {
            log::info!("应用退出");
            Ok(())
        }
    }
}
