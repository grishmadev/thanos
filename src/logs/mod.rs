use chrono::Utc;

pub enum Log {
    Ok,
    Err,
    Info,
    Warn,
}

pub fn plog(text: &str, msg_type: Log) {
    let time = Utc::now().time();
    let msg = match msg_type {
        Log::Ok => "OK",
        Log::Err => "Error",
        Log::Info => "Info",
        Log::Warn => "Warn",
    };
    eprintln!("[ {time} ][ {msg} ] {text}");
}
