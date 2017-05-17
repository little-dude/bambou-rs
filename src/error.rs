use std::{fmt, error};
use reqwest;

#[derive(Debug)]
pub enum Error {
    Request(reqwest::Error),
    InvalidResponse,
    NoSession,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Request(ref e) => fmt::Display::fmt(e, f),
            Error::InvalidResponse => f.write_str("Invalid response"),
            Error::NoSession => f.write_str("Entities must hold a reference to a session to perform ReST requests"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Request(ref e) => e.description(),
            Error::InvalidResponse => "Invalid response",
            Error::NoSession => "Entities must hold a reference to a session to perform ReST requests",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Request(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Request(err)
    }
}
