use std::io;
use std::fmt;
use std::error::Error;
use serde_json;
use hyper;

#[derive(Debug)]
pub enum BambouError {
    /// Indicate a failure to read a response's body.
    CannotReadResponse(io::Error),

    /// Indicate a failure to parse the response body. This is usually a serialization error,
    /// either because of invalid JSON or because the serialization is not implemented for this
    /// JSON.
    CannotParseResponse(serde_json::Error),

    /// Indicate a failure in the HTTP layer, either when sending a request or receiving a
    /// response.
    HttpError(hyper::Error),

    /// Indicate that the response is invalid.
    InvalidResponse(String),

    /// Indicate that the server failed to fullfil a request, and give details about the failure.
    RequestFailed(Reason),

    /// Indicate that an entity does not hold a session. Hence, it cannot perform any server on the
    /// server.
    NoSession,
}

/// Give details about why a request failed.
#[derive(Debug)]
pub struct Reason {
    /// Message contained in the server's response
    pub message: String,
    /// HTTP status code from the server's response
    pub status_code: hyper::status::StatusCode,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "status code: {}, message: {}",
               &self.message,
               self.status_code)
    }
}

impl fmt::Display for BambouError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BambouError::CannotReadResponse(ref err) => {
                write!(f, "IO error while reading response body: {}", err)
            }
            BambouError::CannotParseResponse(ref err) => write!(f, "Failed to parse body: {}", err),
            BambouError::HttpError(ref err) => write!(f, "HTTP error: {}", err),
            BambouError::InvalidResponse(ref err) => write!(f, "An unknown error occured: {}", err),
            BambouError::RequestFailed(ref err) => write!(f, "Request failed: {}", err),
            BambouError::NoSession => {
                write!(f,
                       "The entity does not hold any session so it cannot perform any request")
            }
        }
    }
}

impl Error for BambouError {
    fn description(&self) -> &str {
        match *self {
            BambouError::CannotReadResponse(ref err) => err.description(),
            BambouError::CannotParseResponse(ref err) => err.description(),
            BambouError::HttpError(ref err) => err.description(),
            BambouError::InvalidResponse(ref err) => err,
            BambouError::RequestFailed(_) => "The server failed to fullfil a request",
            BambouError::NoSession => {
                "The entity does not hold any session so it cannot perform any request"
            }
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            BambouError::CannotReadResponse(ref err) => Some(err),
            BambouError::CannotParseResponse(ref err) => Some(err),
            BambouError::HttpError(ref err) => Some(err),
            BambouError::NoSession |
            BambouError::InvalidResponse(_) |
            BambouError::RequestFailed(_) => None,
        }
    }
}

impl From<io::Error> for BambouError {
    fn from(err: io::Error) -> BambouError {
        BambouError::CannotReadResponse(err)
    }
}

impl From<serde_json::Error> for BambouError {
    fn from(err: serde_json::Error) -> BambouError {
        BambouError::CannotParseResponse(err)
    }
}

impl From<hyper::Error> for BambouError {
    fn from(err: hyper::Error) -> BambouError {
        BambouError::HttpError(err)
    }
}
