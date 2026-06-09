use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};

use socket2::{SockAddr, SockRef};
use tokio::net::{TcpListener, TcpSocket, TcpStream};

use crate::{
    Backend,
    logs::{Log, plog},
};

pub async fn run_tproxy_method(
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
        tokio::spawn(async move {
            let current_idx =
                backend_clone.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();

            let current_server = backend_clone.servers.get(current_idx).unwrap();
            let sr_socket = TcpSocket::new_v4().unwrap();
            let sock = SockRef::from(&sr_socket);
            sock.set_ip_transparent_v4(true).unwrap();
            sock.set_reuse_address(true).unwrap();
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
        tokio::spawn(async move {
            let current_idx =
                backend_clone.idx.fetch_add(1, Ordering::Relaxed) % backend_clone.servers.len();

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
