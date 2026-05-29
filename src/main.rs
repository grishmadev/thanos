use std::{error::Error, net::SocketAddr};
use thanos::config::Config;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn read_server(addr: SocketAddr, buf: &mut [u8]) -> Result<usize, Box<dyn Error>> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.write_all(buf).await?;
    let size = stream.read(buf).await?;

    Ok(size)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::get()?;
    let lb_addr = format!("0.0.0.0:{}", config.self_port);
    println!("Load Balancer running on port: {}", config.self_port);
    let sr_addrs = config.servers;
    println!("Serving Addrs: {:?}", sr_addrs);
    let lb_listener = TcpListener::bind(lb_addr).await?;
    let (mut lb_stream, _) = lb_listener.accept().await?;

    let mut idx = 0;
    loop {
        let mut buf = [0u8; 1024];
        let mut size = lb_stream.read(&mut buf).await?;
        if size == 0 {
            continue;
        }

        if let Some(&sr_addr) = sr_addrs.get(idx) {
            println!("addr: {}", sr_addr);
            size = read_server(sr_addr, &mut buf).await?;
            idx = if idx == sr_addrs.len() - 1 {
                0
            } else {
                idx + 1
            };
        }
        lb_stream.write_all(&buf[..size]).await?;
    }
}
