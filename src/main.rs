use std::error::Error;
use thanos::config::Config;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// Address assigned to the Load Balancer
const LB_ADDR: &str = "0.0.0.0:9999";

/// Address assigned to the Server (can be multiple)
const SR_ADDR: &str = "127.0.0.1:8888";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let lb_addr: SockAddr = LB_ADDR.parse::<SocketAddr>()?.into();
    // let sr_addr: SockAddr = SR_ADDR.parse::<SocketAddr>()?.into();
    let config = Config::get();
    println!("Config: {:#?}", config);
    let lb_listener = TcpListener::bind(LB_ADDR).await?;
    let (mut lb_stream, _) = lb_listener.accept().await?;
    loop {
        let mut buf = [0u8; 1024];
        let size = lb_stream.read(&mut buf).await?;
        if size == 0 {
            continue;
        }
        let mut sr_stream = TcpStream::connect(SR_ADDR).await?;
        let raw = String::from_utf8_lossy(&buf[..size]);
        let content = raw.split('\n');
        for line in content {
            println!("{}", line);
        }
        sr_stream.write_all(&buf[..size]).await?;

        // Reading from the Server
        let size = sr_stream.read(&mut buf).await?;
        let raw = String::from_utf8_lossy(&buf[..size]);
        let content = raw.split('\n');
        for line in content {
            println!("{}", line);
        }
        lb_stream.write_all(&buf[..size]).await?;
    }
}
