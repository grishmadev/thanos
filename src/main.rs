use mimalloc::MiMalloc;
use socket2::{SockAddr, SockRef};
use std::{net::SocketAddr, sync::Arc};
use thanos::{
    Backend, ThanosError, check_server_health,
    config::{Config, Method},
    logs::{Log, plog},
    proxy::{run_normal_proxy, run_tproxy_method},
};
use tokio::net::{TcpListener, TcpSocket};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let config = Config::get()?;
    println!("Config: {:#?}", config);
    let lb_addr: SocketAddr = format!("0.0.0.0:{}", config.self_port).parse().unwrap();
    let cores = num_cpus::get_physical();
    plog(
        &format!(
            "Load Balancer running on port {} with {} CPU Cores",
            config.self_port, cores
        ),
        Log::Ok,
    );
    let backend = Arc::new(Backend::from(config.servers));

    check_server_health(Arc::clone(&backend)).await?;
    for _ in 0..cores {
        let backend = Arc::clone(&backend);
        let lb_sock = TcpSocket::new_v4()?;
        let lb_sockref = SockRef::from(&lb_sock);

        lb_sockref.set_reuse_address(true)?;
        lb_sockref.set_reuse_port(true)?;
        lb_sockref.bind(&SockAddr::from(lb_addr))?;

        tokio::spawn(async move {
            let lb_listener: TcpListener = lb_sock.listen(1024).unwrap();
            // let lb_listener = TcpListener::from_std(std_list).unwrap();
            if config.method == Method::Tproxy {
                if let Err(e) = run_tproxy_method(lb_listener, backend).await {
                    plog(&e.to_string(), Log::Err);
                };
            } else {
                if let Err(e) = run_normal_proxy(lb_listener, backend).await {
                    plog(&e.to_string(), Log::Err);
                }
            }
        });
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}
