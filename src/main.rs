use std::net::SocketAddr;

use clap::{ArgGroup, Parser};
use mimalloc::MiMalloc;
use thanos::{
    ThanosError,
    config::{
        self, CliConfig, Config, Method, get_config_path,
        keymatch::{match_method, match_strategy},
    },
    proxy::run_main,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[command(
    name = "Thanos",
    version,
    about = "A Minimal Load Balancer built for Light Speed",
    long_about = "Thanos was built looking at speed.
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

    /// Pass a config file
    #[arg(short = 'C', long = "config")]
    config: Option<String>,

    /// Choose load balancing strategy
    #[arg(short = 'S', long = "strategy")]
    strategy: Option<String>,

    /// Choose number of CPU cores to use
    #[arg(short = 'c', long = "core")]
    core: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<(), ThanosError> {
    let args = Cli::parse();
    let mut def_conf = CliConfig::default();
    let path: String;
    println!("args: {:?}", args);
    if let Some(conf) = args.config {
        path = conf;
    } else {
        def_conf.method = match_method(&args.method);
        def_conf.servers = Some(args.server);
        def_conf.self_port = Some(args.port);
        if let Some(stgy) = args.strategy {
            def_conf.strategy = match_strategy(&stgy);
        }
        path = get_config_path(None);
    };
    let config = Config::get(def_conf, &path)?;

    run_main(config).await?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
