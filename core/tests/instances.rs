//! The instance selector: API requests naming a peer instance are forwarded
//! to it over HTTP.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::TestApp;
use logmaker_core::instances::{INSTANCE_HEADER, Instances};
use serde_json::{Value, json};

/// Serves `router` on a local port and returns its URL.
async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{address}")
}

/// Instance `a`, which forwards requests for `b` to a separate app over HTTP.
async fn pair() -> (TestApp, TestApp) {
    let b = TestApp::new();
    let b_url = serve(b.router()).await;
    let mut a = TestApp::new();
    let instances = Instances::new("a", &format!("a=http://a.invalid:1,b={b_url}")).unwrap();
    a.wrap_router(|router| instances.serve(router));
    (a, b)
}

async fn send(app: &TestApp, instance: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header(INSTANCE_HEADER, instance)
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    app.send(request).await
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn lists_instances() {
    let (a, _b) = pair().await;
    assert_eq!(
        a.get("/api/v1/instances").await,
        json!({"current": "a", "instances": ["a", "b"]})
    );
}

#[tokio::test]
async fn forwards_requests_for_peers() {
    let (a, b) = pair().await;
    let maker = json!({"name": "m", "type": "Fixed", "args": {"value": "x"}});
    let (status, body) = send(&a, "b", Method::POST, "/api/v1/maker", Some(maker)).await;
    assert_eq!((status, &body["type"]), (StatusCode::OK, &json!("SUCCESS")), "{body}");

    assert_eq!(names(&b.get("/api/v1/maker").await), ["m"]);
    assert!(names(&a.get("/api/v1/maker").await).is_empty());
    assert_eq!(names(&send(&a, "b", Method::GET, "/api/v1/maker", None).await.1), ["m"]);
    assert!(names(&send(&a, "a", Method::GET, "/api/v1/maker", None).await.1).is_empty());

    // Errors from the peer come back unchanged.
    let through_a = send(&a, "b", Method::DELETE, "/api/v1/maker/nope", None).await;
    assert_eq!(through_a, b.delete("/api/v1/maker/nope").await);
    assert_eq!(through_a.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn forwards_uploads() {
    let (a, b) = pair().await;
    let boundary = "logmaker-test-boundary";
    let mut body =
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"senders.json\"\r\n\r\n")
            .into_bytes();
    body.extend_from_slice(br#"[{"name":"s","type":"Capture"}]"#);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/sender:import-file")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .header(INSTANCE_HEADER, "b")
        .body(Body::from(body))
        .unwrap();
    let (_, result) = a.send(request).await;
    assert_eq!(result[0]["type"], "SUCCESS", "{result}");
    assert_eq!(names(&b.get("/api/v1/sender").await), ["s"]);
    assert!(names(&a.get("/api/v1/sender").await).is_empty());
}

#[tokio::test]
async fn reports_unknown_and_unreachable_instances() {
    let (a, _b) = pair().await;
    let (status, body) = send(&a, "zzz", Method::GET, "/api/v1/maker", None).await;
    assert_eq!(
        (status, &body["message"]),
        (StatusCode::BAD_REQUEST, &json!("Unknown instance zzz"))
    );

    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut c = TestApp::new();
    let instances = Instances::new("c", &format!("down=http://{closed}")).unwrap();
    c.wrap_router(|router| instances.serve(router));
    let (status, body) = send(&c, "down", Method::GET, "/api/v1/maker", None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .starts_with("Instance down is unreachable"),
        "{body}"
    );
}

#[tokio::test]
async fn serves_health_and_ui_locally() {
    let (a, _b) = pair().await;
    let (status, body) = send(&a, "b", Method::GET, "/actuator/health", None).await;
    assert_eq!((status, body), (StatusCode::OK, json!({"status": "UP"})));
}
