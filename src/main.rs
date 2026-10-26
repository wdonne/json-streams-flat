use crate::error::ServerError::{
    self, Internal, NoAggregation, NoJsonStreamsPath, NoPath, NotObject,
};
use clap::Parser;
use config::{Config, ValueKind};
use http::header::AUTHORIZATION;
use imbl::Vector;
use imbl_util::vector::VectorExt;
use immutable_json::replace::replace_array;
use immutable_json::yaml::from_yaml;
use immutable_json::{
    api::Value, array::Array, config::from_config, object::Object, serde::to_value,
};
use iter_util::fold_result::FoldResultExt;
use log::{debug, info};
use minijinja::{Environment, value::Serde};
use reqwest::Client;
use reqwest::Url;
use rust_yaml::yaml::Yaml;
use salvo::Depot;
use salvo::FlowCtrl;
use salvo::async_trait;
use salvo::routing::PathParams;
use salvo::{Request, Response, Router};
use salvo_from_config::Server;
use std::str::FromStr;

mod error;

const AGGREGATION: &str = "aggregation";
const CONTEXT_PATH: &str = "context-path";
const HANDLERS: &str = "handlers";
const JSON_STREAMS_ENDPOINT: &str = "json-streams-endpoint";
const JSON_STREAMS_PATH: &str = "json-streams-path";
const PATH: &str = "path";
const VERSION: &str = "1.0.0";

#[derive(Parser)]
struct Cli {
    #[arg(
        short,
        long,
        default_value = "8080",
        long_help = "The port the service will bind to"
    )]
    port: Option<u16>,
}

#[derive(Clone)]
struct Handler {
    aggregation: Option<Array>,
    client: Client,
    environment: Option<Environment<'static>>,
    json_streams_endpoint: Url,
    path: String,
}

#[async_trait]
impl salvo::Handler for Handler {
    async fn handle(
        &self,
        req: &mut Request,
        _: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let params = req.params().to_owned();

        match body(req).await {
            Ok(o) => {
                debug!("Received request {}", o);
                match self.json_streams_body(&o, &params) {
                    Ok(b) => {
                        if let Err(e) = self.forward(req, &b, res).await {
                            res.render(e);
                        }
                    }
                    Err(e) => res.render(e),
                }
            }
            Err(e) => res.render(e),
        }

        ctrl.skip_rest();
    }
}

impl Handler {
    async fn forward(
        &self,
        req: &mut Request,
        body: &Array,
        res: &mut Response,
    ) -> Result<(), ServerError> {
        let response = self
            .client
            .execute(self.forward_request(req, body)?)
            .await?;

        debug!("Status code: {}", response.status());
        res.status_code(response.status());
        res.set_headers(response.headers().clone());
        res.stream(response.bytes_stream());
        Ok(())
    }

    fn forward_request(
        &self,
        req: &mut Request,
        body: &Array,
    ) -> Result<reqwest::Request, ServerError> {
        debug!("Forward request {}", body);
        Ok(self
            .client
            .post(self.json_streams_endpoint.clone())
            .header(AUTHORIZATION, req.header(AUTHORIZATION).unwrap_or(""))
            .json(&to_value(&Value::Array(body.clone())).ok_or(Internal)?)
            .build()?)
    }

    fn json_streams_body(
        &self,
        request_body: &Object,
        params: &PathParams,
    ) -> Result<Array, ServerError> {
        if let Some(a) = &self.aggregation {
            Ok(replace_array(a, &|v| {
                v.as_string()
                    .and_then(|s| Self::replace_value(&s, request_body, params))
            }))
        } else if let Some(e) = &self.environment {
            Self::json_streams_body_template(request_body, params, e)
        } else {
            Err(NoAggregation)
        }
    }

    fn json_streams_body_template(
        request_body: &Object,
        params: &PathParams,
        environment: &Environment<'static>,
    ) -> Result<Array, ServerError> {
        let context = Self::template_context(request_body, params);
        let rendered = environment.get_template("template")?.render(&context)?;

        debug!("Render context: {:?}", context);
        debug!("Rendered:\n{}", rendered);
        Self::parse_rendered(&rendered)
            .and_then(|v| v.as_array())
            .ok_or(NoAggregation)
    }

    fn parse_rendered(s: &str) -> Option<Value> {
        match Yaml::new().load_str(s) {
            Ok(v) => Some(from_yaml(&v)),
            Err(_) => Value::from_str(s).ok(),
        }
    }

