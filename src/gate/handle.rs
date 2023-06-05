use super::transmit::{MessageHeader, TransmitAgent, Transmitter};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::ReadHalf;
use tokio::time::timeout;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::protocol::Message;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Error, Debug)]
enum HandleError {
    #[error("消息长度错误 {0}")]
    MessageSizeError(usize),
    #[error("消息类型不是二进制")]
    MessageTypeNotBinary,
}

pub struct WebsocketStreamHandler {
    agent: TransmitAgent,
}

impl WebsocketStreamHandler {
    pub fn new(transmitter: Arc<Transmitter>) -> WebsocketStreamHandler {
        WebsocketStreamHandler {
            agent: TransmitAgent::new(transmitter),
        }
    }

    pub async fn handle_stream(&self, tcp_stream: TcpStream, socket_addr: SocketAddr) {
        let ws_stream = accept_async(tcp_stream).await.unwrap();
        self.handle_websocket(ws_stream, socket_addr).await;
    }

    async fn handle_websocket(&self, ws_stream: WebSocketStream<TcpStream>, addr: SocketAddr) {
        let (mut outgoing, mut incoming) = ws_stream.split();
        // pin_mut!(incoming);

        // 设置超时
        let auth_timer = timeout(AUTH_TIMEOUT, async {
            let msg = incoming.next().await.unwrap().unwrap();
            let (header, body) = read_ws_frame(msg).await.unwrap();
            self.agent.auth(header, body).await.unwrap();
            anyhow::Ok(())
        });

        if !auth_timer.await.is_ok() {
            log::error!("Authentication timeout");
            let _ = outgoing.close().await;
            return;
        }

        loop {
            let msg_timer = timeout(HEARTBEAT_INTERVAL, incoming.next());
            match msg_timer.await {
                Ok(Some(Ok(msg))) => match read_ws_frame(msg).await {
                    Ok((header, body)) => {
                        outgoing
                            .send(Message::Binary("hello world".as_bytes().to_vec()))
                            .await
                            .unwrap();
                        match self.agent.dispatch(header, body).await {
                            Err(err) => {
                                log::error!("dispatch message: {}", err);
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        log::error!("Error receiving TCP message: {}", e);
                        break;
                    }
                },
                Ok(Some(Err(err))) => {
                    log::error!("read ws incoming error {}, addr {}", err, addr);
                }
                // 这里有问题
                Ok(None) => {
                    continue;
                }
                Err(_) => {
                    // 超时
                }
            }
        }
    }
}

async fn read_ws_frame(msg: Message) -> Result<(MessageHeader, Vec<u8>)> {
    if !msg.is_binary() {
        return Err(HandleError::MessageTypeNotBinary.into());
    }
    let data: Vec<u8> = msg.into_data();
    if data.len() < 8 {
        return Err(HandleError::MessageSizeError(data.len()).into());
    }

    let message_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let body_length = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

    // 读取消息体
    let mut body_buffer = vec![0u8; body_length as usize];
    body_buffer.copy_from_slice(&data[9..]);

    let message_header = MessageHeader {
        message_id,
        body_length,
    };

    Ok((message_header, body_buffer))
}

pub struct TcpStreamHandler {
    agent: TransmitAgent,
}

impl TcpStreamHandler {
    pub fn new(transmitter: Arc<Transmitter>) -> TcpStreamHandler {
        TcpStreamHandler {
            agent: TransmitAgent::new(transmitter),
        }
    }

    pub async fn handle_stream(&self, tcp_stream: TcpStream, socket_addr: SocketAddr) {
        let (mut reader, mut writer) = tokio::io::split(tcp_stream);

        // 设置超时
        let auth_result = timeout(AUTH_TIMEOUT, async {
            let (header, body) = read_tcp_frame(&mut reader).await.unwrap();
            self.agent.auth(header, body).await.unwrap();
            anyhow::Ok(())
        })
        .await;

        if auth_result.is_err() {
            log::error!("Authentication timeout");
            let _ = writer.shutdown().await;
            return;
        }

        loop {
            match read_tcp_frame(&mut reader).await {
                Ok((header, body)) => {
                    if let Err(e) = writer.write_all(b"hello world").await {
                        log::error!("Failed to send TCP message: {}, addr: {}", e, socket_addr);
                        break;
                    }
                    match self.agent.dispatch(header, body).await {
                        Err(err) => {
                            log::error!("dispatch message: {}", err);
                            break;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    log::error!("Error receiving TCP message: {}", e);
                    break;
                }
            }
        }
    }
}

async fn read_tcp_frame(tcp_stream: &mut ReadHalf<TcpStream>) -> Result<(MessageHeader, Vec<u8>)> {
    // 读取消息头
    let mut header_buffer = [0u8; 8]; // 假设消息头的长度为 8 字节
    tcp_stream.read_exact(&mut header_buffer).await?;

    let message_id = u32::from_be_bytes([
        header_buffer[0],
        header_buffer[1],
        header_buffer[2],
        header_buffer[3],
    ]);
    let body_length = u32::from_be_bytes([
        header_buffer[4],
        header_buffer[5],
        header_buffer[6],
        header_buffer[7],
    ]);

    // 读取消息体
    let mut body_buffer = vec![0u8; body_length as usize];
    tcp_stream.read_exact(&mut body_buffer).await?;

    let message_header = MessageHeader {
        message_id,
        body_length,
    };

    Ok((message_header, body_buffer))
}
