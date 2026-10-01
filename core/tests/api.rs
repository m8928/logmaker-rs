//! End-to-end tests through the HTTP router, with a test plugin providing a
//! `Fixed` maker and a `Capture` sender.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::{TestApp, eventually};
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(5);

async fn fixed(app: &TestApp, name: &str, value: &str) {
    app.ok(
        "/api/v1/maker",
        json!({"name": name, "type": "Fixed", "args": {"value": value}}),
    )
    .await;
}

async fn capture(app: &TestApp, name: &str) {
    app.ok("/api/v1/sender", json!({"name": name, "type": "Capture"})).await;
}

#[tokio::test]
async fn maker_lifecycle_and_validation() {
    let app = TestApp::new();
    fixed(&app, "m", "one").await;

    let maker = app.find("/api/v1/maker", "m").await;
    assert_eq!(maker["sample"], "one");
    assert_eq!(maker["ref"], 0);

    let (status, body) = app
        .post("/api/v1/maker", json!({"type": "Fixed", "args": {"value": "x"}}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body,
        json!({"type": "ERROR", "message": "Validation failed", "data": {"name": "Name field value is required"}, "notification": true})
    );

    let (_, body) = app
        .post(
            "/api/v1/maker",
            json!({"name": "m", "type": "Fixed", "args": {"value": "x"}}),
        )
        .await;
    assert_eq!(body["message"], "m is the maker name already in use");
    let (_, body) = app.post("/api/v1/maker", json!({"name": "n", "type": "Nope"})).await;
    assert_eq!(body["message"], "Nope is an unavailable maker type");
    let (_, body) = app
        .post(
            "/api/v1/maker",
            json!({"name": "n", "type": "Fixed", "args": {"value": 1}}),
        )
        .await;
    assert_eq!(body["message"], "Invalid maker argument ({\"value\":1})");

    let (status, _) = app.put("/api/v1/maker/m", json!({"args": {"value": "two"}})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.find("/api/v1/maker", "m").await["sample"], "two");
    let (_, body) = app.put("/api/v1/maker/missing", json!({"args": {}})).await;
    assert_eq!(body["message"], "Update maker failed");

    let (status, body) = app.delete("/api/v1/maker/m").await;
    assert_eq!(
        (status, &body["message"]),
        (StatusCode::OK, &json!("Successfully deleted maker"))
    );
    let (_, body) = app.delete("/api/v1/maker/m").await;
    assert_eq!(body["message"], "Maker does not exist");
}

#[tokio::test]
async fn plugin_types_describe_arguments() {
    let app = TestApp::new();
    let makers = app.get("/api/v1/plugin/maker").await;
    let syslog = app
        .get("/api/v1/plugin/sender")
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "Syslog")
        .cloned()
        .unwrap();
    assert!(makers.as_array().unwrap().iter().any(|t| t["type"] == "Regex"));
    assert_eq!(syslog["args"]["port"]["type"], "integer");
    assert_eq!(syslog["args"]["host"]["type"], "list");
    assert_eq!(syslog["args"]["facility"]["required"], false);

    let plugins = app.get("/api/v1/plugin").await;
    assert_eq!(plugins[0]["name"], "default-plugin");
    assert_eq!(plugins[0]["filename"], Value::Null);
}

