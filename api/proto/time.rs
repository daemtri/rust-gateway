use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout, Instant};

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // 设置超时
    let auth_timeout = timeout(AUTH_TIMEOUT, async {
        let mut buf = [0u8; 1024];
        // 读取认证消息
        let n = stream.read(&mut buf).await?;
        // 处理认证消息
        // ...

        Ok(())
    });

    match auth_timeout.await {
        Ok(result) => {
            // 认证成功
            result?;
            println!("Authentication successful");

            // 启动心跳计时器
            let mut last_message_time = Instant::now();
            let heartbeat_interval = Duration::from_secs(10);

            loop {
                // 检查心跳超时
                let elapsed = Instant::now().duration_since(last_message_time);
                if elapsed >= heartbeat_interval {
                    // 心跳超时，断开连接
                    println!("Heartbeat timeout");
                    break;
                }

                // 读取消息或心跳
                let mut buf = [0u8; 1024];
                let n = match timeout(heartbeat_interval - elapsed, stream.read(&mut buf)).await {
                    Ok(Ok(n)) => n,
                    _ => {
                        // 超时或读取错误，断开连接
                        println!("Read error");
                        break;
                    }
                };

                if n == 0 {
                    // 客户端关闭连接
                    break;
                }

                last_message_time = Instant::now();

                // 处理消息
                let message = String::from_utf8_lossy(&buf[..n]);
                println!("Received message: {}", message);

                // 发送响应消息
                let response = "Response";
                stream.write_all(response.as_bytes()).await?;
                stream.flush().await?;
            }
        }
        Err(_) => {
            // 超时，关闭连接
            eprintln!("Authentication timeout");
            stream.shutdown().await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                eprintln!("Error handling connection: {}", err);
            }
        });
    }
}
