pub mod keymatch;
use core::panic;
use std::{env, error::Error, fs::File, io::Read, net::SocketAddr, path::Path, process};

use crate::{
    config::keymatch::{match_method, match_word},
    logs::{Log, plog},
};

/// Location of config file for Thanos
pub fn get_config_path(provided: Option<String>) -> String {
    let home = match env::var("HOME") {
        Ok(s) => s,
        Err(_) => "/home".to_string(),
    };
    let mut path = home;
    path.push_str(&match provided {
        Some(s) => s,
        None => "/.config/thanos/thanos.conf".to_string(),
    });
    path
}

pub enum Key {
    Port,
    Server,
    Method,
    Core,
    Unknown,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Method {
    Tproxy,
    Normal,
}

#[derive(Debug)]
pub struct Config {
    pub servers: Vec<SocketAddr>,
    pub self_port: u16,
    pub method: Method,
    pub core: u64,
}

pub struct CliConfig {
    pub servers: Option<Vec<SocketAddr>>,
    pub self_port: Option<u16>,
    pub method: Option<Method>,
    pub core: Option<u64>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            servers: None,
            self_port: None,
            method: None,
            core: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            self_port: 8080,
            servers: vec![],
            core: 1,
            method: Method::Normal,
        }
    }
}
impl Config {
    pub fn read_file(config_path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        if !Path::new(config_path).exists() {
            return Err("Failed to read config file.\nUsing default settings.".into());
        };

        let mut data = String::new();
        File::open(config_path)
            .expect("Unable to open config file")
            .read_to_string(&mut data)?;
        let mut filtered_data = String::new();
        for line in data.lines() {
            if let Some(cmt_idx) = line.find('#') {
                filtered_data.push_str(&line[..cmt_idx]);
            } else {
                filtered_data.push_str(line);
            }
            filtered_data.push('\n');
        }

        let data = filtered_data
            .split(";")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<String>>();

        Ok(data)
    }
    pub fn get(config: CliConfig, path: &str) -> Result<Self, Box<dyn Error>> {
        let mut result = Self::default();
        let data = match Config::read_file(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}", e);
                return Ok(result);
            }
        };

        for ins in data {
            let ins = ins.trim();
            // Place for commenting
            if ins.starts_with('#') {
                continue;
            }

            if ins.is_empty() {
                continue;
            }

            let pair = ins.split("=").map(|s| s.trim()).collect::<Vec<&str>>();
            if pair.len() != 2 {
                eprintln!("\"{}\" is not formatted properly.", ins);
                process::exit(1);
            }
            let key_word = match pair.first() {
                Some(s) => s.to_owned(),
                None => continue,
            };

            match match_word(key_word) {
                Key::Port => {
                    let port = match pair.get(1) {
                        Some(s) => s.to_owned(),
                        None => {
                            panic!("Port missing.");
                        }
                    };
                    println!("Port: {}", port);
                    let port = match port.parse::<u16>() {
                        Ok(s) => s,
                        Err(e) => {
                            panic!("Port must be a number.\n{}", e)
                        }
                    };
                    result.self_port = port;
                }
                Key::Server => {
                    let addr = match pair.get(1) {
                        None => {
                            panic!("Servers not provided.")
                        }
                        Some(s) if s.len() == 2 => {
                            panic!("Servers not provided or configured wrong.")
                        }
                        Some(s) => s.to_owned(),
                    }
                    .trim();
                    if addr.starts_with("[") && addr.ends_with("]") {
                        let addrs = &addr[1..addr.len() - 1]
                            .split(",")
                            .map(|s| s.trim())
                            .collect::<Vec<&str>>();
                        for &addr in addrs {
                            let addr = addr
                                .trim_matches('\"')
                                .to_owned()
                                .trim_matches('\'')
                                .to_owned();
                            let addr = match addr.parse::<SocketAddr>() {
                                Ok(s) => s,
                                Err(_) => {
                                    plog(
                                        &format!(
                                            "Cannot parse \"{}\" . Make sure it is formatted.",
                                            addr
                                        ),
                                        Log::Err,
                                    );
                                    process::exit(1);
                                }
                            };
                            result.servers.push(addr);
                        }
                    } else {
                        plog(
                            "Server Addresses should be contained in \"[\"\"]\"",
                            Log::Err,
                        );
                        process::exit(1);
                    }
                }
                Key::Method => {
                    let method = match pair.get(1) {
                        Some(s) => s.to_owned(),
                        None => {
                            plog("Method not provided, Using Default.", Log::Warn);
                            "normal"
                        }
                    }
                    .trim_matches('"')
                    .to_owned();
                    let method = match match_method(&method) {
                        Some(s) => s,
                        None => Method::Normal,
                    };

                    result.method = method;
                }
                Key::Core => {
                    let core = if let Some(c) = pair.get(1)
                        && let Ok(parsed_c) = c.parse::<u64>()
                    {
                        parsed_c
                    } else {
                        plog("No Cores Provided, Assigning Physical Cores", Log::Warn);
                        num_cpus::get_physical() as u64
                    };
                    result.core = core;
                }
                Key::Unknown => {
                    plog(&format!("Unknown Key \"{}\"", pair[0]), Log::Warn);
                }
            }
        }
        if let Some(servers) = config.servers
            && !servers.is_empty()
        {
            result.servers = servers;
        }

        if let Some(port) = config.self_port {
            result.self_port = port;
        }

        if let Some(method) = config.method {
            result.method = method;
        }
        if let Some(core) = config.core {
            result.core = core;
        }
        if result.servers.is_empty() {
            plog("Servers not Provided.", Log::Err);
            process::exit(1);
        }
        println!("Configuration set to:\n{:#?}", result);
        Ok(result)
    }
}
