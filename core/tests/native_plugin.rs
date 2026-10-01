//! Builds `examples/sample-plugin` and loads it through the upload API.

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use common::{TestApp, eventually};
use serde_json::{Value, json};

/// Builds the sample plugin and returns the path of its library.
fn build_sample_plugin() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args(["build", "-p", "logmaker-sample-plugin", "--message-format=json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo build");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == "logmaker_sample_plugin"
        })
        .flat_map(|message| message["filenames"].as_array().cloned().unwrap_or_default())
        .filter_map(|file| file.as_str().map(PathBuf::from))
        .find(|file| file.extension().is_some_and(|e| e == std::env::consts::DLL_EXTENSION))
        .expect("sample plugin library artifact")
}

#[tokio::test]
async fn uploads_and_uses_a_native_plugin() {
    let library = build_sample_plugin();
    let bytes = std::fs::read(&library).unwrap();
    let file_name = library.file_name().unwrap().to_str().unwrap().to_owned();
    let app = TestApp::new();

    let (_, body) = app.upload("/api/v1/plugin", &file_name, &bytes).await;
    assert_eq!(body["message"], "Plugin uploaded successfully", "{body}");
    let (_, body) = app.upload("/api/v1/plugin", &file_name, &bytes).await;
    assert_eq!(
        body["message"],
        "Plugin upload failed (plugin sample-plugin is already loaded)"
    );

    let plugin = app.find("/api/v1/plugin", "sample-plugin").await;
    assert!(plugin["filename"].as_str().unwrap().ends_with(&file_name));
    let types = app.get("/api/v1/plugin/maker").await;
    let counter = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "Counter")
        .unwrap();
    assert_eq!(
        counter["args"]["start"],
        json!({"type": "number", "description": "First number (default 1).", "required": false})
    );

    let output = app.dir.path().join("out.txt");
    app.ok(
        "/api/v1/maker",
        json!({"name": "n", "type": "Counter", "args": {"prefix": "evt-", "start": 7}}),
    )
    .await;
    app.ok(
        "/api/v1/sender",
        json!({"name": "file", "type": "File", "args": {"path": output}}),
    )
    .await;
    let (_, body) = app
        .post(
            "/api/v1/sender",
            json!({"name": "bad", "type": "File", "args": {"path": "/nonexistent/dir/x"}}),
        )
        .await;
    assert_eq!(
        body["message"],
        "Invalid sender argument ({\"path\":\"/nonexistent/dir/x\"})"
    );
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<n> ok", "eps": 50, "sender": ["file"]}),
    )
    .await;

    let read = || std::fs::read_to_string(&output).unwrap_or_default();
    assert!(eventually(Duration::from_secs(5), || read().lines().count() >= 3).await);
    assert_eq!(
        read().lines().take(3).collect::<Vec<_>>(),
        ["evt-7 ok", "evt-8 ok", "evt-9 ok"]
    );

    let (_, body) = app.delete("/api/v1/plugin/sample-plugin").await;
    assert_eq!(body["message"], "Plugin deletion failed (plugin is in use)");
    app.delete("/api/v1/log/l").await;
    let (_, body) = app.delete("/api/v1/plugin/sample-plugin").await;
    assert_eq!(body["message"], "Successfully deleted plugin");
    assert!(app.get("/api/v1/maker").await.as_array().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(app.dir.path().join("plugins")).unwrap().count(), 0);
}

#[tokio::test]
async fn loads_plugins_from_the_plugin_directory_on_start() {
    let library = build_sample_plugin();
    let mut app = TestApp::new();
    let plugins = app.dir.path().join("plugins");
    std::fs::copy(&library, plugins.join(library.file_name().unwrap())).unwrap();
    std::fs::write(plugins.join("legacy.jar"), b"PK").unwrap();

    app.restart();
    let names: Vec<Value> = app
        .get("/api/v1/plugin")
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].clone())
        .collect();
    assert_eq!(names, ["default-plugin", "test-plugin", "sample-plugin"]);
}
