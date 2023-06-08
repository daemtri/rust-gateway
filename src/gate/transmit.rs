use anyhow::Result;
use api::business_service_client::BusinessServiceClient;
use api::DispatchRequest;
use futures_channel::mpsc::UnboundedSender;
use futures_util::lock::Mutex;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::Read;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tonic::Request;

pub mod api {
    tonic::include_proto!("transmit");
}

type Tx = UnboundedSender<(MessageHeader, Vec<u8>)>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ServiceEntry {
    id: String,
    name: String,
    alias: Option<String>,
    endpoints: Vec<String>,
}

pub struct Transmitter {
    services: HashMap<String, ServiceEntry>,
    clients: RwLock<HashMap<u32, Channel>>,
    peer_map: PeerMap,
}

#[derive(Debug)]
pub struct MessageHeader {
    pub message_id: u32,
    pub body_length: u32,
}

impl MessageHeader {
    pub fn from_bytes(bytes: &[u8; 8]) -> MessageHeader {
        let message_id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let body_length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        MessageHeader {
            message_id,
            body_length,
        }
    }
}

impl Transmitter {
    pub fn new() -> Self {
        let mut apps_file = File::open("./apps.yaml").expect("read file apps.yaml failed");
        let mut yaml_str = String::new();
        apps_file
            .read_to_string(&mut yaml_str)
            .expect("read file error");
        let apps_config: Vec<ServiceEntry> =
            serde_yaml::from_str(&yaml_str).expect("parse yaml failed");

        log::info!("AppsConfig: {:#?}", apps_config);

        let mut apps_map = HashMap::<String, ServiceEntry>::new();
        for (_, item) in apps_config.iter().enumerate() {
            apps_map.insert(item.name.clone(), item.clone());
        }

        Transmitter {
            services: apps_map,
            clients: RwLock::new(HashMap::new()),
            peer_map: PeerMap::new(Mutex::new(HashMap::new())),
        }
    }

    fn create_transmit_channel(&self, app_id: u32) -> impl Future<Output = Channel> + Send {
        let app_name = format!("app{:03x}", app_id);
        let address = if self.services.contains_key(&app_name) {
            let mut ep = String::new();
            for endpoint in self.services.get(&app_name).unwrap().endpoints.iter() {
                if endpoint.starts_with("grpc://") {
                    ep = endpoint.clone();
                }
            }
            if ep.is_empty() {
                panic!("app {} grpc endpoint is empty", app_name);
            }
            ep.replace("grpc://", "http://")
        } else {
            format!("http://{}:8090", app_name)
        };
        let debug_address = address.clone();
        let channel: tonic::transport::Endpoint = Channel::from_shared(address).unwrap();
        async move {
            channel
                .connect()
                .await
                .expect(format!("连接主机出错: {}", debug_address).as_str())
        }
    }

    pub async fn dispatch(&self, header: MessageHeader, body: Vec<u8>) -> Result<()> {
        log::info!("收到message id: {}", header.message_id);
        let app_id = (header.message_id >> 20) & 0xFFF; // 提取 app_id 的前 12 位

        let clients = self.clients.read().await;
        if clients.contains_key(&app_id) {
            let channel = clients.get(&app_id).unwrap();
            self.do_dispatch(channel, header, body).await?;
        } else {
            drop(clients);
            let mut clients = self.clients.write().await;
            if !clients.contains_key(&app_id) {
                let channel = self.create_transmit_channel(app_id).await;
                clients.insert(app_id, channel);
            }
            drop(clients);
            let clients = self.clients.read().await;
            let channel = clients.get(&app_id).unwrap();
            self.do_dispatch(channel, header, body).await?;
        }

        Ok(())
    }

    async fn do_dispatch(
        &self,
        channel: &Channel,
        header: MessageHeader,
        body: Vec<u8>,
    ) -> Result<()> {
        let mut req = Request::new(DispatchRequest {
            msgid: header.message_id as i32,
            data: body,
        });
        req.metadata_mut()
            .append("user_id", FromStr::from_str("1").unwrap());

        BusinessServiceClient::with_interceptor(channel.clone(), |mut req: tonic::Request<()>| {
            let app_id = "123";
            req.metadata_mut()
                .insert("app_id", FromStr::from_str(app_id).unwrap());
            Ok(req)
        })
        .dispatch(req)
        .await?;
        Ok(())
    }
}

// Authority 认证信息
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Authority {}

// Authority 认证信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthMessage {}

impl AuthMessage {
    pub fn from_bytes(bytes: &[u8]) -> Result<AuthMessage> {
        let auth_message: AuthMessage =
            serde_json::from_slice(bytes).expect("parse auth message json failed");
        Ok(auth_message)
    }
}

pub struct TransmitAgent {
    transmitter: Arc<Transmitter>,
    auth: Option<Authority>,
}

impl TransmitAgent {
    pub fn new(transmitter: Arc<Transmitter>) -> Self {
        TransmitAgent {
            transmitter,
            auth: None,
        }
    }

    pub async fn auth(&mut self, auth_message: AuthMessage) -> Result<()> {
        log::info!("收到登录请求 {:?}", auth_message);
        self.auth = Some(Authority {});
        Ok(())
    }

    pub async fn dispatch(&self, header: MessageHeader, body: Vec<u8>) -> Result<()> {
        if self.auth.is_none() {
            log::info!("未登录")
        }
        // TODO: 处理登录状态
        self.transmitter.dispatch(header, body).await
    }
}
