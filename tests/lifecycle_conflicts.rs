use omatracker::agent;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn call(path: &Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap_or_else(|e| panic!("{action}: {e:#}"))["data"]
        .clone()
}

#[test]
fn entity_tokens_ignore_unrelated_writes_but_reject_changes_to_the_same_entity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let client = call(&path, "client.set", json!({"details":{"name":"A"}}));
    let unrelated = call(&path, "client.set", json!({"details":{"name":"B"}}));
    let renamed = call(
        &path,
        "client.update",
        json!({"id":client["id"],"name":"Renamed A","entityRevision":client["entityRevision"]}),
    );
    call(&path, "client.remove", json!({"id":unrelated["id"]}));
    call(
        &path,
        "client.remove",
        json!({"id":client["id"],"entityRevision":renamed["entityRevision"]}),
    );
    let project = call(&path, "project.create", json!({"name":"Project"}));
    call(&path, "issuer.set", json!({"details":{"name":"Studio"}}));
    let configured = call(
        &path,
        "project.update",
        json!({"project":project["id"],"name":"Changed","rate":"50","currency":"USD","entityRevision":project["entityRevision"]}),
    );
    assert_eq!(
        configured["entityRevision"],
        call(&path, "project.get", json!({"project":project["id"]}))["entityRevision"]
    );
    let task = call(
        &path,
        "task.create",
        json!({"project":project["id"],"title":"Task"}),
    );
    call(
        &path,
        "task.create",
        json!({"project":project["id"],"title":"Other task"}),
    );
    call(
        &path,
        "task.update",
        json!({"id":task["id"],"title":"New title","entityRevision":task["entityRevision"]}),
    );
    let before = fs::read(&path).unwrap();
    let error = agent::execute(
        &path,
        "task.remove",
        json!({"id":task["id"],"entityRevision":task["entityRevision"]}),
        None,
    )
    .unwrap_err();
    assert_eq!(agent::error(&error)["error"]["code"], "REVISION_CONFLICT");
    assert_eq!(fs::read(&path).unwrap(), before);
    call(
        &path,
        "project.remove",
        json!({"project":project["id"],"entityRevision":configured["entityRevision"]}),
    );
}

#[test]
fn recreating_with_a_new_key_gets_a_new_id_and_stale_creation_keys_do_not_resurrect_entities() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    for (create, remove, input, target) in [
        (
            "client.set",
            "client.remove",
            json!({"details":{"name":"Same name"}}),
            "id",
        ),
        (
            "project.create",
            "project.remove",
            json!({"name":"Same name"}),
            "project",
        ),
    ] {
        let key = format!("first-{create}");
        let first = agent::execute(&path, create, input.clone(), Some(&key)).unwrap();
        let id = first["data"]["id"].clone();
        let retry = agent::execute(&path, create, input.clone(), Some(&key)).unwrap();
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["data"]["id"], id);
        call(&path, remove, json!({target:id}));
        let error = agent::execute(&path, create, input.clone(), Some(&key)).unwrap_err();
        assert_eq!(
            agent::error(&error)["error"]["code"],
            "REQUEST_TARGET_REMOVED"
        );
        let new_key = call(&path, "request.key", json!({}))["key"]
            .as_str()
            .unwrap()
            .to_owned();
        let new = agent::execute(&path, create, input, Some(&new_key)).unwrap();
        assert_ne!(new["data"]["id"], id);
        assert_eq!(new["requestKey"], new_key);
        let conflict =
            agent::execute(&path, create, json!({"name":"different"}), Some(&new_key)).unwrap_err();
        assert_eq!(
            agent::error(&conflict)["error"]["code"],
            "IDEMPOTENCY_CONFLICT"
        );
    }
}

#[test]
fn cloning_an_archived_project_does_not_reassign_its_deleted_client() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let client = call(&path, "client.set", json!({"details":{"name":"Client"}}));
    let project = call(
        &path,
        "project.create",
        json!({"name":"Project","client":client["id"]}),
    );
    call(&path, "project.remove", json!({"project":project["id"]}));
    call(&path, "client.remove", json!({"id":client["id"]}));
    let cloned = call(
        &path,
        "project.create",
        json!({"name":"Project","copyFrom":project["id"]}),
    );
    assert_eq!(cloned["billing"]["clientId"], "");
    assert_eq!(cloned["project"]["clientName"], "");
    let replacement = call(&path, "client.set", json!({"details":{"name":"Client"}}));
    call(
        &path,
        "project.update",
        json!({"project":cloned["id"],"client":replacement["id"],"entityRevision":cloned["entityRevision"]}),
    );
}

#[test]
fn auto_keys_are_unique_reported_before_execution_and_reusable_for_exact_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let command = |key: &str| {
        Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .arg("--data-path")
            .arg(&path)
            .args([
                "agent",
                "client.set",
                "--input",
                r#"{"details":{"name":"Client"}}"#,
                "--key",
                key,
            ])
            .output()
            .unwrap()
    };
    let first = command("auto");
    assert!(first.status.success());
    let response: Value = serde_json::from_slice(&first.stdout).unwrap();
    let key = response["requestKey"].as_str().unwrap();
    assert!(String::from_utf8_lossy(&first.stderr).contains(key));
    let retry: Value = serde_json::from_slice(&command(key).stdout).unwrap();
    assert_eq!(retry["replayed"], true);
    assert_eq!(retry["data"]["id"], response["data"]["id"]);
    call(&path, "client.remove", json!({"id":response["data"]["id"]}));
    let next: Value = serde_json::from_slice(&command("auto").stdout).unwrap();
    assert_ne!(next["requestKey"], response["requestKey"]);
    assert_ne!(next["data"]["id"], response["data"]["id"]);
}

#[test]
fn request_key_generation_does_not_create_or_open_a_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing/ledger.json");
    let one = call(&path, "request.key", json!({}));
    let two = call(&path, "request.key", json!({}));
    assert_ne!(one["key"], two["key"]);
    assert!(!path.parent().unwrap().exists());
}
