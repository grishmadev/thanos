use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use socket2::{SockAddr, SockRef};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

use crate::{
    Backend, ThanosError, check_server_health,
    config::{Config, Method, Strategy},
    logs::{Log, plog},
};

pub const THRESHOLD: u32 = 3;

#[inline]
pub fn inc_and_check(val: &Arc<AtomicUsize>, idx: usize, rem: bool, backend: Arc<Backend>) -> bool {
    val.fetch_add(1, Ordering::Relaxed);
    let value = val.load(Ordering::Relaxed);
    let cond = value >= THRESHOLD as usize;
    if cond {
        if rem {
            backend.rem_server(idx);
        } else {
            backend.add_server(idx);
        }
        val.store(0, Ordering::Relaxed);
    };
    cond
}

pub async fn run_tproxy_method(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
    strategy: Strategy,
) -> Result<(), std::io::Error> {
    async fn run_tproxy_lc(
        lb_listener: TcpListener,
        backend: Arc<Backend>,
    ) -> Result<(), std::io::Error> {
        let failed = Arc::new(AtomicUsize::new(0));
        loop {
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, client_addr) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let current_idx = backend.idx.load(Ordering::Relaxed);
            let failed_copy = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = backend_clone.servers.get(current_idx).unwrap();
                let sr_socket = TcpSocket::new_v4().unwrap();
                let sock = SockRef::from(&sr_socket);
                sock.set_ip_transparent_v4(true).unwrap();
                sock.set_reuse_address(true).unwrap();
                let client_addr = SockAddr::from(SocketAddr::new(client_addr.ip(), 0));

                if let Err(e) = sock.bind(&client_addr) {
                    plog(&format!("Bind Error: {}", e), Log::Err);
                    return;
                };

                let mut sr_stream = match sr_socket.connect(current_server.addr).await {
                    Ok(s) => {
                        current_server.ttlcn.fetch_add(1, Ordering::Relaxed);
                        s
                    }
                    Err(e) => {
                        inc_and_check(&failed_copy, current_idx, true, backend_clone);
                        plog(&e.to_string(), Log::Err);
                        return;
                    }
                };
                sr_stream.set_nodelay(true).unwrap();
                lb_stream.set_nodelay(true).unwrap();

                if tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await
                .is_err()
                {
                    inc_and_check(&failed_copy, current_idx, true, Arc::clone(&backend_clone));
                }

                // Decreasing count when client disconnects
                current_server.ttlcn.fetch_sub(1, Ordering::Relaxed);
            });
        }
    }

    async fn run_tproxy_rr(
        lb_listener: TcpListener,
        backend: Arc<Backend>,
    ) -> Result<(), std::io::Error> {
        let failed = Arc::new(AtomicUsize::new(0));
        loop {
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, client_addr) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let current_idx = backend.next();
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = if let Some(s) = backend_clone.servers.get(current_idx) {
                    s
                } else {
                    return;
                };
                let sr_socket = if let Ok(s) = TcpSocket::new_v4() {
                    s
                } else {
                    return;
                };
                let sock = SockRef::from(&sr_socket);
                _ = sock.set_ip_transparent_v4(true);
                _ = sock.set_reuse_address(true);
                let client_addr = SockAddr::from(SocketAddr::new(client_addr.ip(), 0));

                if let Err(e) = sock.bind(&client_addr) {
                    plog(&format!("Bind Error: {}", e), Log::Err);
                    return;
                };

                let mut sr_stream = match sr_socket.connect(current_server.addr).await {
                    Ok(s) => s,
                    Err(e) => {
                        inc_and_check(&failed_clone, current_idx, true, backend_clone);
                        plog(&e.to_string(), Log::Err);
                        return;
                    }
                };
                _ = sr_stream.set_nodelay(true);
                _ = lb_stream.set_nodelay(true);

                if tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await
                .is_err()
                {
                    inc_and_check(&failed_clone, current_idx, true, backend_clone);
                };
            });
        }
    }
    match strategy {
        Strategy::RoundRobin => run_tproxy_rr(lb_listener, backend).await,
        Strategy::LeastConnections => run_tproxy_lc(lb_listener, backend).await,
    }
}

