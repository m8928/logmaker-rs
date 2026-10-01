//! HTTP routes: the `/api/v1` REST API, health probes and the embedded web UI.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::multipart::MultipartRejection;
use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Multipart, Path, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::api_result::{ApiResult, INTERNAL_ERROR, not_found};
use crate::log::LogRequest;
use crate::maker::MakerRequest;
use crate::scenario::ScenarioConfig;
use crate::sender::SenderRequest;

/// Upload limit for plugin and import files.
const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

type AppStateRef = State<Arc<AppState>>;

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/dashboard", get(dashboard))
        .route("/maker", get(list_makers).post(create_maker))
        .route("/maker:import", post(import_makers))
        .route("/maker:import-file", post(import_maker_file))
        .route("/maker/{name}", put(update_maker).delete(delete_maker))
        .route("/sender", get(list_senders).post(create_sender))
        .route("/sender:import", post(import_senders))
        .route("/sender:import-file", post(import_sender_file))
        .route("/sender/{name}", put(update_sender).delete(delete_sender))
        .route("/log", get(list_logs).post(create_log))
        .route("/log:preview", post(preview_log))
        .route("/log:import", post(import_logs))
        .route("/log:import-file", post(import_log_file))
        .route("/log/{name}", put(update_log).delete(delete_log).post(log_action))
        .route("/plugin", get(list_plugins).post(upload_plugin))
        .route("/plugin/maker", get(maker_types))
        .route("/plugin/sender", get(sender_types))
        .route("/plugin/{name}", delete(delete_plugin))
        .route("/scenario", get(list_scenarios).post(create_scenario))
        .route(
            "/scenario/{name}",
            put(update_scenario).delete(delete_scenario).post(scenario_action),
        )
        .fallback(|| async { not_found() });

    Router::new()
        .nest("/api/v1", api)
        .route(
            "/actuator/health",
            get(|| async { Json(serde_json::json!({ "status": "UP" })) }),
        )
        .route("/actuator/info", get(|| async { Json(serde_json::json!({})) }))
        .fallback(static_files)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Runs blocking service code off the async executor.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ApiResult> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        tracing::error!("API handler failed: {e}");
        ApiResult::error(INTERNAL_ERROR)
    })
}

async fn json<T: Serialize + Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Response {
    match blocking(f).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn result(f: impl FnOnce() -> ApiResult + Send + 'static) -> ApiResult {
    blocking(f).await.unwrap_or_else(|error| error)
}

/// JSON body extractor that accepts any content type and reports malformed
/// bodies as an `ApiResult`.
struct ApiJson<T>(T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiResult;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|e| ApiResult::error(format!("Invalid request body ({})", e.body_text())))?;
        serde_json::from_slice(&bytes)
            .map(ApiJson)
            .map_err(|e| ApiResult::error(format!("Invalid request body ({e})")))
    }
}

/// The `{name}` path parameter; malformed values (e.g. invalid UTF-8) are
/// reported as an `ApiResult`.
struct ApiPath(String);

impl<S: Send + Sync> FromRequestParts<S> for ApiPath {
    type Rejection = ApiResult;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<String>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| ApiPath(value))
            .map_err(|e| ApiResult::error(e.body_text()))
    }
}

/// Reads the `file` part of a multipart upload.
async fn upload_file(multipart: Result<Multipart, MultipartRejection>) -> Result<(Option<String>, Bytes), ApiResult> {
    let mut multipart = multipart.map_err(|e| ApiResult::error(format!("Invalid upload ({})", e.body_text())))?;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiResult::error(format!("Invalid upload ({e})")))?
    {
        if field.name() == Some("file") {
            let file_name = field.file_name().map(str::to_owned);
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiResult::error(format!("Invalid upload ({e})")))?;
            return Ok((file_name, bytes));
        }
    }
    Err(ApiResult::error("Invalid upload (missing 'file' part)"))
}

/// Splits `name:start` / `name:stop` into the name and whether to pause.
fn action(target: &str) -> Option<(String, bool)> {
    if let Some(name) = target.strip_suffix(":start") {
        Some((name.to_owned(), false))
    } else {
        target.strip_suffix(":stop").map(|name| (name.to_owned(), true))
    }
}

