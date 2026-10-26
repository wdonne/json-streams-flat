use config::ConfigError;
use log::error;
use salvo::http::StatusError;
use salvo::jwt_auth::JwtAuthError;
use salvo::{Response, Scribe};
use std::io;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum ServerError {
    #[error("HTTP body parse error: {0}")]
    Body(#[from] salvo::http::ParseError),
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON parse error: {0}")]
    ImmutableJson(#[from] immutable_json::error::Error),
    #[error("internal error")]
    Internal,
    #[error("Jinja error: {0}")]
    Jinja(#[from] minijinja::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JWT auth config error: {0}")]
    JwtAuth(#[from] JwtAuthError),
    #[error("no aggregation")]
    NoAggregation,
    #[error("no json-streams-path")]
    NoJsonStreamsPath,
    #[error("no path")]
    NoPath,
    #[error("request body is not a JSON object: {0}")]
    NotObject(String),
    #[error("salvo: {0}")]
    Salvo(#[from] salvo_from_config::error::Error),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("UTF-8 encoding error: {0}")]
    Utf8(#[from] FromUtf8Error),
}

impl Scribe for ServerError {
    fn render(self, res: &mut Response) {
        error!("ServerError: {0}", self);
        match self {
            ServerError::Body(_) => res.render(StatusError::bad_request()),
            ServerError::ImmutableJson(_) => res.render(StatusError::bad_request()),
            ServerError::NotObject(_) => res.render(StatusError::bad_request()),
            ServerError::Utf8(_) => res.render(StatusError::bad_request()),
            _ => res.render(StatusError::internal_server_error().brief("unknown error")),
        }
    }
}
