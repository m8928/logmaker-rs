//! Switching between LogMaker instances from one web UI.
//!
//! An instance can list peers (`--peers name=http://host:port,...`). The UI
//! sends `X-LogMaker-Instance: <name>` with its API requests, and requests for
//! a peer are forwarded to it, so one address (for example one Kubernetes
//! NodePort) serves all instances.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::uri::{Authority, Scheme};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use indexmap::IndexMap;
use serde::Serialize;

use crate::api_result::ApiResult;
use crate::names;

/// Request header naming the instance an API request is for.
pub const INSTANCE_HEADER: &str = "x-logmaker-instance";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);

/// Headers that apply to one connection and are not forwarded.
const HOP_BY_HOP: [&str; 8] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

pub struct Instances {
    name: String,
    /// Instance name → address, in the configured order. Contains this
    /// instance (without an address) even when it is not configured.
    instances: IndexMap<String, Option<(Scheme, Authority)>>,
    client: Client<HttpConnector, Body>,
}

#[derive(Serialize)]
struct InstancesDto<'a> {
    current: &'a str,
    instances: Vec<&'a str>,
}

impl Instances {
    /// `peers` is a comma-separated list of `name=http://host:port` entries.
    /// An entry for `name` itself is allowed (so all instances can share one
    /// list) and refers to this instance.
    pub fn new(name: &str, peers: &str) -> anyhow::Result<Self> {
        if names::check_name(name).is_some() {
            bail!("invalid instance name {name:?}: use letters, digits, '_' and '-' (at most 64)");
        }
        let mut instances = IndexMap::new();
        for entry in peers.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (peer, url) = entry
                .split_once('=')
                .with_context(|| format!("invalid peer {entry:?}: expected name=http://host:port"))?;
            let (peer, url) = (peer.trim(), url.trim());
            if names::check_name(peer).is_some() {
                bail!("invalid peer name {peer:?}: use letters, digits, '_' and '-' (at most 64)");
            }
            let address = if peer == name { None } else { Some(parse_address(url)?) };
            if instances.insert(peer.to_owned(), address).is_some() {
                bail!("peer {peer} is listed twice");
            }
        }
        if !instances.contains_key(name) {
            instances.shift_insert(0, name.to_owned(), None);
        }
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(CONNECT_TIMEOUT));
        Ok(Self {
            name: name.to_owned(),
            instances,
            client: Client::builder(TokioExecutor::new()).build(connector),
        })
    }

    /// Names of all instances, in the configured order.
    pub fn names(&self) -> Vec<&str> {
        self.instances.keys().map(String::as_str).collect()
    }

    /// Adds the instance selector to `router`: `GET /api/v1/instances` and
    /// forwarding of API requests for peers.
    pub fn serve(self, router: Router) -> Router {
        router.layer(middleware::from_fn_with_state(Arc::new(self), route))
    }

    async fn forward(&self, peer: &str, scheme: &Scheme, authority: &Authority, request: Request) -> Response {
        let (mut parts, body) = request.into_parts();
        let path = parts.uri.path_and_query().map_or("/", |p| p.as_str());
        parts.uri = match Uri::builder()
            .scheme(scheme.clone())
            .authority(authority.clone())
            .path_and_query(path)
            .build()
        {
            Ok(uri) => uri,
            Err(e) => return peer_unreachable(peer, &e.to_string()),
        };
        strip_hop_by_hop(&mut parts.headers);
        parts.headers.remove(header::HOST);
        parts.headers.remove(INSTANCE_HEADER);
        let request = Request::from_parts(parts, body);
        match tokio::time::timeout(RESPONSE_TIMEOUT, self.client.request(request)).await {
            Ok(Ok(response)) => {
                let mut response = response.map(Body::new);
                strip_hop_by_hop(response.headers_mut());
                response
            }
            Ok(Err(e)) => peer_unreachable(peer, &cause(&e)),
            Err(_) => peer_unreachable(peer, "no response in time"),
        }
    }
}

fn parse_address(url: &str) -> anyhow::Result<(Scheme, Authority)> {
    let uri: Uri = url
        .parse()
        .with_context(|| format!("invalid peer address {url:?}: expected http://host:port"))?;
    if uri.scheme() != Some(&Scheme::HTTP) {
        bail!("invalid peer address {url:?}: only http:// is supported");
    }
    if uri.path_and_query().is_some_and(|p| p.as_str() != "/") {
        bail!("invalid peer address {url:?}: expected http://host:port without a path");
    }
    let authority = uri
        .authority()
        .with_context(|| format!("invalid peer address {url:?}: host is missing"))?;
    Ok((Scheme::HTTP, authority.clone()))
}

fn strip_hop_by_hop(headers: &mut HeaderMap) {
    for name in HOP_BY_HOP {
        headers.remove(name);
    }
}

/// The causes of a client error (e.g. "tcp connect error: Connection
/// refused"), without the generic outer message.
fn cause(error: &dyn std::error::Error) -> String {
    let mut causes = Vec::new();
    let mut source = error.source();
    while let Some(e) = source {
        causes.push(e.to_string());
        source = e.source();
    }
    if causes.is_empty() {
        error.to_string()
    } else {
        causes.join(": ")
    }
}

fn peer_unreachable(peer: &str, reason: &str) -> Response {
    tracing::warn!("cannot forward request to instance {peer}: {reason}");
    let result = ApiResult::error(format!("Instance {peer} is unreachable ({reason})"));
    (StatusCode::BAD_GATEWAY, Json(result)).into_response()
}

async fn route(State(instances): State<Arc<Instances>>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path == "/api/v1/instances" {
        let dto = InstancesDto {
            current: &instances.name,
            instances: instances.names(),
        };
        return Json(dto).into_response();
    }
    if !path.starts_with("/api/") {
        return next.run(request).await;
    }
    let Some(target) = request.headers().get(INSTANCE_HEADER) else {
        return next.run(request).await;
    };
    let Ok(target) = target.to_str().map(str::to_owned) else {
        return ApiResult::error("Invalid instance header").into_response();
    };
    match instances.instances.get(&target) {
        None => ApiResult::error(format!("Unknown instance {target}")).into_response(),
        Some(None) => next.run(request).await,
        Some(Some((scheme, authority))) => instances.forward(&target, scheme, authority, request).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_peers() {
        let instances = Instances::new("b", " a=http://a:19999, b=http://b:19999 ,c=http://10.0.0.3:31000/,").unwrap();
        assert_eq!(instances.names(), ["a", "b", "c"]);
        assert!(instances.instances["b"].is_none(), "own entry is local");
        let (_, authority) = instances.instances["c"].as_ref().unwrap();
        assert_eq!(authority.as_str(), "10.0.0.3:31000");

        let alone = Instances::new("local", "").unwrap();
        assert_eq!(alone.names(), ["local"]);
        assert_eq!(Instances::new("me", "x=http://x:1").unwrap().names(), ["me", "x"]);
    }

    #[test]
    fn rejects_invalid_peers() {
        for (name, peers, message) in [
            ("a b", "", "invalid instance name"),
            ("a", "b", "expected name=http://host:port"),
            ("a", "b=https://b:443", "only http://"),
            ("a", "b=http://b:1/api", "without a path"),
            ("a", "b=b:1", "only http://"),
            ("a", "b c=http://b:1", "invalid peer name"),
            ("a", "b=http://b:1,b=http://c:1", "listed twice"),
        ] {
            let error = Instances::new(name, peers).err().expect("rejected").to_string();
            assert!(error.contains(message), "{peers}: {error}");
        }
    }
}