async fn dashboard(State(state): AppStateRef) -> Response {
    json(move || state.dashboard()).await
}

// --- Makers -------------------------------------------------------------

async fn list_makers(State(state): AppStateRef) -> Response {
    json(move || state.makers.list()).await
}

async fn create_maker(State(state): AppStateRef, ApiJson(request): ApiJson<MakerRequest>) -> ApiResult {
    result(move || state.makers.create(request)).await
}

async fn import_makers(State(state): AppStateRef, ApiJson(requests): ApiJson<Vec<MakerRequest>>) -> Response {
    json(move || state.makers.import(requests)).await
}

async fn import_maker_file(State(state): AppStateRef, multipart: Result<Multipart, MultipartRejection>) -> Response {
    match upload_file(multipart).await {
        Ok((_, bytes)) => json(move || state.makers.import_file(&bytes)).await,
        Err(error) => error.into_response(),
    }
}

async fn update_maker(
    State(state): AppStateRef,
    ApiPath(name): ApiPath,
    ApiJson(request): ApiJson<MakerRequest>,
) -> ApiResult {
    result(move || state.makers.update(&name, request)).await
}

async fn delete_maker(State(state): AppStateRef, ApiPath(name): ApiPath) -> ApiResult {
    result(move || state.makers.delete(&name)).await
}

// --- Senders ------------------------------------------------------------

async fn list_senders(State(state): AppStateRef) -> Response {
    json(move || state.senders.list()).await
}

async fn create_sender(State(state): AppStateRef, ApiJson(request): ApiJson<SenderRequest>) -> ApiResult {
    result(move || state.senders.create(request)).await
}

async fn import_senders(State(state): AppStateRef, ApiJson(requests): ApiJson<Vec<SenderRequest>>) -> Response {
    json(move || state.senders.import(requests)).await
}

async fn import_sender_file(State(state): AppStateRef, multipart: Result<Multipart, MultipartRejection>) -> Response {
    match upload_file(multipart).await {
        Ok((_, bytes)) => json(move || state.senders.import_file(&bytes)).await,
        Err(error) => error.into_response(),
    }
}

async fn update_sender(
    State(state): AppStateRef,
    ApiPath(name): ApiPath,
    ApiJson(request): ApiJson<SenderRequest>,
) -> ApiResult {
    result(move || state.senders.update(&name, request)).await
}

async fn delete_sender(State(state): AppStateRef, ApiPath(name): ApiPath) -> ApiResult {
    result(move || state.senders.delete(&name)).await
}

// --- Logs ---------------------------------------------------------------

async fn list_logs(State(state): AppStateRef) -> Response {
    json(move || state.logs.list()).await
}

async fn create_log(State(state): AppStateRef, ApiJson(request): ApiJson<LogRequest>) -> ApiResult {
    result(move || state.logs.create(request)).await
}

async fn preview_log(State(state): AppStateRef, ApiJson(request): ApiJson<LogRequest>) -> ApiResult {
    result(move || state.logs.preview(&request.format.unwrap_or_default())).await
}

async fn import_logs(State(state): AppStateRef, ApiJson(requests): ApiJson<Vec<LogRequest>>) -> Response {
    json(move || state.logs.import(requests)).await
}

async fn import_log_file(State(state): AppStateRef, multipart: Result<Multipart, MultipartRejection>) -> Response {
    match upload_file(multipart).await {
        Ok((_, bytes)) => json(move || state.logs.import_file(&bytes)).await,
        Err(error) => error.into_response(),
    }
}

async fn update_log(
    State(state): AppStateRef,
    ApiPath(name): ApiPath,
    ApiJson(request): ApiJson<LogRequest>,
) -> ApiResult {
    result(move || state.logs.update(&name, request)).await
}

async fn delete_log(State(state): AppStateRef, ApiPath(name): ApiPath) -> ApiResult {
    result(move || state.logs.delete(&name)).await
}

