use crate::config::{Key, Method, Strategy};

pub fn match_key(key: Key) -> Option<String> {
    let word = match key {
        Key::Port => "port",
        Key::Server => "server",
        Key::Method => "method",
        Key::Core => "core",
        _ => return None,
    };
    Some(word.to_string())
}

pub fn match_word(word: &str) -> Key {
    match word {
        "port" => Key::Port,
        "server" | "servers" => Key::Server,
        "method" => Key::Method,
        "core" | "cores" => Key::Core,
        "strategy" => Key::Strategy,
        _ => Key::Unknown,
    }
}

pub fn match_method(method: &str) -> Option<Method> {
    let key = match &method.to_lowercase()[..] {
        "tproxy" => Method::Tproxy,
        "normal" => Method::Normal,
        _ => return None,
    };
    Some(key)
}

pub fn match_strategy(strategy: &str) -> Option<Strategy> {
    let stgy = match strategy.to_lowercase().as_str() {
        "roundrobin" | "round robin" => Strategy::RoundRobin,
        "leastconnection" | "least connection" | "least connections" | "leastconnections" => {
            Strategy::LeastConnections
        }
        _ => return None,
    };
    Some(stgy)
}
