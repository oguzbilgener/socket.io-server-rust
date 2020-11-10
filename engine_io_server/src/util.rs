use crate::server::CookieOptions;
use crate::transport::TransportKind;
use crate::adapter::Adapter;
use engine_io_parser::packet::ParsePacketError;
use serde::Serialize;
use std::collections::HashMap;

// FIXME: rename this to HttpRequestContext?
#[derive(Debug, Clone, PartialEq)]
pub struct RequestContext {
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub origin: Option<String>,
    pub secure:  bool,
    pub user_agent: String,
    pub content_type: String,
    pub transport_kind: TransportKind,
    pub http_method: HttpMethod,
    pub remote_address: String,
    pub request_url: String,
    pub set_cookie: Option<SetCookie>,
}

impl RequestContext {
    // TODO: take
    pub fn with_set_cookie(&self, set_cookie: Option<SetCookie>) -> RequestContext {
        RequestContext {
            set_cookie,
            ..self.clone()
        }
    }
}

#[derive(Display, Debug, Copy, Clone, PartialEq)]
pub enum HttpMethod {
    Options,
    Get,
    Post,
    Put,
    Delete,
    Head,
    Trace,
    Connect,
    Patch,
}

#[derive(Display, Debug, Clone, PartialEq, EnumString, IntoStaticStr)]
pub enum ServerError {
    #[strum(serialize = "Transport unknown")]
    UnknownTransport = 0,
    #[strum(serialize = "Session ID unknown")]
    UnknownSid = 1,
    #[strum(serialize = "Bad handshake method")]
    BadHandshakeMethod = 2,
    #[strum(serialize = "Bad request")]
    BadRequest = 3,
    #[strum(serialize = "Forbidden")]
    Forbidden = 4,
    #[strum(serialize = "Unknown")]
    Unknown = -1,
    #[strum(serialize = "Server is shutting down")]
    ShuttingDown = -99,
}

#[derive(Debug, Serialize)]
pub struct ServerErrorMessage {
    pub code: i8,
    pub message: String,
}

impl From<ServerError> for ServerErrorMessage {
    fn from(server_error: ServerError) -> Self {
        let message = server_error.to_string().to_owned();
        ServerErrorMessage {
            code: server_error as i8,
            message,
        }
    }
}

impl From<ParsePacketError> for ServerError {
    fn from(_: ParsePacketError) -> Self {
        // TODO: add more details
        ServerError::BadRequest
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetCookie {
    pub name: String,
    pub value: String,
    pub path: String,
    pub http_only: bool,
    pub same_site: bool,
}

impl SetCookie {
    pub(crate) fn from_cookie_options(
        cookie_options: &Option<CookieOptions>,
        value: String,
    ) -> Option<Self> {
        match cookie_options {
            Some(options) => Some(SetCookie {
                name: options.name.clone(),
                value,
                path: options.path.clone(),
                http_only: options.http_only,
                same_site: true,
            }),
            None => None,
        }
    }
}

impl From<SetCookie> for String {
    fn from(set_cookie: SetCookie) -> String {
        todo!();
    }
}

#[derive(Display, Debug, Clone, PartialEq)]
pub enum SendPacketError {
    UnknownConnectionId,
}

