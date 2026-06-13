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

#[inline(always)]
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
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = &backend_clone.servers[current_idx];
                let sr_socket = TcpSocket::new_v4().unwrap();
                let sock = SockRef::from(&sr_socket);
                _ = sock.set_ip_transparent_v4(true);
                _ = sock.set_reuse_address(true);
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
                    inc_and_check(&failed_clone, current_idx, true, Arc::clone(&backend_clone));
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
        let counter = AtomicUsize::new(0);
        loop {
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, client_addr) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let current_idx = backend.next(&counter);
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = &backend_clone.servers[current_idx];
                let sr_socket = TcpSocket::new_v4().unwrap();
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
                }
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
        let counter = AtomicUsize::new(0);
        loop {
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, _) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            let current_idx = backend.next(&counter);
            let failed_clone = Arc::clone(&failed);
            tokio::spawn(async move {
                let current_server = &backend_clone.servers[current_idx];
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
                let current_server = &backend_clone.servers[current_idx];
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

                // let (sr_read, sr_write) = sr_stream.split();
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
        // Allocate active server for Least Connection
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10));
            loop {
                interval.tick().await;
                let active = backend_clone.active_idxs.load();
                if active.is_empty() {
                    continue;
                }
                let mut best = active[0];
                let mut least = backend_clone.servers[best].ttlcn.load(Ordering::Relaxed);
                for &idx in active.iter().skip(1) {
                    let conns = backend_clone.servers[idx].ttlcn.load(Ordering::Relaxed);
                    if conns < least {
                        least = conns;
                        best = idx;
                    }
                }
                backend_clone.idx.store(best, Ordering::Relaxed);
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
            let lb_listener: TcpListener = lb_sock.listen(4096).unwrap();
            let res = {
                if config.method == Method::Tproxy {
                    run_tproxy_method(lb_listener, backend, config.strategy).await
                } else {
                    run_normal_proxy(lb_listener, backend, config.strategy).await
                }
            };
            if let Err(e) = res {
                plog(&e.to_string(), Log::Err);
            };
        });
    }
    Ok(())
}
