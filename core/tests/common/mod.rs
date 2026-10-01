#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use logmaker_core::{AppState, Config, router};
use logmaker_plugin_api::{
    ArgSpec, ArgType, Args, Maker, MakerFactory, PluginDefinition, PluginError, PluginInfo, Sender, SenderFactory,
    arg_str,
};
use parking_lot::Mutex;
use serde_json::Value;
use tower::ServiceExt;

/// Lines received by `Capture` senders, keyed by sender name.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<(String, String)>>>);

impl Captured {
    pub fn lines(&self, sender: &str) -> Vec<String> {
        self.0
            .lock()
            .iter()
            .filter(|(s, _)| s == sender)
            .map(|(_, l)| l.clone())
            .collect()
    }
}

struct FixedFactory;

struct Fixed(String);

impl MakerFactory for FixedFactory {
    fn type_name(&self) -> &str {
        "Fixed"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::required("value", ArgType::String, "")]
    }

    fn create(&self, _name: &str, args: &Args) -> Result<Box<dyn Maker>, PluginError> {
        Ok(Box::new(Fixed(arg_str(args, "value").unwrap_or_default().to_owned())))
    }
}

impl Maker for Fixed {
    fn get_data(&self) -> String {
        self.0.clone()
    }
}

struct CaptureFactory(Captured);

struct Capture {
    name: String,
    captured: Captured,
}

impl SenderFactory for CaptureFactory {
    fn type_name(&self) -> &str {
        "Capture"
    }

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn create(&self, name: &str, _args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        Ok(Box::new(Capture {
            name: name.to_owned(),
            captured: self.0.clone(),
        }))
    }
}

impl Sender for Capture {
    fn send(&self, data: &str) -> Result<(), PluginError> {
        self.captured.0.lock().push((self.name.clone(), data.to_owned()));
        Ok(())
    }
}

pub struct TestApp {
    pub state: Arc<AppState>,
    router: Router,
    pub captured: Captured,
    pub dir: tempfile::TempDir,
}

fn test_plugin(captured: &Captured) -> PluginDefinition {
    PluginDefinition {
        info: PluginInfo {
            id: "test-plugin".into(),
            version: "1".into(),
            provider: "tests".into(),
        },
        makers: vec![Box::new(FixedFactory)],
        senders: vec![Box::new(CaptureFactory(captured.clone()))],
    }
}

fn start(dir: &Path, captured: &Captured) -> Arc<AppState> {
    let config = Config {
        data_root: dir.join("data"),
        plugin_root: dir.join("plugins"),
    };
    AppState::start_with_plugins(&config, vec![test_plugin(captured)]).expect("app starts")
}

impl TestApp {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let captured = Captured::default();
        let state = start(dir.path(), &captured);
        Self {
            router: router(Arc::clone(&state)),
            state,
            captured,
            dir,
        }
    }

    /// Shuts the app down and starts a new one on the same directories.
    pub fn restart(&mut self) {
        self.state.shutdown();
        self.captured = Captured::default();
        self.state = start(self.dir.path(), &self.captured);
        self.router = router(Arc::clone(&self.state));
    }

    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
        self.send(request).await
    }

    pub async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value =
            serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
        (status, value)
    }

    pub async fn get(&self, path: &str) -> Value {
        let (status, value) = self.request(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {value}");
        value
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::POST, path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::PUT, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> (StatusCode, Value) {
        self.request(Method::DELETE, path, None).await
    }

    /// POST that must succeed with `type: SUCCESS`.
    pub async fn ok(&self, path: &str, body: Value) {
        let (status, value) = self.post(path, body).await;
        assert_eq!(
            (status, &value["type"]),
            (StatusCode::OK, &Value::from("SUCCESS")),
            "POST {path}: {value}"
        );
    }

    pub async fn upload(&self, path: &str, file_name: &str, content: &[u8]) -> (StatusCode, Value) {
        let boundary = "logmaker-test-boundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("content-type", format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .unwrap();
        self.send(request).await
    }

    pub async fn find(&self, list_path: &str, name: &str) -> Value {
        let list = self.get(list_path).await;
        list.as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} not in {list_path}: {list}"))
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.state.shutdown();
    }
}

/// Polls `condition` until it holds or `timeout` passes.
pub async fn eventually(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    condition()
}
