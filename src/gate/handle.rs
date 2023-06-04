use super::transmit::{MessageHeader, Transmitter};
use anyhow::Result;
use futures_util::SinkExt;
use futures_util::{pin_mut, StreamExt};

use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::ReadHalf;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::protocol::Message;

#[derive(Error, Debug)]
enum HandleError {
    #[error("消息长度错误 {0}")]
    MessageSizeError(usize),
}

pub struct WsHandler {
    transmitter: Arc<Transmitter>,
}

impl WsHandler {
    pub fn new(transmitter: Arc<Transmitter>) -> WsHandler {
        WsHandler { transmitter }
    }

    pub async fn handle_stream(&self, tcp_stream: TcpStream, socket_addr: SocketAddr) {
        let ws_stream = accept_async(tcp_stream).await.unwrap();
        self.handle_websocket(ws_stream, socket_addr).await;
    }

    async fn handle_websocket(&self, ws_stream: WebSocketStream<TcpStream>, addr: SocketAddr) {
        // 处理WebSocket连接的代码
        // 例如，可以使用`ws_stream`来发送和接收WebSocket消息
        // 这里只是简单地打印收到的消息并回显给客户端
        let (mut outgoing, incoming) = ws_stream.split();
        pin_mut!(incoming);
        loop {
            if let Some(result) = incoming.next().await {
                match result {
                    Ok(msg) => {
                        if !msg.is_binary() {
                            log::error!("message type is not binary");
                            break;
                        }
                        let data: Vec<u8> = msg.into_data();
                        match read_ws_frame(data).await {
                            Ok((header, body)) => {
                                outgoing
                                    .send(Message::Binary("hello world".as_bytes().to_vec()))
                                    .await
                                    .unwrap();
                                match self.transmitter.dispatch(header, body).await {
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
                    Err(err) => {
                        log::error!("read ws incoming error {}, addr {}", err, addr);
                    }
                }
            }
        }
    }
}

async fn read_ws_frame(data: Vec<u8>) -> Result<(MessageHeader, Vec<u8>)> {
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

pub struct TcpHandler {
    transmitter: Arc<Transmitter>,
}

impl TcpHandler {
    pub fn new(transmitter: Arc<Transmitter>) -> TcpHandler {
        TcpHandler { transmitter }
    }

    pub async fn handle_stream(&self, tcp_stream: TcpStream, socket_addr: SocketAddr) {
        // 处理TCP连接的代码
        // 例如，可以读取和写入TCP数据流
        // 这里只是简单地打印收到的消息并回显给客户端
        let (mut reader, mut writer) = tokio::io::split(tcp_stream);

        loop {
            match read_tcp_frame(&mut reader).await {
                Ok((header, body)) => {
                    if let Err(e) = writer.write_all(b"hello world").await {
                        log::error!("Failed to send TCP message: {}, addr: {}", e, socket_addr);
                        break;
                    }
                    match self.transmitter.dispatch(header, body).await {
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
