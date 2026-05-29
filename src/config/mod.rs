pub mod keymatch;
use core::panic;
use std::{
    env,
    error::Error,
    fs::File,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    process,
};

use crate::config::keymatch::match_word;

/// Location of config file for Thanos
fn get_config_path() -> String {
    let home = match env::var("HOME") {
        Ok(s) => s,
        Err(_) => "/home".to_string(),
    };
    let mut path = home;
    path.push_str("/.config/thanos/thanos.conf");
    path
}

pub enum Key {
    Port,
    Server,
}

#[derive(Debug)]
pub struct Config {
    pub servers: Vec<SocketAddr>,
    pub self_port: u16,
    pub method: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            self_port: 8080,
            servers: vec![],
            method: String::new(),
        }
    }
}
impl Config {
    pub fn read_file() -> Result<Vec<String>, Box<dyn Error>> {
        let config_path = get_config_path();
        if !Path::new(&config_path).exists() {
            return Err("Failed to read config file.\nUsing default settings.".into());
        };

        let mut data = String::new();
        File::open(&config_path)
            .expect("Unable to open config file")
            .read_to_string(&mut data)?;

        let data = data
            .split(";")
            .map(|s| s.trim().to_string())
            .collect::<Vec<String>>();
        Ok(data)
    }
    pub fn get() -> Result<Self, Box<dyn Error>> {
        let mut result = Self::default();
        let data = match Config::read_file() {
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
                    panic!("Syntax error at or near {}", key_word);
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
                        Some(s) => s.to_owned(),
                        None => {
                            panic!("Servers not provided.")
                        }
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
                                Err(e) => {
                                    panic!(
                                        "Cannot parse {} . Make sure it is formatted.\n{}",
                                        addr, e
                                    );
                                }
                            };
                            result.servers.push(addr);
                        }
                    } else {
                        panic!("Server Addresses should be contained in \"[\"\"]\"");
                    }
                }
            }
        }
        Ok(result)
    }
}
