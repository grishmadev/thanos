use mimalloc::MiMalloc;
use socket2::{Domain, Protocol, SockAddr, SockRef, Socket, Type};
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};
use thanos::{
    Backend, ThanosError, check_server_health,
    config::{Config, Method},
    logs::{Log, plog},
};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let config = Config::get()?;
    println!("Config: {:#?}", config);
    let lb_addr: SocketAddr = format!("0.0.0.0:{}", config.self_port).parse().unwrap();
    plog(
        &format!("Load Balancer running on port: {}", config.self_port),
        Log::Ok,
    );
    let backend = Arc::new(Backend::from(config.servers));

    check_server_health(Arc::clone(&backend)).await?;
    for _ in 0..2 {
        let lb_sock = Socket::new(Domain::IPV4, Type::STREAM, None)?;
        let backend = Arc::clone(&backend);

        lb_sock.set_reuse_address(true)?;
        lb_sock.set_reuse_port(true)?;
        lb_sock.bind(&SockAddr::from(lb_addr))?;
        lb_sock.listen(1024)?;

        tokio::spawn(async move {
            let lb_listener = TcpListener::from_std(lb_sock.into()).unwrap();
            if config.method == Method::Tproxy {
                loop {
                    println!("stuck");
                    let backend_clone = Arc::clone(&backend);
                    let (mut lb_stream, client_addr) = match lb_listener.accept().await {
                        Ok(s) => {
                            println!("found client");
                            s
                        }
                        Err(e) => {
                            plog(&e.to_string(), Log::Err);
                            continue;
                        }
                    };
                    tokio::spawn(async move {
                        let current_idx = backend_clone.idx.fetch_add(1, Ordering::Relaxed)
                            % backend_clone.servers.len();

                        let current_server = backend_clone.servers.get(current_idx).unwrap();
                        let sr_socket = TcpSocket::new_v4().unwrap();
                        let sock = SockRef::from(&sr_socket);
                        sock.set_ip_transparent_v4(true).unwrap();
                        sock.set_reuse_address(true).unwrap();
                        println!(
                            "assigned slient to {} w indx {}",
                            current_server.addr, current_idx
                        );
                        let client_addr = SockAddr::from(SocketAddr::new(client_addr.ip(), 0));
                        if let Err(e) = sock.bind(&client_addr) {
                            eprintln!("Bind Error: {}", e);
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

                        tokio::io::copy_bidirectional_with_sizes(
                            &mut sr_stream,
                            &mut lb_stream,
                            4096,
                            4096,
                        )
                        .await
                        .unwrap();
                    });
                }
            } else {
                loop {
                    let backend_clone = Arc::clone(&backend);
                    let (mut lb_stream, _) = match lb_listener.accept().await {
                        Ok(s) => s,
                        Err(e) => {
                            plog(&e.to_string(), Log::Err);
                            continue;
                        }
                    };
                    tokio::spawn(async move {
                        let current_idx = backend_clone.idx.fetch_add(1, Ordering::Relaxed)
                            % backend_clone.servers.len();

                        let current_server = backend_clone.servers.get(current_idx).unwrap();
                        let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                            Ok(s) => s,
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
                    });
                }
            }
        });
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}
