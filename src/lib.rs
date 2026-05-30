use std::{error::Error, fmt, io};

pub mod config;

#[derive(Debug, PartialEq)]
pub enum ThanosError {
    ConnectionRefused,
    Other(String),
}

impl fmt::Display for ThanosError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#?}", self)
    }
}

impl From<Box<dyn Error>> for ThanosError {
    fn from(e: Box<dyn Error>) -> Self {
        ThanosError::Other(e.to_string())
    }
}
impl From<io::Error> for ThanosError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::ConnectionRefused => ThanosError::ConnectionRefused,
            s => ThanosError::Other(s.to_string()),
        }
    }
}