#[tokio::test]
async fn log_generates_at_rate_and_pauses() {
    let app = TestApp::new();
    fixed(&app, "word", "hi").await;
    capture(&app, "out").await;
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<word> <word>!", "eps": 20, "sender": ["out"]}),
    )
    .await;

    assert!(eventually(WAIT, || app.captured.lines("out").len() >= 20).await);
    assert_eq!(app.captured.lines("out")[0], "hi hi!");
    let log = app.find("/api/v1/log", "l").await;
    assert_eq!(log["sample"], "hi hi!");
    assert_eq!(app.find("/api/v1/maker", "word").await["ref"], 1);

    let (_, body) = app.post("/api/v1/log/l:stop", json!(null)).await;
    assert_eq!(body["message"], "Log stopped");
    let stopped = app.captured.lines("out").len();
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert_eq!(app.captured.lines("out").len(), stopped);
    let log = app.find("/api/v1/log", "l").await;
    assert_eq!(
        (log["paused"].clone(), log["currentEps"].clone()),
        (json!(true), json!(0))
    );

    app.ok("/api/v1/log/l:start", json!(null)).await;
    assert!(eventually(WAIT, || app.captured.lines("out").len() > stopped).await);

    let (status, _) = app.post("/api/v1/log/l:restart", json!(null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn low_rates_accumulate_across_seconds() {
    let app = TestApp::new();
    fixed(&app, "w", "x").await;
    capture(&app, "out").await;
    // 120 per minute = 2 per second.
    app.ok(
        "/api/v1/log",
        json!({"name": "slow", "format": "<w>", "eps": 120, "epsTimeUnit": "min", "sender": ["out"]}),
    )
    .await;
    assert!(eventually(WAIT, || app.captured.lines("out").len() >= 2).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(app.captured.lines("out").len() <= 6);
}

#[tokio::test]
async fn sender_limit_caps_deliveries() {
    let app = TestApp::new();
    fixed(&app, "w", "x").await;
    app.ok(
        "/api/v1/sender",
        json!({"name": "capped", "type": "Capture", "limit": 3}),
    )
    .await;
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<w>", "eps": 100, "sender": ["capped"]}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert!(app.find("/api/v1/log", "l").await["count"].as_u64().unwrap() >= 100);
    assert_eq!(app.captured.lines("capped").len(), 3);
    let sender = app.find("/api/v1/sender", "capped").await;
    assert_eq!((sender["count"].clone(), sender["limit"].clone()), (json!(3), json!(3)));

    // Raising the limit through an update resumes delivery.
    app.put("/api/v1/sender/capped", json!({"args": {}, "limit": 0})).await;
    assert!(eventually(WAIT, || app.captured.lines("capped").len() > 3).await);
}

#[tokio::test]
async fn log_update_applies_and_rejects_unknown_makers() {
    let app = TestApp::new();
    fixed(&app, "a", "A").await;
    fixed(&app, "b", "B").await;
    capture(&app, "out").await;
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<a>", "eps": 50, "sender": ["out"]}),
    )
    .await;
    assert!(eventually(WAIT, || !app.captured.lines("out").is_empty()).await);

    let (_, body) = app
        .put(
            "/api/v1/log/l",
            json!({"format": "<missing>", "eps": 50, "sender": ["out"]}),
        )
        .await;
    assert_eq!(body["message"], "Update log failed (maker not found. [missing])");
    assert_eq!(app.find("/api/v1/log", "l").await["format"], "<a>");

    let (status, _) = app
        .put("/api/v1/log/l", json!({"format": "<b>", "eps": 50, "sender": ["out"]}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(eventually(WAIT, || app.captured.lines("out").last().is_some_and(|l| l == "B")).await);
    assert_eq!(app.find("/api/v1/maker", "a").await["ref"], 0);

    let (_, body) = app.delete("/api/v1/maker/b").await;
    assert_eq!(body["message"], "Maker is currently in use");
    app.delete("/api/v1/log/l").await;
    let (_, body) = app.delete("/api/v1/maker/b").await;
    assert_eq!(body["message"], "Successfully deleted maker");
}

#[tokio::test]
async fn log_validation_and_preview() {
    let app = TestApp::new();
    fixed(&app, "a", "A").await;
    let (status, body) = app.post("/api/v1/log", json!({"name": "l"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"], json!({"format": "Format field value is required"}));
    let (_, body) = app
        .post("/api/v1/log", json!({"name": "l", "format": "<a>", "sender": ["none"]}))
        .await;
    assert_eq!(body["message"], "Invalid log argument (<a>): sender not found. [none]");

    let (status, body) = app
        .post("/api/v1/log:preview", json!({"format": "<a> <zzz> <134>"}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (body["message"].clone(), body["notification"].clone()),
        (json!("A <zzz> <134>"), json!(false))
    );
}

#[tokio::test]
async fn scenario_routes_steps_with_overrides_and_shared_variables() {
    let app = TestApp::new();
    fixed(&app, "value", "maker-value").await;
    fixed(&app, "sharedValue", "base-shared").await;
    fixed(&app, "shared-maker", "shared-value").await;
    fixed(&app, "src-maker", "10.10.10.10").await;
    capture(&app, "first").await;
    capture(&app, "second").await;
    app.ok(
        "/api/v1/log",
        json!({"name": "login", "format": "<value>-<sharedValue>", "eps": 0, "paused": true}),
    )
    .await;

    app.ok(
        "/api/v1/scenario",
        json!({
            "name": "flow",
            "sharedVariables": {"sharedValue": "shared-maker", "src_ip": "src-maker"},
            "steps": [
                {"logName": "login", "senders": ["first"], "overrides": {"value": "step-value", "sharedValue": "ignored"}},
                {"logName": "login", "repeat": 2, "senders": ["second", "second"], "overrides": {"value": "${src_ip}"}},
                {"logName": "login", "senders": []}
            ],
            "loopCount": 1
        }),
    )
    .await;
    app.ok("/api/v1/scenario/flow:start", json!(null)).await;
    wait_until_finished(&app, "flow").await;

    assert_eq!(app.captured.lines("first"), ["step-value-shared-value"]);
    assert_eq!(
        app.captured.lines("second"),
        ["10.10.10.10-shared-value", "10.10.10.10-shared-value"]
    );
    let scenario = app.find("/api/v1/scenario", "flow").await;
    assert_eq!(scenario["totalSteps"], 3);
    assert_eq!(scenario["description"], Value::Null);
}

async fn wait_until_finished(app: &TestApp, scenario: &str) {
    let deadline = std::time::Instant::now() + WAIT;
    while app.find("/api/v1/scenario", scenario).await["status"] == true {
        assert!(
            std::time::Instant::now() < deadline,
            "scenario {scenario} did not finish"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn scenario_validation_and_lifecycle() {
    let app = TestApp::new();
    fixed(&app, "a", "A").await;
    capture(&app, "out").await;
    app.ok("/api/v1/log", json!({"name": "l", "format": "<a>", "paused": true}))
        .await;

    let cases = [
        (
            json!({"name": "s", "steps": [{"logName": ""}]}),
            "Scenario step 1 log is required",
        ),
        (
            json!({"name": "s", "steps": [{"logName": "nope"}]}),
            "Scenario references unknown log: nope",
        ),
        (
            json!({"name": "s", "steps": [{"logName": "l", "senders": ["x"]}]}),
            "Scenario references unknown sender: x",
        ),
        (
            json!({"name": "s", "sharedVariables": {"v": "zz"}}),
            "Scenario references unknown maker: zz",
        ),
    ];
    for (body, message) in cases {
        let (_, result) = app.post("/api/v1/scenario", body).await;
        assert_eq!(result["message"], message);
    }
    let (status, result) = app.post("/api/v1/scenario", json!({"name": " "})).await;
    assert_eq!(
        (status, &result["message"]),
        (StatusCode::BAD_REQUEST, &json!("Validation failed"))
    );

    // Infinite loop with short intervals.
    let scenario =
        json!({"name": "s", "steps": [{"logName": "l", "senders": ["out"]}], "intervalMinMs": 10, "intervalMaxMs": 20});
    app.ok("/api/v1/scenario", scenario.clone()).await;
    let (_, result) = app.post("/api/v1/scenario", scenario).await;
    assert_eq!(result["message"], "s is the scenario name already in use");

    app.ok("/api/v1/scenario/s:start", json!(null)).await;
    let (_, result) = app.post("/api/v1/scenario/s:start", json!(null)).await;
    assert_eq!(result["message"], "Scenario is already running");
    assert!(eventually(WAIT, || app.captured.lines("out").len() >= 3).await);
    let running = app.find("/api/v1/scenario", "s").await;
    assert_eq!(running["status"], true);
    assert!(running["currentLoop"].as_i64().unwrap() >= 1);
    assert_eq!(running["stepCounts"].as_array().unwrap().len(), 1);

    let (_, result) = app.delete("/api/v1/maker/a").await;
    assert_eq!(result["message"], "Maker is currently in use");
    let (_, result) = app.delete("/api/v1/sender/out").await;
    assert_eq!(result["message"], "Sender is currently in use");

    // Updating a running scenario restarts it with the new definition.
    let (status, result) = app
        .put("/api/v1/scenario/s", json!({"steps": [{"logName": "l", "senders": ["out"], "overrides": {"a": "B"}}], "intervalMinMs": 10, "intervalMaxMs": 20}))
        .await;
    assert_eq!(
        (status, &result["message"]),
        (StatusCode::OK, &json!("Successfully updated scenario"))
    );
    assert!(eventually(WAIT, || app.captured.lines("out").last().is_some_and(|l| l == "B")).await);

    app.ok("/api/v1/scenario/s:stop", json!(null)).await;
    let (_, result) = app.post("/api/v1/scenario/s:stop", json!(null)).await;
    assert_eq!(result["message"], "Scenario is not running");
    let stopped = app.find("/api/v1/scenario", "s").await;
    assert_eq!(
        (stopped["status"].clone(), stopped["stepCounts"].clone()),
        (json!(false), Value::Null)
    );

    let (_, result) = app.delete("/api/v1/sender/out").await;
    assert_eq!(result["message"], "Successfully deleted sender");
    let (_, result) = app.delete("/api/v1/scenario/s").await;
    assert_eq!(result["message"], "Successfully deleted scenario");
}

#[tokio::test]
async fn imports_report_per_item_results() {
    let app = TestApp::new();
    let (status, body) = app
        .post("/api/v1/maker:import", json!([{"name": "a", "type": "Fixed", "args": {"value": "1"}}, {"name": "a", "type": "Fixed", "args": {"value": "2"}}]))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["type"], "SUCCESS");
    assert_eq!(body[1]["message"], "a is the maker name already in use");

    let file = br#"[{"name":"s","type":"Capture"},{"name":"bad","type":"Nope"}]"#;
    let (_, body) = app.upload("/api/v1/sender:import-file", "senders.json", file).await;
    assert_eq!(body[0]["type"], "SUCCESS");
    assert_eq!(body[1]["message"], "Nope is an unavailable sender type");

    let (_, body) = app.upload("/api/v1/log:import-file", "logs.json", b"not json").await;
    assert_eq!(
        body,
        json!([{"type": "ERROR", "message": "Log file import failed", "notification": true}])
    );
}

#[tokio::test]
async fn definitions_survive_restart() {
    let mut app = TestApp::new();
    fixed(&app, "a", "A").await;
    app.ok("/api/v1/sender", json!({"name": "out", "type": "Capture", "limit": 7}))
        .await;
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<a>", "eps": 5, "sender": ["out"]}),
    )
    .await;
    app.ok("/api/v1/log/l:stop", json!(null)).await;
    app.ok(
        "/api/v1/scenario",
        json!({"name": "s", "description": "demo", "steps": [{"logName": "l", "senders": ["out"]}]}),
    )
    .await;
    let reg_time = app.find("/api/v1/maker", "a").await["regTime"].clone();

    app.restart();
    assert_eq!(app.find("/api/v1/maker", "a").await["regTime"], reg_time);
    assert_eq!(app.find("/api/v1/sender", "out").await["limit"], 7);
    let log = app.find("/api/v1/log", "l").await;
    assert_eq!((log["paused"].clone(), log["eps"].clone()), (json!(true), json!(5)));
    assert_eq!(app.find("/api/v1/scenario", "s").await["description"], "demo");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(app.captured.lines("out").is_empty(), "paused logs stay paused");
}

#[tokio::test]
async fn unknown_routes_and_bad_bodies() {
    let app = TestApp::new();
    let not_found = json!({"type": "ERROR", "message": "Not found", "notification": true});
    let (status, body) = app.request(axum::http::Method::GET, "/api/v1/nothing", None).await;
    assert_eq!((status, body), (StatusCode::NOT_FOUND, not_found.clone()));
    let (status, body) = app.request(axum::http::Method::GET, "/missing.js", None).await;
    assert_eq!((status, body), (StatusCode::NOT_FOUND, not_found));

    let request = axum::http::Request::post("/api/v1/maker")
        .body(axum::body::Body::from("{oops"))
        .unwrap();
    let (status, body) = app.send(request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().starts_with("Invalid request body"));

    let (status, body) = app.request(axum::http::Method::GET, "/actuator/health", None).await;
    assert_eq!((status, body), (StatusCode::OK, json!({"status": "UP"})));

    let dashboard = app.get("/api/v1/dashboard").await;
    assert_eq!(dashboard["plugin"], 2);
    assert!(dashboard["version"].is_string());
}

#[tokio::test]
async fn scenario_number_fields_accept_null() {
    let app = TestApp::new();
    fixed(&app, "a", "A").await;
    app.ok("/api/v1/log", json!({"name": "l", "format": "<a>", "paused": true}))
        .await;
    app.ok(
        "/api/v1/scenario",
        json!({"name": "s", "intervalMinMs": null, "intervalMaxMs": null, "loopCount": null, "steps": [{"logName": "l", "repeat": null, "delayMinMs": null}]}),
    )
    .await;
    let scenario = app.find("/api/v1/scenario", "s").await;
    assert_eq!(
        (
            scenario["intervalMinMs"].clone(),
            scenario["steps"][0]["repeat"].clone()
        ),
        (json!(0), json!(0))
    );
    app.ok("/api/v1/scenario", json!({"name": "defaults"})).await;
    let defaults = app.find("/api/v1/scenario", "defaults").await;
    assert_eq!(
        (defaults["intervalMinMs"].clone(), defaults["intervalMaxMs"].clone()),
        (json!(1000), json!(5000))
    );
}

#[tokio::test]
async fn new_names_must_be_url_safe() {
    let app = TestApp::new();
    let characters = "Name may only contain letters, digits, '_' and '-' (at most 64 characters)";
    for name in ["a b", "a#b", "a?b", "a/b", "..", "한글", " pad", &"x".repeat(65)] {
        let cases = [
            ("/api/v1/maker", json!({"name": name, "type": "IP"})),
            ("/api/v1/sender", json!({"name": name, "type": "Capture"})),
            ("/api/v1/log", json!({"name": name, "format": "x"})),
            ("/api/v1/scenario", json!({"name": name})),
        ];
        for (path, body) in cases {
            let (status, result) = app.post(path, body).await;
            assert_eq!(
                (status, &result["data"]["name"]),
                (StatusCode::BAD_REQUEST, &json!(characters)),
                "{path} {name:?}"
            );
        }
    }
    let (_, result) = app
        .post("/api/v1/maker", json!({"name": "404-code", "type": "IP"}))
        .await;
    assert_eq!(
        result["data"]["name"],
        "Maker name must start with a letter or '_' to be usable as <name> in log formats"
    );
    app.ok("/api/v1/sender", json!({"name": "404-code", "type": "Capture"}))
        .await;
    app.ok("/api/v1/maker", json!({"name": "Src_IP-2", "type": "IP"})).await;
    let (_, result) = app
        .post("/api/v1/maker:import", json!([{"name": "bad name", "type": "IP"}]))
        .await;
    assert_eq!(result[0]["data"]["name"], characters);
}

#[tokio::test]
async fn stored_names_outside_the_rules_stay_manageable() {
    let mut app = TestApp::new();
    let data = app.dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let maker = json!([{"name": "a", "type": "Fixed", "args": {"value": "A"}}, {"name": "a#b", "type": "Fixed", "args": {"value": "B"}}]);
    std::fs::write(data.join("makers.json"), maker.to_string()).unwrap();
    let logs =
        json!([{"name": "a", "format": "<a>", "paused": true}, {"name": "로그 a#b", "format": "<a>", "paused": true}]);
    std::fs::write(data.join("logs.json"), logs.to_string()).unwrap();
    app.restart();

    // Paths as the UI builds them: encodeURIComponent(name).
    let (status, _) = app.put("/api/v1/maker/a%23b", json!({"args": {"value": "C"}})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.find("/api/v1/maker", "a#b").await["sample"], "C");
    let log = "%EB%A1%9C%EA%B7%B8%20a%23b";
    app.ok(&format!("/api/v1/log/{log}:start"), json!(null)).await;
    let (status, result) = app.delete(&format!("/api/v1/log/{log}")).await;
    assert_eq!(
        (status, &result["message"]),
        (StatusCode::OK, &json!("Successfully deleted log"))
    );
    assert_eq!(
        app.get("/api/v1/log").await.as_array().unwrap().len(),
        1,
        "log a is untouched"
    );

    let request = axum::http::Request::delete("/api/v1/maker/%FF")
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, result) = app.send(request).await;
    assert_eq!(
        (status, result["type"].clone()),
        (StatusCode::BAD_REQUEST, json!("ERROR"))
    );
}