pub async fn run_normal_proxy(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
    strategy: Strategy,
) -> Result<(), std::io::Error> {
    async fn run_normal_rr(
        lb_listener: TcpListener,
        backend: Arc<Backend>,
    ) -> Result<(), std::io::Error> {
        let failed = Arc::new(AtomicUsize::new(0));
        loop {
            let backend_clone = Arc::clone(&backend);
            let current_idx = backend.next();
            let (mut lb_stream, _) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = backend_clone.servers.get(current_idx).unwrap();
                let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                    Ok(s) => s,
                    Err(_) => {
                        inc_and_check(&failed_clone, current_idx, true, backend_clone);
                        return;
                    }
                };
                _ = sr_stream.set_nodelay(true);
                _ = lb_stream.set_nodelay(true);
                if tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await
                .is_err()
                {
                    inc_and_check(&failed_clone, current_idx, true, backend_clone);
                }
            });
        }
    }

    async fn run_normal_lc(
        lb_listener: TcpListener,
        backend: Arc<Backend>,
    ) -> Result<(), std::io::Error> {
        let failed = Arc::new(AtomicUsize::new(0));
        loop {
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, _) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let current_idx = backend_clone.idx.load(Ordering::Relaxed);
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = backend_clone.servers.get(current_idx).unwrap();
                let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                    Ok(s) => {
                        current_server.ttlcn.fetch_add(1, Ordering::Relaxed);
                        s
                    }
                    Err(_) => {
                        inc_and_check(&failed_clone, current_idx, true, backend_clone);
                        return;
                    }
                };
                _ = sr_stream.set_nodelay(true);
                _ = lb_stream.set_nodelay(true);

                if tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await
                .is_err()
                {
                    inc_and_check(&failed_clone, current_idx, true, Arc::clone(&backend_clone));
                }
                current_server.ttlcn.fetch_sub(1, Ordering::Relaxed);
            });
        }
    }
    match strategy {
        Strategy::RoundRobin => run_normal_rr(lb_listener, backend).await,
        Strategy::LeastConnections => run_normal_lc(lb_listener, backend).await,
    }
}

pub async fn run_main(config: Config) -> Result<(), ThanosError> {
    let lb_addr: SocketAddr = format!("0.0.0.0:{}", config.self_port).parse().unwrap();
    let origin_servers = Arc::new(Backend::from(config.servers.clone()));
    check_server_health(Arc::clone(&origin_servers)).await?;
    let backend_clone = Arc::clone(&origin_servers);
    if config.strategy == Strategy::LeastConnections {
        // Check for connections and assign server with least connection every 2 ms
        tokio::spawn(async move {
            let mut curidx = 0;
            loop {
                let mut least_conns = usize::MAX;
                {
                    let idxs = backend_clone.active_idxs.read().unwrap();
                    for idx in idxs.iter() {
                        let current_server = &backend_clone.servers[*idx];
                        let ttlcn = current_server.ttlcn.load(Ordering::Relaxed);
                        if ttlcn <= least_conns {
                            least_conns = ttlcn;
                            backend_clone.idx.store(*idx, Ordering::Relaxed);
                            curidx = 0;
                        }
                        if curidx < idxs.len() {
                            curidx += 1;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        });
    }
    for _ in 0..config.core {
        let backend = Arc::clone(&origin_servers);
        let lb_sock = TcpSocket::new_v4()?;
        let lb_sockref = SockRef::from(&lb_sock);

        lb_sockref.set_reuse_address(true)?;
        lb_sockref.set_reuse_port(true)?;
        lb_sockref.bind(&SockAddr::from(lb_addr))?;

        tokio::spawn(async move {
            let lb_listener: TcpListener = lb_sock.listen(1024).unwrap();
            if config.method == Method::Tproxy {
                if let Err(e) = run_tproxy_method(lb_listener, backend, config.strategy).await {
                    plog(&e.to_string(), Log::Err);
                };
            } else {
                if let Err(e) = run_normal_proxy(lb_listener, backend, config.strategy).await {
                    plog(&e.to_string(), Log::Err);
                }
            }
        });
    }
    Ok(())
}