    fn replace_value(s: &str, request_body: &Object, params: &PathParams) -> Option<Value> {
        if s.starts_with("$$") {
            Some(Value::String(
                s.strip_prefix("$$")
                    .and_then(|k| params.get(k))
                    .cloned()
                    .unwrap_or(s.to_string()),
            ))
        } else if s.starts_with("${") && s.ends_with("}") {
            Some(
                request_body
                    .get_p(&Self::to_pointer(&s[2..s.len() - 1]))
                    .unwrap_or_else(|| Value::String("".to_string())),
            )
        } else {
            None
        }
    }

    fn template_context(request_body: &Object, params: &PathParams) -> minijinja::Value {
        minijinja::Value::from(Serde(
            to_value(&Value::Object(
                params.iter().fold(request_body.clone(), |r, (k, v)| {
                    r.set(k, &Value::String(v.clone()))
                }),
            ))
            .unwrap_or_default(),
        ))
    }

    fn to_pointer(dot_path: &str) -> String {
        "/".to_string() + &dot_path.replace(".", "/")
    }
}

fn aggregation_as_array(value: &config::Value) -> Option<Array> {
    match value.kind {
        ValueKind::Array(_) => value
            .clone()
            .into_array()
            .ok()
            .and_then(|_| from_config(value))
            .and_then(|v| v.as_array()),
        _ => None,
    }
}

fn aggregation_as_template(
    value: &config::Value,
) -> Result<Option<Environment<'static>>, ServerError> {
    match value.kind.clone() {
        ValueKind::String(s) => Ok(Some(template(s)?)),
        _ => Ok(None),
    }
}

async fn body(req: &mut Request) -> Result<Object, ServerError> {
    let body = String::from_utf8(Vec::<u8>::from(
        req.payload_with_max_size(512 * 1024).await?.clone(),
    ))?;
    debug!("Request body: {0}", body);
    Value::from_str(&body)?
        .as_object()
        .ok_or_else(|| NotObject(body))
}

fn handler(
    value: &config::Map<String, config::Value>,
    context_path: &str,
    json_streams_endpoint: &Url,
    client: &Client,
) -> Result<Handler, ServerError> {
    let aggregation = value.get(AGGREGATION).ok_or(NoAggregation)?;
    let handler = Handler {
        aggregation: aggregation_as_array(aggregation),
        client: client.clone(),
        environment: aggregation_as_template(aggregation)?,
        json_streams_endpoint: json_streams_endpoint.clone(),
        path: value
            .get(PATH)
            .and_then(|v| v.clone().into_string().ok())
            .map(|p| context_path.to_string() + &p)
            .ok_or(NoPath)?,
    };

    if handler.aggregation.is_none() && handler.environment.is_none() {
        Err(NoAggregation)
    } else {
        Ok(handler)
    }
}

fn handlers(config: &Config, client: &Client) -> Result<Vector<Handler>, ServerError> {
    let context_path = config.get_string(CONTEXT_PATH).unwrap_or("".to_string());

    config
        .get_array(HANDLERS)?
        .iter()
        .filter_map(|i| i.clone().into_table().ok())
        .map(|i| {
            handler(
                &i,
                &context_path,
                &json_streams_endpoint(config, &i)?,
                client,
            )
        })
        .fold_result(Vector::new(), |v, h| v.push_back_self(h))
}

fn handler_router(handler: &Handler) -> Router {
    debug!("Deploying handler for path {}", handler.path);
    Router::with_path(handler.path.clone()).post(handler.clone())
}

fn json_streams_endpoint(
    config: &Config,
    handler_value: &config::Map<String, config::Value>,
) -> Result<Url, ServerError> {
    Ok(Url::parse(
        &(config.get_string(JSON_STREAMS_ENDPOINT)?
            + &handler_value
                .get(JSON_STREAMS_PATH)
                .and_then(|v| v.clone().into_string().ok())
                .ok_or(NoJsonStreamsPath)?),
    )?)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = Client::builder().gzip(true).tls_backend_rustls().build()?;
    let config = config_util::config()?;
    let server = Server::new(&config, "", Cli::parse().port.unwrap_or_default())?;

    info!("Version: {VERSION}");

    server
        .run(
            handlers(&config, &client)?
                .iter()
                .fold(Router::new(), |r, h| r.push(handler_router(h))),
        )
        .await?;

    Ok(())
}

fn template(s: String) -> Result<Environment<'static>, ServerError> {
    let mut env = Environment::new();

    env.add_template_owned("template", s)?;

    Ok(env)
}
