use std::{
    error::Error,
    fmt, io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvError, SendError},
    },
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket, TcpStream},
    sync::mpsc,
};

pub mod config;
pub mod logs;

#[derive(Debug)]
pub struct Server {
    pub addr: SocketAddr,
    pub is_healthy: AtomicBool,
}

impl From<SocketAddr> for Server {
    fn from(server: SocketAddr) -> Self {
        Self {
            addr: server,
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

pub struct Pool {
    sender: mpsc::Sender<TcpStream>,
    receiver: mpsc::Receiver<TcpStream>,
}

impl Pool {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<TcpStream>();
        Self { sender, receiver }
    }

    fn acquire(self) -> Result<TcpStream, RecvError> {
        self.receiver.recv()
    }

    fn release(self, stream: TcpStream) -> Result<(), SendError<TcpStream>> {
        self.sender.send(stream)
    }
}

pub async fn handle_client(
    sr_addr: SocketAddr,
    lb_listener: &mut TcpListener,
) -> Result<(), ThanosError> {
    let (mut lb_stream, _) = lb_listener.accept().await?;
    tokio::spawn(async move {
        let mut sr_stream = match TcpStream::connect(sr_addr).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Cannot connect to {}\n{}", sr_addr, e);
                return;
            }
        };
        if let Err(e) = tokio::io::copy_bidirectional(&mut sr_stream, &mut lb_stream).await {
            eprintln!("Address Redirection Error: {}", e);
        };
    });
    Ok(())
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
