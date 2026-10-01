//! Data files written by the Java edition load unchanged; entries that cannot
//! be recreated survive saves; files written here stay readable by the Java
//! edition (its reader rejects unknown properties).

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use common::{TestApp, eventually};
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, value: Value) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(file), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn read(dir: &Path, file: &str) -> Vec<Value> {
    serde_json::from_slice::<Value>(&std::fs::read(dir.join(file)).unwrap())
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn names(entries: &[Value]) -> Vec<&str> {
    entries.iter().map(|e| e["name"].as_str().unwrap()).collect()
}

/// Java-edition files, including an entry whose type comes from a Java plugin
/// that no longer exists and the log and scenario depending on it.
fn java_fixtures(data: &Path) {
    write(
        data,
        "makers.json",
        json!([
            {"name": "srcip", "type": "IPRange", "args": {"start": "10.0.0.1", "end": "10.0.0.254", "deviation": 30},
             "sample": "10.0.0.120", "size": 100000, "ref": 1, "regTime": 1700000000},
            {"name": "seq", "type": "NumberRange", "args": {"start": 1, "end": 3, "random": false},
             "sample": 1, "size": 100000, "ref": 0, "regTime": 1700000001},
            {"name": "custom", "type": "CustomJava", "args": {"x": 1}, "sample": "?", "size": 0, "ref": 1, "regTime": 1700000002}
        ]),
    );
    write(
        data,
        "senders.json",
        json!([
            {"name": "out", "type": "Capture", "args": {}, "limit": 0, "ref": 1, "count": 10, "bytes": 100, "bytesPerSec": 0, "regTime": 1700000000}
        ]),
    );
    write(
        data,
        "logs.json",
        json!([
            {"name": "fw", "format": "<seq> src=<srcip>", "eps": 20, "epsUnit": "events", "epsTimeUnit": "sec", "sender": ["out"],
             "paused": false, "sample": "x", "currentEps": 2, "count": 5, "bytes": 50, "bytesPerSec": 20, "regTime": 1700000000},
            {"name": "orphan-log", "format": "<custom>", "eps": 1, "epsUnit": "events", "epsTimeUnit": "sec", "sender": ["out"],
             "paused": true, "currentEps": 0, "count": 0, "bytes": 0, "bytesPerSec": 0, "regTime": 1700000001}
        ]),
    );
    write(
        data,
        "scenarios.json",
        json!([
            {"name": "s1", "description": "d", "sharedVariables": {}, "steps": [{"logName": "fw", "repeat": 1, "delayMinMs": 0,
             "delayMaxMs": 0, "senders": ["out"], "overrides": {}}], "intervalMinMs": 1000, "intervalMaxMs": 5000, "loopCount": 0,
             "status": false, "count": 0, "currentStep": 0, "currentLoop": 0, "totalSteps": 0, "stepCounts": null},
            {"name": "orphan-scenario", "steps": [{"logName": "orphan-log", "senders": ["out"]}], "loopCount": 1}
        ]),
    );
}

#[tokio::test]
async fn loads_java_files_and_keeps_entries_that_cannot_load() {
    let mut app = TestApp::new();
    let data = app.dir.path().join("data");
    java_fixtures(&data);
    app.restart();

    assert_eq!(app.find("/api/v1/maker", "srcip").await["regTime"], 1700000000);
    assert_eq!(app.find("/api/v1/scenario", "s1").await["description"], "d");
    assert!(eventually(Duration::from_secs(5), || app.captured.lines("out").len() >= 3).await);
    let lines = app.captured.lines("out");
    assert_eq!(lines[..3].iter().map(|l| &l[..1]).collect::<Vec<_>>(), ["1", "2", "3"]);
    assert!(lines[0].starts_with("1 src=10.0.0."));

    let listed = app.get("/api/v1/maker").await;
    assert!(!names(listed.as_array().unwrap()).contains(&"custom"));

    // Saving any definition writes the unloadable ones back.
    app.ok("/api/v1/maker", json!({"name": "new", "type": "IP"})).await;
    app.ok(
        "/api/v1/log",
        json!({"name": "new-log", "format": "<new>", "paused": true}),
    )
    .await;
    app.ok("/api/v1/scenario", json!({"name": "new-scenario"})).await;
    let makers = read(&data, "makers.json");
    assert_eq!(names(&makers), ["srcip", "seq", "new", "custom"]);
    assert_eq!(makers[3]["args"], json!({"x": 1}), "kept verbatim");
    assert_eq!(names(&read(&data, "logs.json")), ["fw", "new-log", "orphan-log"]);
    assert_eq!(
        names(&read(&data, "scenarios.json")),
        ["s1", "new-scenario", "orphan-scenario"]
    );

    // Creating an entry with the same name replaces the unloadable one.
    app.ok("/api/v1/maker", json!({"name": "custom", "type": "UUID"})).await;
    let makers = read(&data, "makers.json");
    assert_eq!(names(&makers).iter().filter(|n| **n == "custom").count(), 1);
    assert_eq!(makers.iter().find(|m| m["name"] == "custom").unwrap()["type"], "UUID");
}

fn keys(entries: &[Value]) -> HashSet<String> {
    entries
        .iter()
        .flat_map(|e| e.as_object().unwrap().keys().cloned())
        .collect()
}

fn set(keys: &[&str]) -> HashSet<String> {
    keys.iter().map(|k| (*k).to_owned()).collect()
}

#[tokio::test]
async fn written_files_only_use_fields_known_to_the_java_edition() {
    let app = TestApp::new();
    app.ok(
        "/api/v1/maker",
        json!({"name": "a", "type": "Fixed", "args": {"value": "A"}}),
    )
    .await;
    app.ok("/api/v1/sender", json!({"name": "out", "type": "Capture", "limit": 3}))
        .await;
    app.ok(
        "/api/v1/log",
        json!({"name": "l", "format": "<a>", "sender": ["out"], "paused": true}),
    )
    .await;
    app.ok(
        "/api/v1/scenario",
        json!({"name": "s", "steps": [{"logName": "l", "senders": ["out"], "overrides": {"a": "x"}}]}),
    )
    .await;
    let data = app.dir.path().join("data");

    let java_maker = set(&["name", "type", "args", "sample", "size", "ref", "regTime"]);
    let java_sender = set(&[
        "name",
        "type",
        "args",
        "limit",
        "ref",
        "count",
        "bytes",
        "bytesPerSec",
        "regTime",
    ]);
    let java_log = set(&[
        "name",
        "format",
        "eps",
        "epsUnit",
        "epsTimeUnit",
        "sender",
        "paused",
        "sample",
        "currentEps",
        "count",
        "bytes",
        "bytesPerSec",
        "regTime",
    ]);
    let java_scenario = set(&[
        "name",
        "description",
        "sharedVariables",
        "steps",
        "intervalMinMs",
        "intervalMaxMs",
        "loopCount",
        "status",
        "count",
        "currentStep",
        "currentLoop",
        "totalSteps",
        "stepCounts",
    ]);
    let java_step = set(&["logName", "repeat", "delayMinMs", "delayMaxMs", "senders", "overrides"]);

    assert!(keys(&read(&data, "makers.json")).is_subset(&java_maker));
    assert!(keys(&read(&data, "senders.json")).is_subset(&java_sender));
    assert!(keys(&read(&data, "logs.json")).is_subset(&java_log));
    let scenarios = read(&data, "scenarios.json");
    assert!(keys(&scenarios).is_subset(&java_scenario));
    assert!(keys(scenarios[0]["steps"].as_array().unwrap()).is_subset(&java_step));
}