/// `POST /log/{name}:start` and `POST /log/{name}:stop`.
async fn log_action(State(state): AppStateRef, ApiPath(target): ApiPath) -> Response {
    let Some((name, pause)) = action(&target) else {
        return not_found();
    };
    result(move || state.logs.set_paused(&name, pause))
        .await
        .into_response()
}

// --- Plugins ------------------------------------------------------------

async fn list_plugins(State(state): AppStateRef) -> Response {
    json(move || state.plugin_list()).await
}

async fn upload_plugin(State(state): AppStateRef, multipart: Result<Multipart, MultipartRejection>) -> ApiResult {
    match upload_file(multipart).await {
        Ok((file_name, bytes)) => result(move || state.upload_plugin(file_name.as_deref(), &bytes)).await,
        Err(error) => error,
    }
}

async fn delete_plugin(State(state): AppStateRef, ApiPath(name): ApiPath) -> ApiResult {
    result(move || state.delete_plugin(&name)).await
}

#[derive(Serialize)]
struct TypeDto {
    #[serde(rename = "type")]
    type_name: String,
    args: serde_json::Map<String, serde_json::Value>,
}

fn type_dtos(types: Vec<crate::plugin::TypeInfo>) -> Vec<TypeDto> {
    types
        .into_iter()
        .map(|t| TypeDto {
            type_name: t.type_name,
            args: t
                .args
                .into_iter()
                .map(|arg| {
                    let spec = serde_json::json!({
                        "type": arg.arg_type,
                        "description": arg.description,
                        "required": arg.required,
                    });
                    (arg.name, spec)
                })
                .collect(),
        })
        .collect()
}

async fn maker_types(State(state): AppStateRef) -> Response {
    json(move || type_dtos(state.plugins.maker_types())).await
}

async fn sender_types(State(state): AppStateRef) -> Response {
    json(move || type_dtos(state.plugins.sender_types())).await
}

// --- Scenarios ----------------------------------------------------------

async fn list_scenarios(State(state): AppStateRef) -> Response {
    json(move || state.scenarios.list()).await
}

async fn create_scenario(State(state): AppStateRef, ApiJson(config): ApiJson<ScenarioConfig>) -> ApiResult {
    result(move || state.scenarios.create(config)).await
}

async fn update_scenario(
    State(state): AppStateRef,
    ApiPath(name): ApiPath,
    ApiJson(config): ApiJson<ScenarioConfig>,
) -> ApiResult {
    result(move || state.scenarios.update(&name, config)).await
}

async fn delete_scenario(State(state): AppStateRef, ApiPath(name): ApiPath) -> ApiResult {
    result(move || state.scenarios.delete(&name)).await
}

/// `POST /scenario/{name}:start` and `POST /scenario/{name}:stop`.
async fn scenario_action(State(state): AppStateRef, ApiPath(target): ApiPath) -> Response {
    let Some((name, stop)) = action(&target) else {
        return not_found();
    };
    result(move || {
        if stop {
            state.scenarios.stop(&name)
        } else {
            state.scenarios.start(&name)
        }
    })
    .await
    .into_response()
}

// --- Web UI -------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "static/"]
#[exclude = ".gitkeep"]
struct Assets;

const ASSET_EXTENSIONS: [&str; 16] = [
    ".js", ".css", ".map", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".ico", ".svg", ".woff", ".woff2",
    ".ttf", ".eot", ".wasm",
];

fn asset_response(path: &str, asset: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = Response::new(Body::from(asset.data.into_owned()));
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// Serves UI files; unknown non-asset paths get `index.html` so client-side
/// routes work on reload.
async fn static_files(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") {
        return not_found();
    }
    let file = match path.trim_start_matches('/') {
        "" => "index.html",
        file => file,
    };
    if let Some(asset) = Assets::get(file) {
        return asset_response(file, asset);
    }
    let lower = path.to_ascii_lowercase();
    if file == "index.html" || ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return not_found();
    }
    match Assets::get("index.html") {
        Some(index) => asset_response("index.html", index),
        None => (
            StatusCode::NOT_FOUND,
            "LogMaker UI is not built (run `npm run build` in ui/)",
        )
            .into_response(),
    }
}
