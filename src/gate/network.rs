use super::transmit::{AuthMessage, MessageHeader, TransmitAgent, Transmitter};
use anyhow::Result;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::time::timeout;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::protocol::Message;

const AUTH_MESSAGE_ID: u32 = 0;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);
const WEBSOCKET_UPGRADE: &str = "Upgrade: websocket";

#[derive(Error, Debug)]
enum NetworkError {
    #[error("消息长度错误 {0}")]
    MessageSizeError(usize),
    #[error("消息类型不是二进制")]
    MessageTypeNotBinary,
    // #[error("客户端通道已关闭")]
    // ClientClosed,
    #[error("认证消息ID错误")]
    AuthMessageIdError,
}

pub enum MessageReceiver {
    Tcp(ReadHalf<TcpStream>),
    WebSocket(SplitStream<WebSocketStream<TcpStream>>),
}

impl MessageReceiver {
    pub async fn next_message(&mut self) -> Result<(MessageHeader, Vec<u8>)> {
        match self {
            Self::Tcp(reader) => {
                let mut header = [0u8; 8];
                reader.read_exact(&mut header).await?;
                let header = MessageHeader::from_bytes(&header);
                if header.body_length > 0 {
                    let mut body = vec![0u8; header.body_length as usize];
                    reader.read_exact(&mut body).await?;
                    Ok((header, body))
                } else {
                    Ok((header, vec![]))
                }
            }
            Self::WebSocket(reader) => {
                let msg = reader.next().await.unwrap()?;
                if !msg.is_binary() {
                    return Err(NetworkError::MessageTypeNotBinary.into());
                }
                let data: Vec<u8> = msg.into_data();
                if data.len() < 8 {
                    return Err(NetworkError::MessageSizeError(data.len()).into());
                }

                let message_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let body_length = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

                if body_length > 0 {
                    let mut body_buffer = vec![0u8; body_length as usize];
                    body_buffer.copy_from_slice(&data[8..]);
                    let message_header = MessageHeader {
                        message_id,
                        body_length,
                    };
                    Ok((message_header, body_buffer))
                } else {
                    let message_header = MessageHeader {
                        message_id,
                        body_length,
                    };
                    Ok((message_header, vec![]))
                }
            }
        }
    }
}

pub enum MessageSender {
    Tcp(WriteHalf<TcpStream>),
    WebSocket(SplitSink<WebSocketStream<TcpStream>, Message>),
}

impl MessageSender {
    pub async fn send_message(&mut self, header: MessageHeader, body: Vec<u8>) -> Result<()> {
        match self {
            Self::Tcp(writer) => {
                let mut header_buffer = [0u8; 8];
                header_buffer[..4].copy_from_slice(&header.message_id.to_be_bytes());
                header_buffer[4..].copy_from_slice(&header.body_length.to_be_bytes());
                writer.write_all(&header_buffer).await?;
                if header.body_length > 0 {
                    writer.write_all(&body).await?;
                }
                writer.flush().await?;
                Ok(())
            }
            Self::WebSocket(writer) => {
                let mut header_buffer = [0u8; 8];
                header_buffer[..4].copy_from_slice(&header.message_id.to_be_bytes());
                header_buffer[4..].copy_from_slice(&header.body_length.to_be_bytes());
                let mut buffer = Vec::with_capacity(8 + body.len());
                buffer.extend_from_slice(&header_buffer);
                buffer.extend_from_slice(&body);
                writer.send(Message::Binary(buffer)).await?;
                Ok(())
            }
        }
    }
}

/// 单个客户端连接, 用于处理认证和心跳, 以及转发消息
pub struct MultipleServer {
    agent: TransmitAgent,
    socket_addr: SocketAddr,
    receiver: MessageReceiver,
    sender: MessageSender,
}

impl MultipleServer {
    pub fn new(
        transmitter: Arc<Transmitter>,
        socket_addr: SocketAddr,
        receiver: MessageReceiver,
        sender: MessageSender,
    ) -> Self {
        Self {
            agent: TransmitAgent::new(transmitter),
            socket_addr,
            receiver,
            sender,
        }
    }
    pub async fn from_tcp_stream(
        transmitter: Arc<Transmitter>,
        socket_addr: SocketAddr,
        tcp_stream: TcpStream,
    ) -> Result<Self> {
        let mut buffer: [u8; 1024] = [0u8; 1024];
        tcp_stream.peek(&mut buffer).await?;
        // Convert the request headers to a string
        // TODO: 这里第一条消息就要判断超时
        let request = String::from_utf8_lossy(&buffer);
        if request.contains(WEBSOCKET_UPGRADE) {
            let ws_stream = accept_async(tcp_stream).await?;
            let (writer, reader) = ws_stream.split();
            let receiver = MessageReceiver::WebSocket(reader);
            let sender = MessageSender::WebSocket(writer);
            Ok(Self::new(transmitter, socket_addr, receiver, sender))
        } else {
            let (reader, writer) = tokio::io::split(tcp_stream);
            let receiver = MessageReceiver::Tcp(reader);
            let sender = MessageSender::Tcp(writer);
            Ok(Self::new(transmitter, socket_addr, receiver, sender))
        }
    }

    pub async fn auth(&mut self) -> Result<()> {
        let auth_timer = timeout(AUTH_TIMEOUT, self.receiver.next_message()).await?;
        let (header, body) = auth_timer?;
        if header.message_id != AUTH_MESSAGE_ID {
            return Err(NetworkError::AuthMessageIdError.into());
        }
        log::debug!("认证成功1");
        let auth_message = AuthMessage::from_bytes(&body)?;
        self.agent.auth(auth_message).await?;
        self.sender.send_message(header, vec![]).await?;

        log::debug!("认证成功");
        Ok(())
    }

    pub async fn serve(&mut self) -> Result<()> {
        loop {
            let message_timer = timeout(HEARTBEAT_TIMEOUT, self.receiver.next_message()).await?;
            let (header, body) = message_timer?;
            if let Err(err) = self.agent.dispatch(header, body).await {
                log::error!("dispatch message: {}, addr: {}", err, self.socket_addr);
            }
        }
    }
}

/// 处理 TCP 连接
/// 1. 读取第一条消息，判断是否是 websocket 协议
/// 2. 如果是 websocket 协议，升级协议
/// 3. 读取第一条消息，判断是否是 auth 消息
/// 4. 如果是 auth 消息，验证 auth 消息
/// 5. 如果验证成功，返回成功消息
/// 6. 如果验证失败，返回失败消息
/// 7. 如果不是 auth 消息，返回失败消息
/// 8. 读取消息，分发消息
pub async fn handle_tcp_stream(
    transmitter: Arc<Transmitter>,
    socket_addr: SocketAddr,
    tcp_stream: TcpStream,
) -> Result<()> {
    timeout(AUTH_TIMEOUT, async move {
        let mut server = MultipleServer::from_tcp_stream(transmitter, socket_addr, tcp_stream)
            .await
            .unwrap();
        server.auth().await.unwrap();
        server
    })
    .await?
    .serve()
    .await
}
