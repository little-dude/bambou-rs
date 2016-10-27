#![feature(conservative_impl_trait)]
#![feature(plugin)]
#![plugin(clippy)]
#[macro_use]
extern crate hyper;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate log;

pub mod error;

use hyper::client::{Client, Response};
use hyper::header::{Headers, Authorization, Basic, ContentType};
use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};
use hyper::status::StatusCode;
use std::io::Read;

pub use error::{BambouError, Reason};

pub trait RestEntity<'a>: serde::Serialize + serde::Deserialize {
    /// Give a reference to an existing session to the entity. Without a session, an entity is
    /// pretty much useless, since it cannot be fetched, updated, deleted or used to create
    /// children entities.
    fn set_session(&mut self, session: &'a Session);

    /// Return a reference to a session. This session is used to make server requests.
    fn get_session(&self) -> Option<&Session>;

    /// Return the rest path of the entity, without its ID.
    fn path() -> &'static str;

    /// Return the rest path of the entity's group. This is usually the same than de entity's path.
    fn group_path() -> &'static str;

    /// Return the ID of the entity. It may not exist (if the entity does not exist on the server
    /// for instance), or just not be known on the client side.
    fn id(&self) -> Option<&str>;

    /// Must return true if the entity is a root of the API and false otherwise.
    fn is_root(&self) -> bool;

    /// Fetch the entity from the server and populate its attributes from the response.
    fn fetch(&mut self) -> Result<Response, BambouError>;

    /// Update the entity on the server from its attributes.
    fn save(&mut self) -> Result<Response, BambouError>;

    /// Delete the entity from the server.
    fn delete(self) -> Result<Response, BambouError>;

    /// Fetch children entities from the server.
    fn fetch_children<C>(&self, children: &mut Vec<C>) -> Result<Response, BambouError>
        where C: RestEntity<'a>;

    /// Create a child entity.
    fn create_child<C>(&self, child: &mut C) -> Result<Response, BambouError>
        where C: RestEntity<'a>;
}

pub trait RestRootEntity<'a>: RestEntity<'a> {
    /// Return the API key for the current session. After the first password authentication, the
    /// root entity should hold an API key. This API key is then used by the session for all
    /// subsequent requests.
    fn get_api_key(&self) -> Option<&str>;
}

pub struct Session {
    client: Client,
    config: SessionConfig,
}

pub struct SessionConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub api_key: Option<String>,
    pub organization: String,
    pub root: String,
}

header! { (XNuageOrganization, "X-Nuage-Organization") => [String] }

