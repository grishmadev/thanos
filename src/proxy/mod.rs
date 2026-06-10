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

pub async fn run_tproxy_method(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
    strategy: Strategy,
) -> Result<(), std::io::Error> {
    match strategy {
        Strategy::RoundRobin => run_round_robin(lb_listener, backend).await,
        Strategy::LeastConnections => run_least_connections(lb_listener, backend).await,
    }
}

async fn run_least_connections(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
) -> Result<(), std::io::Error> {
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
                    plog(&e.to_string(), Log::Err);
                    return;
                }
            };
            sr_stream.set_nodelay(true).unwrap();
            lb_stream.set_nodelay(true).unwrap();

            tokio::io::copy_bidirectional_with_sizes(&mut sr_stream, &mut lb_stream, 4096, 4096)
                .await
                .unwrap();

            // Decreasing count when client disconnects
            current_server.ttlcn.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

async fn run_round_robin(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
) -> Result<(), std::io::Error> {
    loop {
        let backend_clone = Arc::clone(&backend);
        let (mut lb_stream, client_addr) = match lb_listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                plog(&e.to_string(), Log::Err);
                continue;
            }
        };
        let current_idx = backend.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();
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
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    return;
                }
            };
            sr_stream.set_nodelay(true).unwrap();
            lb_stream.set_nodelay(true).unwrap();

            tokio::io::copy_bidirectional_with_sizes(&mut sr_stream, &mut lb_stream, 4096, 4096)
                .await
                .unwrap();
        });
    }
}

pub async fn run_normal_proxy(
    lb_listener: TcpListener,
    backend: Arc<Backend>,
) -> Result<(), std::io::Error> {
    loop {
        let backend_clone = Arc::clone(&backend);
        let (mut lb_stream, _) = match lb_listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                plog(&e.to_string(), Log::Err);
                continue;
            }
        };
        let current_idx =
            backend_clone.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();
        tokio::spawn(async move {
            let current_server = backend_clone.servers.get(current_idx).unwrap();
            let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                Ok(s) => {
                    current_server.ttlcn.fetch_add(1, Ordering::Relaxed);
                    s
                }
                Err(_) => return,
            };
            _ = sr_stream.set_nodelay(true);
            _ = lb_stream.set_nodelay(true);
            _ = tokio::io::copy_bidirectional_with_sizes(
                &mut sr_stream,
                &mut lb_stream,
                4096,
                4096,
            )
            .await;

            current_server.ttlcn.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

pub async fn run_main(config: Config) -> Result<(), ThanosError> {
    let lb_addr: SocketAddr = format!("0.0.0.0:{}", config.self_port).parse().unwrap();
    let backend = Arc::new(Backend::from(config.servers));
    check_server_health(Arc::clone(&backend)).await?;
    let backend_clone = Arc::clone(&backend);
    if config.strategy == Strategy::LeastConnections {
        tokio::spawn(async move {
            let mut curidx = 0;
            loop {
                let mut least_conns = usize::MAX;
                backend_clone.servers.iter().for_each(|s| {
                    let curcon = s.ttlcn.load(Ordering::Relaxed);
                    if curcon < least_conns {
                        least_conns = curcon;
                        backend_clone.idx.store(curidx, Ordering::Relaxed);
                    }
                    curidx += 1;
                });
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        });
    }
    for _ in 0..config.core {
        let backend = Arc::clone(&backend);
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
                if let Err(e) = run_normal_proxy(lb_listener, backend).await {
                    plog(&e.to_string(), Log::Err);
                }
            }
        });
    }
    Ok(())
}
