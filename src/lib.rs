#[macro_use]
extern crate hyper;
extern crate serde;
extern crate serde_json;
extern crate reqwest;

pub mod error;

use reqwest::{Client, ClientBuilder, Response, Url};
use reqwest::header::{Headers, Authorization, Basic, ContentType};
use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};
use serde::Serialize;

pub use error::{Error};
pub use reqwest::Certificate;

pub trait RestEntity<'a>: Serialize + for<'de> serde::Deserialize<'de> {
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
    fn fetch(&mut self) -> Result<Response, Error>;

    /// Update the entity on the server from its attributes.
    fn save(&mut self) -> Result<Response, Error>;

    /// Delete the entity from the server.
    fn delete(self) -> Result<Response, Error>;

    /// Fetch children entities from the server.
    // fn fetch_children<C>(&self, children: &mut Vec<C>) -> Fetcher<C>
    fn fetch_children<C>(&self, children: &mut Vec<C>) -> Result<Response, Error>
        where C: RestEntity<'a>;

    /// Create a child entity.
    fn create_child<C>(&self, child: &mut C) -> Result<Response, Error>
        where C: RestEntity<'a>;
}
pub trait RestRootEntity<'a>: RestEntity<'a> {
    /// Return the API key for the current session. After the first password authentication, the
    /// root entity should hold an API key. This API key is then used by the session for all
    /// subsequent requests.
    fn get_api_key(&self) -> Option<&str>;
}

pub struct SessionBuilder {
    client_builder: ClientBuilder,
    pub url: Url,
    pub username: String,
    pub password: String,
    pub api_key: Option<String>,
    pub organization: String,
}

impl SessionBuilder {
    /// Create a new session builder
    pub fn new(url: &str, login: &str, password: &str, organization: &str) -> Result<Self, Error> {
        let session = SessionBuilder {
            client_builder: ClientBuilder::new()?,
            url: Url::parse(url)?,
            username: login.to_owned(),
            password: password.to_owned(),
            organization: organization.to_owned(),
            api_key: None,
        };
        Ok(session)
    }

    pub fn add_root_certificate(&mut self, cert: Certificate) -> Result<(), Error> {
        self.client_builder.add_root_certificate(cert)?;
        Ok(())
    }

    /// Disable hostname verification
    pub fn danger_disable_hostname_verification(&mut self) {
        self.client_builder.danger_disable_hostname_verification();
    }

    /// Enable hostname verification
    pub fn enable_hostname_verification(&mut self) {
        self.client_builder.enable_hostname_verification();
    }

    pub fn build(mut self) -> Result<Session, Error> {
        Ok(Session {
            client: self.client_builder.build()?,
            url: self.url,
            username: self.username,
            password: self.password,
            api_key: self.api_key,
            organization: self.organization,
        })
    }
}

header! { (XNuageOrganization, "X-Nuage-Organization") => [String] }

#[derive(Clone, Debug)]
pub struct Session {
    client: Client,
    pub url: Url,
    pub username: String,
    pub password: String,
    pub api_key: Option<String>,
    pub organization: String,
}

impl<'a> Session {

    /// Delete an entity. This consumes the entity.
    pub fn delete<E>(&self, entity: E) -> Result<Response, Error>
        where E: RestEntity<'a>
    {
        let url = self.entity_url(&entity)?;
        let headers = self.headers();
        let resp = self.client
            .delete(url)?
            .headers(headers)
            .send()?;
        Ok(resp)
    }

    /// Save an entity.
    pub fn save<E>(&'a self, entity: &mut E) -> Result<Response, Error>
        where E: RestEntity<'a>
    {
        let headers = self.headers();
        let url = self.entity_url(entity)?;

        let mut resp = self.client
            .put(url)?
            .headers(headers)
            .json(entity)?
            .send()?;

        let mut entities: Vec<E> = resp.json()?;
        *entity = entities.pop().ok_or(Error::NoEntity)?;
        entity.set_session(self);
        Ok(resp)
    }

    /// Create a child under the parent, and give the child a reference to the current session.
    pub fn create_child<P, C>(&'a self, parent: &P, child: &mut C) -> Result<Response, Error>
        where P: RestEntity<'a>,
              C: RestEntity<'a>
    {
        let url = if parent.is_root() {
            self.url.join(C::group_path())?
        } else {
            self.entity_url(parent)?.join(C::group_path())?
        };
        let headers = self.headers();

        let mut resp = self.client
            .post(url)?
            .headers(headers)
            .json(child)?
            .send()?;

        let mut entities: Vec<C> = resp.json()?;
        *child = entities.pop().ok_or(Error::NoEntity)?;
        child.set_session(self);
        Ok(resp)
    }

    /// Fetch the children of a parent entity, and give the children a reference to the current
    /// session.
    pub fn fetch_children<P, C>(&'a self, parent: &P, children: &mut Vec<C>) -> Result<Response, Error>
        where P: RestEntity<'a>,
              C: RestEntity<'a>
    {
        let url = if parent.is_root() {
            self.url.join(C::group_path())?
        } else {
            self.entity_url(parent)?.join(C::group_path())?
        };
        let headers = self.headers();
        let mut resp = self.client
            .get(url)?
            .headers(headers)
            .send()?;

        // XXX: No idea why I can't just write `children = resp.json()?;`
        let children_: Vec<C> = resp.json()?;
        *children = children_;

        for mut child in children {
            child.set_session(self);
        }
        Ok(resp)
    }

    /// Start a new session. The root object is populated with a reference to the session.
    pub fn connect<R>(&'a mut self, root: &mut R) -> Result<Response, Error>
        where R: RestRootEntity<'a>
    {
        let url = self.entity_url(root)?;
        let headers = self.headers();
        let client = self.client.clone();
        let mut resp = client
            .get(url)?
            .headers(headers)
            .send()?;
        let mut entities: Vec<R> = resp.json()?;
        *root = entities.pop().ok_or(Error::NoEntity)?;
        self.api_key = root.get_api_key().map(|s| s.to_string());
        root.set_session(self);
        Ok(resp)
    }

    /// Fetch an entity and populate its attributes, and set its session.
    pub fn fetch_entity<E>(&'a self, entity: &mut E) -> Result<Response, Error>
        where E: RestEntity<'a>
    {
        let url = self.entity_url(entity)?;
        let headers = self.headers();
        let mut resp = self.client
            .get(url)?
            .headers(headers)
            .send()?;
        let mut entities: Vec<E> = resp.json()?;
        *entity = entities.pop().unwrap();
        entity.set_session(self);
        Ok(resp)
    }

    fn headers(&self) -> Headers {
        let mut headers = Headers::new();

        // X-Nuage-Organization: organization
        headers.set(XNuageOrganization(self.organization.clone()));

        // content-type: application/json
        headers.set(ContentType(Mime(TopLevel::Application, SubLevel::Json, vec![(Attr::Charset, Value::Utf8)])));

        // Authorization: base64("login:password")
        // or if we have an API Key already:
        // Authorization: base64("login:api_key")
        headers.set(Authorization(Basic {
            username: self.username.clone(),
            password: self.api_key.clone().or_else(|| Some(self.password.clone())),
        }));

        headers
    }

    fn entity_url<E>(&self, entity: &E) -> Result<Url, Error>
        where E: RestEntity<'a>
    {
        let url = self.url
            .join(E::path())?
            .join(entity.id().ok_or(Error::MissingId)?)?;
        Ok(url)
    }

}
