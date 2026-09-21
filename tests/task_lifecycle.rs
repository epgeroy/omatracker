use omatracker::{TaskStatus, agent, parse_state, presentation_status};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn call(path: &Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap_or_else(|error| panic!("{action}: {error:#}"))
        ["data"]
        .clone()
}

fn fail(path: &Path, action: &str, input: Value, code: &str) {
    let error = agent::execute(path, action, input, None).unwrap_err();
    assert_eq!(agent::error(&error)["error"]["code"], code, "{error:#}");
}

#[test]
fn task_lifecycle_records_time_blocks_done_trackers_and_can_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.json");
    let project = call(&path, "project.create", json!({"name":"Lifecycle"}));
    let task = call(
        &path,
        "task.create",
        json!({"project":project["id"],"title":"Design"}),
    );
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["status"], "stopped");
    assert_eq!(task["completedAt"], 0);

    call(&path, "task.start", json!({"id":id}));
    let concurrent = call(
        &path,
        "task.create",
        json!({"project":project["id"],"title":"Review"}),
    );
    call(&path, "task.start", json!({"id":concurrent["id"]}));
    assert_eq!(presentation_status(&path).unwrap().running_timers, 2);
    let mut state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    state.tasks[0].started_at = omatracker::now_ms() - 61_000;
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    let done = call(&path, "task.complete", json!({"id":id}));
    assert_eq!(done["status"], "done");
    assert!(done["completedAt"].as_i64().unwrap() > 0);
    assert_eq!(done["startedAt"], 0);
    assert!(
        parse_state(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .entries[0]
            .seconds
            >= 60
    );
    fail(&path, "task.start", json!({"id":id}), "TASK_DONE");
    assert!(
        omatracker::reset_task(&path, id)
            .unwrap_err()
            .to_string()
            .contains("TASK_DONE")
    );

    call(&path, "task.update", json!({"id":id,"add":"15m"}));
    assert_eq!(call(&path, "task.get", json!({"id":id}))["status"], "done");

    let reopened = call(&path, "task.reopen", json!({"id":id}));
    assert_eq!(reopened["status"], "stopped");
    assert_eq!(reopened["completedAt"], 0);
    assert_eq!(
        call(&path, "task.start", json!({"id":id}))["status"],
        "tracking"
    );
}

#[test]
fn legacy_running_tasks_migrate_to_tracking_and_done_tasks_are_separated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.json");
    fs::write(
        &path,
        r#"{"version":4,"activeProjectId":"project-unassigned","projects":[{"id":"project-unassigned","name":"Unassigned"}],"tasks":[{"id":"tracking","projectId":"project-unassigned","title":"Tracking","running":true,"startedAt":1000},{"id":"done","projectId":"project-unassigned","title":"Done","status":"done","completedAt":2000}]}"#,
    )
    .unwrap();

    let state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(state.tasks[0].status, TaskStatus::Tracking);
    assert_eq!(state.tasks[1].status, TaskStatus::Done);
    let status = presentation_status(&path).unwrap();
    assert_eq!(status.active_tasks.len(), 1);
    assert_eq!(status.completed_tasks.len(), 1);
    assert_eq!(status.completed_tasks[0].task.id, "done");
}
