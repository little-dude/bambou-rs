use std::{fmt, error};
use reqwest;
use hyper;

#[derive(Debug)]
pub enum Error {
    InvalidUrl(hyper::error::ParseError),
    Request(reqwest::Error),
    MissingId,
    NoEntity,
    NoSession,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::InvalidUrl(ref e) => fmt::Display::fmt(e, f),
            Error::Request(ref e) => fmt::Display::fmt(e, f),
            Error::MissingId => f.write_str("The entity does not have an ID"),
            Error::NoEntity => f.write_str("No entity in response body"),
            Error::NoSession => f.write_str("Entities must hold a reference to a session to perform ReST requests"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::InvalidUrl(ref e) => e.description(),
            Error::Request(ref e) => e.description(),
            Error::MissingId => "The entity does not have an ID",
            Error::NoEntity => "No entity in response body",
            Error::NoSession => "Entities must hold a reference to a session to perform ReST requests",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::InvalidUrl(ref e) => Some(e),
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

impl From<hyper::error::ParseError> for Error {
    fn from(err: hyper::error::ParseError) -> Self {
        Error::InvalidUrl(err)
    }
}
