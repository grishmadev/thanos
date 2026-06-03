use std::sync::{Arc, atomic::Ordering};
use thanos::{Backend, ThanosError, check_server_health, config::Config, logs::p_err};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let config = Config::get()?;
    let lb_addr = format!("0.0.0.0:{}", config.self_port);
    println!("Load Balancer running on port: {}", config.self_port);
    let backend = Arc::new(Backend::from(config.servers));
    check_server_health(Arc::clone(&backend)).await?;

    let lb_listener = TcpListener::bind(lb_addr).await?;

    loop {
        let (mut lb_stream, _) = lb_listener.accept().await?;
        let backend_clone = Arc::clone(&backend);
        tokio::spawn(async move {
            let backend = Arc::clone(&backend_clone);
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
                        p_err(&format!("Copy Error: {}", e));
                        continue;
                    };
                    break;
                } else {
                    p_err("Connection Refused.");
                }
            }
        });
    }

    // Ok(())
}
