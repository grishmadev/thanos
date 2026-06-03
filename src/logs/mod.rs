use chrono::Utc;

pub fn p_err(text: &str) {
    let time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    eprintln!("[ {} ][ Error ] {text}", time);
}

pub fn p_info(text: &str) {
    let time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    eprintln!("[ {} ][ Info ] {text}", time);
}

pub fn p_warn(text: &str) {
    let time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    eprintln!("[ {} ][ Warn ] {text}", time);
}

pub fn p_suc(text: &str) {
    let time = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    eprintln!("[ {} ][ OK ] {text}", time);
}
