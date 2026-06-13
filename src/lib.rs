use std::{
    error::Error,
    fmt::{self, Debug},
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::logs::{Log, plog};

pub mod config;
pub mod logs;
pub mod proxy;

pub const DEFAULT_PORT: u16 = 8080;

#[derive(Debug)]
pub struct Server {
    pub addr: SocketAddr,
    // total connections, idk how to write it in short
    pub ttlcn: AtomicUsize,
}

impl From<SocketAddr> for Server {
    fn from(server: SocketAddr) -> Self {
        Self {
            addr: server,
            ttlcn: AtomicUsize::new(0),
        }
    }
}

#[derive(Debug)]
pub struct Backend {
    pub servers: Vec<Server>,
    pub active_idxs: ArcSwap<Vec<usize>>,
    pub idx: AtomicUsize,
}

impl Backend {
    pub fn add_server(&self, idx: usize) {
        let mut list = self.active_idxs.load().to_vec();
        if !list.contains(&idx) {
            list.push(idx);
            self.active_idxs.store(Arc::new(list));
        }
    }

    pub fn rem_server(&self, idx: usize) {
        let list = self
            .active_idxs
            .load()
            .iter()
            .filter(|f| **f != idx)
            .map(|f| f.to_owned())
            .collect::<Vec<usize>>();
        self.active_idxs.store(Arc::new(list));
    }
    pub fn select_least_conn_server(&self) -> Option<usize> {
        let active = self.active_idxs.load(); // ArcSwap<Vec<usize>>
        if active.is_empty() {
            return None;
        }
        let mut best = active[0];
        let mut least = self.servers[best].ttlcn.load(Ordering::Relaxed);
        for &idx in active.iter().skip(1) {
            let conns = self.servers[idx].ttlcn.load(Ordering::Relaxed);
            if conns < least {
                least = conns;
                best = idx;
            }
        }
        Some(best)
    }
    #[inline]
    pub fn next(&self, idx: &AtomicUsize) -> usize {
        let list;
        {
            list = self.active_idxs.load();
        }
        let idx_idx = idx.fetch_add(1, Ordering::Relaxed) % list.len();
        list[idx_idx]
    }

    #[inline]
    pub fn contains(&self, idx: &usize) -> bool {
        self.active_idxs.load().contains(idx)
    }
}

impl From<Vec<SocketAddr>> for Backend {
    fn from(servers: Vec<SocketAddr>) -> Self {
        let mut res = Vec::new();
        let active_idxs = (0..servers.len()).collect::<Vec<usize>>();
        for addr in servers {
            res.push(Server {
                ttlcn: AtomicUsize::new(0),
                addr,
            });
        }
        Self {
            servers: res,
            active_idxs: ArcSwap::new(Arc::new(active_idxs)),
            idx: AtomicUsize::new(0),
        }
    }
}

impl From<Vec<Server>> for Backend {
    fn from(servers: Vec<Server>) -> Self {
        Self {
            active_idxs: ArcSwap::new(Arc::new((0..servers.len()).collect::<Vec<usize>>())),
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
            let mut failed = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if failed == threshold {
                    plog(&format!("Declaring {} as Unhealthy", addr), Log::Info);
                    backend_clone.rem_server(idx);
                }
                let mut buf = [0u8; 1024];
                let mut sr_stream = match TcpStream::connect(addr).await {
                    Ok(s) => {
                        if failed != 0 {
                            plog(&format!("Declaring {} as Healthy", addr), Log::Info);
                            // failed = 0;
                            if failed >= threshold {
                                failed = threshold - 1;
                            } else {
                                failed -= 1;
                            }
                        } else {
                            backend_clone.add_server(idx);
                        }
                        s
                    }
                    Err(e) => {
                        failed += 1;
                        if backend_clone.contains(&idx) {
                            plog(&format!("{addr} {e}"), Log::Err);
                        }
                        continue;
                    }
                };
                sr_stream.write_all(request.as_bytes()).await.unwrap();
                match sr_stream.read(&mut buf).await {
                    Ok(0) => {
                        plog(&format!("Server {} not Responding.", addr), Log::Err);
                        failed += 1;
                        continue;
                    }
                    Ok(_) => {
                        // failed = 0;
                        if failed >= threshold {
                            failed = threshold - 1;
                        } else if failed != 0 {
                            failed -= 1;
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        if backend_clone.contains(&idx) {
                            plog(&format!("{addr} {e}"), Log::Err);
                        }
                    }
                };
            }
        });
    }
    Ok(())
}
