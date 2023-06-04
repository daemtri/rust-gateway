use anyhow::{Ok, Result};
use api::transmit_client::TransmitClient;
use api::{DispatchReply, DispatchRequest};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::Read;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Channel;

pub mod api {
    tonic::include_proto!("transmit");
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ServiceEntry {
    id: String,
    name: String,
    alias: Option<String>,
    endpoints: Vec<String>,
}

pub struct Transmitter {
    services: HashMap<String, ServiceEntry>,
    clients: RwLock<HashMap<u32, RefCell<TransmitClient<Channel>>>>,
}

#[derive(Debug)]
pub struct MessageHeader {
    pub message_id: u32,
    pub body_length: u32,
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
        }
    }

    async fn get_client(&self, key: u32) -> RefCell<TransmitClient<tonic::transport::Channel>> {
        let lock = self.clients.read().await;
        lock.get(&key).unwrap().clone()
    }

    fn create_transmit_client(
        &self,
        app_id: u32,
    ) -> impl Future<Output = TransmitClient<tonic::transport::Channel>> + Send {
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

        let channel = tonic::transport::Channel::from_shared(address).unwrap();
        async move {
            let channel = channel.connect().await.unwrap();
            TransmitClient::new(channel)
        }
    }

    pub async fn dispatch(&self, header: MessageHeader, body: Vec<u8>) -> Result<()> {
        let app_id = (header.message_id >> 20) & 0xFFF; // 提取 app_id 的前 12 位

        let clients = self.clients.read().await;
        if clients.contains_key(&app_id) {
            let client = self.get_client(app_id).await;
            do_dispatch(client, header, body).await?;
        } else {
            drop(clients);
            let mut clients = self.clients.write().await;
            if !clients.contains_key(&app_id) {
                let client = self.create_transmit_client(app_id).await;
                clients.insert(123, RefCell::new(client));
            }
            drop(clients);
            let clients = self.clients.read().await;
            let client = self.get_client(app_id).await;
            do_dispatch(client, header, body).await?;
        }

        Ok(())
    }
}

async fn do_dispatch(
    client: RefCell<TransmitClient<tonic::transport::Channel>>,
    header: MessageHeader,
    body: Vec<u8>,
) -> Result<()> {
    client.borrow_mut().dispatch(DispatchRequest {
        msgid: header.message_id as i32,
        data: body,
    });
    Ok(())
}
