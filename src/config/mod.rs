pub mod keymatch;
use core::panic;
use std::{default, env, error::Error, fs::File, io::Read, net::SocketAddr, path::Path, process};

use crate::{
    DEFAULT_PORT,
    config::{
        self,
        keymatch::{match_method, match_word},
    },
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

            let key = match match_word(key_word) {
                Some(s) => s,
                None => {
                    panic!("Syntax error at or near \"{}\"", key_word);
                }
            };
            match key {
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
                                            "Cannot parse {} . Make sure it is formatted.",
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
                        panic!("Server Addresses should be contained in \"[\"\"]\"");
                    }
                }
                Key::Method => {
                    let method = match pair.get(1) {
                        Some(s) => s.to_owned(),
                        None => {
                            panic!("Method not provided.")
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
                Key::Core => {}
            }
        }
        if result.servers.is_empty() {
            plog("Servers not Provided.", Log::Err);
            process::exit(1);
        }
        if let Some(s) = config.servers
            && !s.is_empty()
        {
            result.servers = s;
        }

        if let Some(s) = config.self_port {
            result.self_port = s;
        }

        if let Some(s) = config.method {
            result.method = s;
        }

        Ok(result)
    }
}
