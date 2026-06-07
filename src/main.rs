use mimalloc::MiMalloc;
use socket2::{SockAddr, SockRef};
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};
use thanos::{
    Backend, ThanosError, check_server_health,
    config::{Config, Method},
    logs::{Log, plog},
};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpSocket, TcpStream},
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

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
            let backend_clone = Arc::clone(&backend);
            let (mut lb_stream, client_addr) = match lb_listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    plog(&e.to_string(), Log::Err);
                    continue;
                }
            };
            tokio::spawn(async move {
                let current_idx =
                    backend_clone.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();

                let current_server = backend_clone.servers.get(current_idx).unwrap();
                let sr_socket = TcpSocket::new_v4().unwrap();
                let sock = SockRef::from(&sr_socket);
                _ = sock.set_ip_transparent_v4(true);
                _ = sock.set_reuse_address(true);
                let client_addr = SockAddr::from(SocketAddr::new(client_addr.ip(), 0));
                _ = sock.bind(&client_addr);
                let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                    Ok(s) => s,
                    Err(e) => {
                        plog(&e.to_string(), Log::Err);
                        return;
                    }
                };
                _ = sr_stream.set_nodelay(true);

                _ = tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await;
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
                let current_idx =
                    backend_clone.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();

                let current_server = backend_clone.servers.get(current_idx).unwrap();
                let mut sr_stream = match TcpStream::connect(current_server.addr).await {
                    Ok(s) => s,
                    Err(e) => {
                        plog(&e.to_string(), Log::Err);
                        return;
                    }
                };
                _ = sr_stream.set_nodelay(true);
                _ = tokio::io::copy_bidirectional_with_sizes(
                    &mut sr_stream,
                    &mut lb_stream,
                    4096,
                    4096,
                )
                .await;

                // let (lb_read, mut lb_write) = lb_stream.into_split();
                // let (sr_read, mut sr_write) = sr_stream.into_split();
                // let mut lb_read_buf = BufReader::with_capacity(4096, lb_read);
                // let mut sr_read_buf = BufReader::with_capacity(4096, sr_read);
                //
                // let cl_to_sr = tokio::io::copy_buf(&mut lb_read_buf, &mut sr_write);
                // let sr_to_cl = tokio::io::copy_buf(&mut sr_read_buf, &mut lb_write);
                // if let Err(e) = tokio::try_join!(cl_to_sr, sr_to_cl) {
                //     plog(&format!("Connection ended with error: {}", e), Log::Err);
                // }
            });
        }
    };
}