impl<'a> Session {
    /// Create a new session.
    pub fn new(config: SessionConfig) -> Self {
        Session {
            config: config,
            client: Client::new(),
        }
    }

    /// Delete an entity. This consumes the entity.
    pub fn delete<E>(&self, entity: E) -> Result<Response, BambouError>
        where E: RestEntity<'a>
    {
        Ok(try!(self.http_delete(&self.get_entity_path(&entity), self.get_common_headers())).0)
    }

    /// Save an entity.
    pub fn save<E>(&'a self, entity: &mut E) -> Result<Response, BambouError>
        where E: RestEntity<'a>
    {
        let path = self.get_entity_path(entity);
        let body = try!(serde_json::to_string(&entity));
        let headers = self.get_common_headers();

        let (resp, body) = try!(self.http_put(&path, &body, headers));

        let mut entities: Vec<E> = try!(serde_json::from_str(&body));
        *entity = try!(entities.pop()
            .ok_or(BambouError::InvalidResponse("Failed to read updated entity : body is empty."
                .to_string())));
        entity.set_session(self);

        Ok(resp)
    }

    /// Create a child under the parent, and give the child a reference to the current session.
    pub fn create_child<P, C>(&'a self, parent: &P, child: &mut C) -> Result<Response, BambouError>
        where P: RestEntity<'a>,
              C: RestEntity<'a>
    {
        let mut path = String::new();
        if !parent.is_root() {
            self.get_entity_path(parent);
        }
        path.push_str(C::group_path());

        let body = try!(serde_json::to_string(&child));

        let (resp, body) = try!(self.http_post(&path, &body, self.get_common_headers()));

        let mut entities: Vec<C> = try!(serde_json::from_str(&body));
        *child = entities.pop().unwrap();
        child.set_session(self);
        Ok(resp)
    }

    /// Fetch the children of a parent entity, and give the children a reference to the current
    /// session.
    pub fn fetch_children<P, C>(&'a self,
                                parent: &P,
                                children: &mut Vec<C>)
                                -> Result<Response, BambouError>
        where P: RestEntity<'a>,
              C: RestEntity<'a>
    {
        let mut path = String::new();
        if !parent.is_root() {
            self.get_entity_path(parent);
        }
        path.push_str(C::path());
        let (resp, body) = try!(self.http_get(&path, self.get_common_headers()));
        *children = try!(serde_json::from_str(&body));
        for child in children {
            child.set_session(self);
        }
        Ok(resp)
    }

    /// Fetch and entity, populate its attributes, and set the session on the entity.
    pub fn fetch<E>(&'a self, entity: &mut E) -> Result<Response, BambouError>
        where E: RestEntity<'a>
    {
        let resp = try!(self.fetch_entity(entity));
        entity.set_session(self);
        Ok(resp)
    }

    /// Start a new session. The root object is populated with a reference to the session.
    pub fn connect<R>(&'a mut self, root: &mut R) -> Result<Response, BambouError>
        where R: RestRootEntity<'a>
    {
        info!("Starting new session...");
        let resp = try!(self.fetch_entity(root));
        self.config.api_key = Some(root.get_api_key().unwrap().to_string());
        root.set_session(self);
        info!("New session started");
        Ok(resp)
    }

    /// Fetch an entity and populate its attributes, but do not set the session on the entity.
    fn fetch_entity<E>(&self, entity: &mut E) -> Result<Response, BambouError>
        where E: RestEntity<'a>
    {
        let (resp, body) =
            try!(self.http_get(&self.get_entity_path(entity), self.get_common_headers()));

        // The api is weird: even when fetching a single object, we get a list
        let mut entities: Vec<E> = try!(serde_json::from_str(&body));
        *entity = try!(entities.pop()
            .ok_or(BambouError::InvalidResponse("Failed to fetch entity: body is empty."
                .to_string())));
        Ok(resp)
    }

    /// Send a PUT request
    fn http_put(&self,
                path: &str,
                body: &str,
                headers: Headers)
                -> Result<(Response, String), BambouError> {
        let path = self.get_url(path);
        info!("PUT >>> {} {}", path, body);
        debug!("{}", &headers);

        let mut resp = try!(self.client.put(&path).body(body).headers(headers).send());
        let body = try!(Session::read_response(&mut resp));
        info!("PUT <<< {} {}", &resp.status, &body);
        debug!("{}", &resp.headers);

        if resp.status != StatusCode::Ok {
            return Err(BambouError::RequestFailed(Reason {
                message: body,
                status_code: resp.status,
            }));
        }
        Ok((resp, body))
    }

    /// Send a GET request
    fn http_get(&self, path: &str, headers: Headers) -> Result<(Response, String), BambouError> {
        let path = self.get_url(path);
        info!("GET >>> {}", path);
        debug!("{}", &headers);

        let mut resp = try!(self.client.get(&path).headers(headers).send());
        let body = try!(Session::read_response(&mut resp));
        info!("GET <<< {} {}", &resp.status, &body);
        debug!("{}", &resp.headers);

        if resp.status != StatusCode::Ok {
            return Err(BambouError::RequestFailed(Reason {
                message: body,
                status_code: resp.status,
            }));
        }

        Ok((resp, body))
    }

    /// Send a POST request
    fn http_post(&self,
                 path: &str,
                 body: &str,
                 headers: Headers)
                 -> Result<(Response, String), BambouError> {
        let path = self.get_url(path);
        info!("POST >>> {} {}", path, body);
        debug!("{}", &headers);

        let mut resp = try!(self.client.post(&path).body(body).headers(headers).send());

        let body = try!(Session::read_response(&mut resp));
        info!("POST <<< {} {}", &resp.status, &body);
        debug!("{}", &resp.headers);

        if resp.status != StatusCode::Created {
            return Err(BambouError::RequestFailed(Reason {
                message: body,
                status_code: resp.status,
            }));
        }

        Ok((resp, body))
    }

    /// Send a DELETE request
    fn http_delete(&self, path: &str, headers: Headers) -> Result<(Response, String), BambouError> {
        let path = self.get_url(path);
        info!("DELETE >>> {}", path);
        debug!("{}", &headers);

        let mut resp = try!(self.client.delete(&path).headers(headers).send());

        let body = try!(Session::read_response(&mut resp));
        info!("DELETE <<< {} {}", &resp.status, &body);
        debug!("{}", &resp.headers);

        if resp.status != StatusCode::NoContent {
            return Err(BambouError::RequestFailed(Reason {
                message: body,
                status_code: resp.status,
            }));
        }

        Ok((resp, body))
    }

    /// Return the response body
    fn read_response(resp: &mut Response) -> Result<String, BambouError> {
        let mut body = String::new();
        try!(resp.read_to_string(&mut body));
        Ok(body)
    }

    fn get_common_headers(&self) -> Headers {
        let mut headers = Headers::new();

        // X-Nuage-Organization: organization
        headers.set(XNuageOrganization(self.config.organization.clone()));

        // content-type: application/json
        headers.set(ContentType(Mime(TopLevel::Application,
                                     SubLevel::Json,
                                     vec![(Attr::Charset, Value::Utf8)])));

        // Authorization: base64("login:password")
        // or if we have an API Key already:
        // Authorization: base64("login:api_key")
        headers.set(Authorization(Basic {
            username: self.config.username.clone(),
            password: self.config
                .api_key
                .as_ref()
                .and_then(|api_key| Some(api_key.clone()))
                .or_else(|| Some(self.config.password.clone())),
        }));

        headers
    }

    fn get_url(&self, path: &str) -> String {
        let mut url = self.config.url.clone();
        url.push_str(&self.config.root);
        url.push_str(path);
        url
    }

    fn get_entity_path<E>(&self, entity: &E) -> String
        where E: RestEntity<'a>
    {
        let mut path = E::path().to_string();
        if entity.is_root() {
            return path;
        }
        path.push('/');
        path.push_str(entity.id().unwrap_or(""));
        path
    }
}
