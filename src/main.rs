use std::{
    error::Error,
    io::{self, ErrorKind},
    net::SocketAddr,
};
use thanos::{ThanosError, config::Config};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn read_server(addr: SocketAddr, buf: &mut [u8]) -> Result<usize, ThanosError> {
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
    let idx_next = |n: &mut usize| {
        *n = if *n == sr_addrs.len() - 1 { 0 } else { *n + 1 };
    };
    let mut buf = [0u8; 1024];
    let mut reloaded = false;
    let mut attempt = 0;
    loop {
        let mut size: usize = 0;
        if !reloaded {
            size = lb_stream.read(&mut buf).await?;
            attempt = 0;
            if size == 0 {
                continue;
            }
        }
        if attempt >= sr_addrs.len() {
            eprintln!("All Servers Down!");
            break;
        }

        if let Some(&sr_addr) = sr_addrs.get(idx) {
            println!("addr: {}", sr_addr);
            attempt += 1;
            match read_server(sr_addr, &mut buf).await {
                Ok(s) => {
                    size = s;
                    println!("writing to load balancer: {}", size);
                    idx_next(&mut idx);
                    reloaded = false;
                }
                Err(e) => {
                    println!("Error: {}", e);
                    if e == ThanosError::ConnectionRefused {
                        idx_next(&mut idx);
                        reloaded = true;
                        continue;
                    }
                    break;
                }
            };
        };
        lb_stream.write_all(&buf[..size]).await?;
    }
    Ok(())
}
