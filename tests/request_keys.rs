use omatracker::{agent, parse_state};
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::Path, process::Command};

fn keys(path: &Path, labels: Value) -> Value {
    agent::execute(path, "request.keys", json!({"labels":labels}), None).unwrap()
}

#[test]
fn batches_are_compact_fresh_and_accept_documented_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing/ledger.json");
    let mut labels: Vec<String> = (0..63).map(|n| format!("task-{n}:create")).collect();
    labels.push("é".repeat(40)); // 80 UTF-8 bytes, preserved without normalization.
    let mut seen = HashSet::new();
    for _ in 0..3 {
        let result = keys(&path, json!(labels));
        let mapping = result["data"]["keys"].as_object().unwrap();
        assert_eq!(mapping.len(), labels.len());
        for label in &labels {
            let key = mapping[label].as_str().unwrap();
            assert!(!key.is_empty());
            assert!(seen.insert(key.to_owned()));
        }
        assert_eq!(
            result,
            json!({"schemaVersion":1,"ok":true,"changed":false,"data":{"keys":mapping}})
        );
    }
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn invalid_batches_fail_before_ledger_access() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing/ledger.json");
    for input in [
        json!({}),
        json!(null),
        json!([]),
        json!({"labels":null}),
        json!({"labels":"create"}),
        json!({"labels":[1]}),
        json!({"labels":[]}),
        json!({"labels":["create","create"]}),
        json!({"labels":["valid",""]}),
        json!({"labels":[" "]}),
        json!({"labels":[" leading"]}),
        json!({"labels":["trailing "]}),
        json!({"labels":["a\nb"]}),
        json!({"labels":["a\u{0000}b"]}),
        json!({"labels":["x".repeat(81)]}),
        json!({"labels":["é".repeat(41)]}),
        json!({"labels":(0..65).map(|n| n.to_string()).collect::<Vec<_>>()}),
        json!({"labels":["create"],"project":"ignored?"}),
    ] {
        let error = agent::execute(&path, "request.keys", input.clone(), None).unwrap_err();
        assert_eq!(
            agent::error(&error)["error"]["code"],
            "INVALID_INPUT",
            "{input}"
        );
    }
    let error = agent::execute(
        &path,
        "request.keys",
        json!({"labels":["create"]}),
        Some("old-key"),
    )
    .unwrap_err();
    assert_eq!(agent::error(&error)["error"]["code"], "INVALID_INPUT");
    // The new field does not become an ignored field on ledger writes.
    assert!(agent::execute(&path, "task.create", json!({"labels":["create"]}), None).is_err());
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn key_actions_do_not_read_migrate_or_open_ledger_locks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    // A directory at the lock-file path makes any attempt to open it fail.
    fs::create_dir(dir.path().join("ledger.json.lock")).unwrap();
    for bytes in [b"not JSON".as_slice(), br#"{"version":2}"#] {
        fs::write(&path, bytes).unwrap();
        let single = agent::execute(&path, "request.key", json!({}), None).unwrap();
        assert!(!single["data"]["key"].as_str().unwrap().is_empty());
        keys(&path, json!(["create"]));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

#[test]
fn cli_advertises_batches_and_reads_them_from_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("keys.json");
    fs::write(&input, r#"{"labels":["create","issue","render","upload"]}"#).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
        command
            .arg("--data-path")
            .arg(dir.path().join("missing/ledger.json"))
            .arg("agent");
        command
    };
    let help = command().arg("help").output().unwrap();
    assert!(help.status.success());
    let help: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert!(
        help["data"]["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("request.keys"))
    );
    assert_eq!(help["data"]["requestKeys"]["maxItems"], 64);
    assert_eq!(help["data"]["requestKeys"]["maxLabelBytes"], 80);
    let mut seen = HashSet::new();
    for _ in 0..2 {
        let output = command()
            .args(["request.keys", "--input-file"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        for value in response["data"]["keys"].as_object().unwrap().values() {
            assert!(seen.insert(value.as_str().unwrap().to_owned()));
        }
    }
    let invalid = command()
        .args(["request.keys", "--input", r#"{"labels":[]}"#])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let error: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(error["error"]["code"], "INVALID_INPUT");
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn prepared_keys_replay_lost_responses_and_preserve_target_and_argument_guards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let batch = keys(&path, json!(["task:create", "entry:add", "invoice:create"]));
    let batch = &batch["data"]["keys"];
    let task_input = json!({"project":"project-unassigned","title":"Work"});
    let task_key = batch["task:create"].as_str().unwrap();
    // Discard the response to model a committed write whose result was lost.
    agent::execute(&path, "task.create", task_input.clone(), Some(task_key)).unwrap();
    let before = fs::read(&path).unwrap();
    let task = agent::execute(&path, "task.create", task_input.clone(), Some(task_key)).unwrap();
    assert_eq!(task["replayed"], true);
    assert_eq!(fs::read(&path).unwrap(), before);
    for (action, label, input) in [
        (
            "entry.add",
            "entry:add",
            json!({"id":task["data"]["id"],"start":"2026-09-01T09:00:00Z","seconds":3600}),
        ),
        (
            "invoice.create",
            "invoice:create",
            json!({"project":"project-unassigned","from":"2026-09-01","to":"2026-09-02","currency":"USD"}),
        ),
    ] {
        let key = batch[label].as_str().unwrap();
        let first = agent::execute(&path, action, input.clone(), Some(key)).unwrap();
        let before = fs::read(&path).unwrap();
        let retry = agent::execute(&path, action, input, Some(key)).unwrap();
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["data"], first["data"]);
        assert_eq!(fs::read(&path).unwrap(), before);
        let error = agent::execute(&path, action, json!({}), Some(key)).unwrap_err();
        assert_eq!(
            agent::error(&error)["error"]["code"],
            "IDEMPOTENCY_CONFLICT"
        );
    }
    let state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.billing.invoices.len(), 1);
    agent::execute(&path, "task.remove", json!({"id":task["data"]["id"]}), None).unwrap();
    let error = agent::execute(&path, "task.create", task_input, Some(task_key)).unwrap_err();
    assert_eq!(
        agent::error(&error)["error"]["code"],
        "REQUEST_TARGET_REMOVED"
    );
}
