use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("Gateway server listening on {}", addr);

    // 创建共享的 TransmitClient HashMap
    let transmit_clients = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, _) = listener.accept().await?;

        let shared_clients = Arc::clone(&transmit_clients);
        tokio::spawn(async move {
            handle_client(stream, shared_clients).await;
        });
    }
}

async fn handle_client(
    mut stream: tokio::net::TcpStream,
    clients: Arc<Mutex<HashMap<u32, Arc<TransmitClient<tonic::transport::Channel>>>>>,
) {
    loop {
        // 读取头部数据
        let mut header_buffer = [0u8; 8];
        match stream.read_exact(&mut header_buffer).await {
            Ok(_) => {
                // 解析头部数据
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

                // 读取 body 数据
                let mut body_buffer = vec![0u8; body_length as usize];
                match stream.read_exact(&mut body_buffer).await {
                    Ok(_) => {
                        // 获取或创建对应的 TransmitClient
                        let transmit_client = {
                            let clients = clients.lock().unwrap();
                            clients
                                .entry(get_app_id(message_id))
                                .or_insert_with(|| create_transmit_client(get_app_id(message_id)))
                                .clone()
                        };

                        // 调用 gRPC 方法
                        match transmit_client.dispatch(Request::new(request)).await {
                            // 处理 gRPC 响应...
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to read body from client: {}", err);
                        break; // 发生错误，退出循环
                    }
                }
            }
            Err(err) => {
                eprintln!("Failed to read header from client: {}", err);
                break; // 发生错误，退出循环
            }
        }
    }
}

fn get_app_id(message_id: u32) -> u32 {
    message_id >> 20 // 取前12位作为 app_id
}

fn create_transmit_client(app_id: u32) -> Arc<TransmitClient<tonic::transport::Channel>> {
    // 根据 app_id 生成地址...
    // 创建 gRPC 客户端...
    // 返回 TransmitClient 实例...
    // 获取 message_id
    let message_id = get_message_id_from_stream(&stream);

    // 根据 message_id 生成地址
    let app_id = message_id >> 20; // 取前12位作为 app_id
    let app_id_hex = format!("{:03X}", app_id); // 转换为16进制字符串
    let address = format!("http://localhost:50051/{}", app_id_hex); // 添加前缀

    // 创建 gRPC 客户端
    let channel = tonic::transport::Channel::from_static(&address);
    let client = TransmitClient::new(channel);

    Arc::new(client)
}
