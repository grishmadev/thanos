use crate::config::{Key, Method};

pub fn match_key(key: Key) -> Option<String> {
    let word = match key {
        Key::Port => "port",
        _ => {
            return None;
        }
    };
    Some(word.to_string())
}

pub fn match_word(word: &str) -> Option<Key> {
    let key = match word {
        "port" => Key::Port,
        "server" | "servers" => Key::Server,
        "method" => Key::Method,
        _ => return None,
    };
    Some(key)
}

pub fn match_method(method: &str) -> Option<Method> {
    let key = match &method.to_lowercase()[..] {
        "tproxy" => Method::Tproxy,
        "normal" => Method::Normal,
        _ => return None,
    };
    Some(key)
}
