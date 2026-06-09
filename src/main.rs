use std::net::SocketAddr;

use clap::{ArgGroup, Parser};
use mimalloc::MiMalloc;
use thanos::{
    ThanosError,
    config::{self, CliConfig, Config, Method, get_config_path, keymatch::match_method},
    proxy::run_main,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[command(
    name = "Thanos",
    version,
    about = "A Minimal Load Balancer built for Light Speed",
    long_about = "Thanos was built looking at speed, exceeding 20k RPS on a 2 Core CPU with Celeron Chip.
    Usage: thanos -p 8080 -m tproxy -s 127.0.0.1:8888 -s 127.0.0.1:8889 -s 127.0.0.1:8890"
)]
pub struct Cli {
    /// Port to run Load Balancer on
    #[arg(short = 'p', long = "port", default_value_t = 8080)]
    port: u16,

    /// Assign servers for load balancing
    #[arg(short = 's', long = "server", action = clap::ArgAction::Append)]
    server: Vec<SocketAddr>,

    /// Method to use between normal and tproxy
    #[arg(short = 'm', long = "method", default_value = "normal")]
    method: String,

    #[arg(long = "config", default_value = "")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let args = Cli::parse();
    let mut def_conf = CliConfig::default();
    let path: String;
    if args.config.is_empty() {
        def_conf.method = match match_method(&args.method) {
            Some(s) => s,
            None => Method::Normal,
        };
        def_conf.servers = Some(args.server);
        def_conf.self_port = args.port;
        path = get_config_path(None)
    } else {
        path = args.config;
    };
    let config = Config::get(def_conf, &path)?;

    run_main(config).await?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
