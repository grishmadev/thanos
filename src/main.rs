use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};
use thanos::{
    Backend, ThanosError, check_server_health,
    config::{Config, Method},
    logs::{Log, plog},
};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let config = Config::get()?;
    println!("Config: {:#?}", config);
    let lb_addr = format!("0.0.0.0:{}", config.self_port);
    plog(
        &format!("Load Balancer running on port: {}", config.self_port),
        Log::Ok,
    );
    let backend = Arc::new(Backend::from(config.servers));

    check_server_health(Arc::clone(&backend)).await?;

    let lb_listener = TcpListener::bind(lb_addr).await?;
    if config.method == Method::Tproxy {
        loop {
            let (mut lb_stream, client_addr) = lb_listener.accept().await?;
            let backend_clone = Arc::clone(&backend);
            tokio::spawn(async move {
                let backend = backend_clone;
                loop {
                    let current_idx =
                        backend.idx.fetch_add(1, Ordering::Relaxed) % backend.servers.len();
                    let current_server = backend.servers.get(current_idx).unwrap();
                    let cur_addr = current_server.addr;
                    if current_server.is_healthy.load(Ordering::Relaxed) {
                        let sock =
                            Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
                        let client_addr = SockAddr::from(SocketAddr::new(client_addr.ip(), 0));

                        _ = sock.set_ip_transparent_v4(true);
                        _ = sock.set_reuse_address(true);
                        _ = sock.set_nonblocking(true);
                        _ = sock.bind(&client_addr);
                        _ = sock.connect(&SockAddr::from(cur_addr));
                        let mut sr_stream = match TcpStream::from_std(sock.into()) {
                            Ok(s) => s,
                            Err(e) => {
                                plog(&e.to_string(), Log::Err);
                                continue;
                            }
                        };

                        if let Err(e) =
                            tokio::io::copy_bidirectional(&mut lb_stream, &mut sr_stream).await
                        {
                            plog(&e.to_string(), Log::Err);
                            continue;
                        } else {
                            break;
                        };
                    }
                }
            });
        }
    } else {
        loop {
            let (mut lb_stream, _) = lb_listener.accept().await?;
            let backend_clone = Arc::clone(&backend);
            tokio::spawn(async move {
                let backend = backend_clone;
                loop {
                    let current_idx =
                        backend.idx.fetch_add(1, Ordering::Relaxed) % backend.servers.len();
                    let current_server = backend.servers.get(current_idx).unwrap();
                    if current_server.is_healthy.load(Ordering::Relaxed)
                        && let Ok(mut sr_stream) = TcpStream::connect(current_server.addr).await
                    {
                        if let Err(e) =
                            tokio::io::copy_bidirectional(&mut sr_stream, &mut lb_stream).await
                        {
                            plog(&e.to_string(), Log::Err);
                            continue;
                        };
                        break;
                    } else {
                        plog("Connection Refused.", Log::Err);
                    }
                }
            });
        }
    }

    // Ok(())
}
