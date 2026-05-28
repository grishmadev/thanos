use crate::config::Key;

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
        _ => return None,
    };
    Some(key)
}
