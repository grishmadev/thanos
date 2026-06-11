use std::{
    error::Error,
    fmt::{self, Debug},
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub mod config;
pub mod logs;
pub mod proxy;

pub const DEFAULT_PORT: u16 = 8080;

#[derive(Debug)]
pub struct Server {
    pub addr: SocketAddr,
    pub is_healthy: AtomicBool,
    // total connections, idk how to write it in short
    pub ttlcn: AtomicUsize,
}

impl From<SocketAddr> for Server {
    fn from(server: SocketAddr) -> Self {
        Self {
            addr: server,
            ttlcn: AtomicUsize::new(0),
            is_healthy: AtomicBool::new(true),
        }
    }
}

#[derive(Debug)]
pub struct Backend {
    pub servers: Vec<Server>,
    pub idx: AtomicUsize,
}

impl From<Vec<SocketAddr>> for Backend {
    fn from(servers: Vec<SocketAddr>) -> Self {
        let mut res = Vec::new();
        for addr in servers {
            res.push(Server {
                is_healthy: AtomicBool::new(true),
                ttlcn: AtomicUsize::new(0),
                addr,
            });
        }
        Self {
            servers: res,
            idx: AtomicUsize::new(0),
        }
    }
}

impl From<Vec<Server>> for Backend {
    fn from(servers: Vec<Server>) -> Self {
        Self {
            servers,
            idx: AtomicUsize::new(0),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ThanosError {
    ConnectionRefused,
    Other(String),
}

impl fmt::Display for ThanosError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#?}", self)
    }
}

impl From<Box<dyn Error>> for ThanosError {
    fn from(e: Box<dyn Error>) -> Self {
        ThanosError::Other(e.to_string())
    }
}
impl From<io::Error> for ThanosError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::ConnectionRefused => ThanosError::ConnectionRefused,
            s => ThanosError::Other(s.to_string()),
        }
    }
}

pub async fn check_server_health(backend: Arc<Backend>) -> Result<(), ThanosError> {
    for idx in 0..backend.servers.len() {
        let backend_clone = Arc::clone(&backend);
        tokio::spawn(async move {
            let current_server = &backend_clone.servers[idx];
            let host = "localhost:8080";
            let request = format!(
                "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                host
            );
            let addr = current_server.addr;
            let threshold = 5;
            let mut attempts = 0usize;
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let is_healthy = &current_server.is_healthy;
                if attempts == threshold {
                    println!("Declaring {} as unhealthy", addr);
                    is_healthy.store(false, Ordering::Relaxed);
                }
                let mut buf = [0u8; 1024];
                let mut sr_stream = match TcpStream::connect(addr).await {
                    Ok(s) => s,
                    Err(e) => {
                        attempts += 1;
                        eprintln!("[{}] {}", addr, e);
                        continue;
                    }
                };
                sr_stream.write_all(request.as_bytes()).await.unwrap();
                match sr_stream.read(&mut buf).await {
                    Ok(s) => {
                        if s == 0 {
                            eprintln!("Server not Responding.");
                            attempts += 1;
                            continue;
                        }
                        is_healthy.store(true, Ordering::Relaxed);
                        attempts = 0;
                    }
                    Err(e) => {
                        is_healthy.store(false, Ordering::Relaxed);
                        attempts += 1;
                        eprintln!("[{}] {}", addr, e);
                    }
                };
            }
        });
    }
    Ok(())
}
