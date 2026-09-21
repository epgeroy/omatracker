use omatracker::{State, Task, TaskStatus, agent, parse_state};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn call(path: &Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap()["data"].clone()
}

fn create(path: &Path, project: &str, title: &str) -> Value {
    call(
        path,
        "task.create",
        json!({"project":project,"title":title}),
    )
}

#[test]
fn cursor_catalogue_scopes_filters_orders_pages_and_preserves_legacy_lists() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.json");
    let first_project = call(&path, "project.create", json!({"name":"First"}));
    let second_project = call(&path, "project.create", json!({"name":"Second"}));
    let alpha = create(&path, first_project["id"].as_str().unwrap(), "Alpha design");
    let beta = create(&path, first_project["id"].as_str().unwrap(), "Beta review");
    let done = create(
        &path,
        first_project["id"].as_str().unwrap(),
        "Finished work",
    );
    let other = create(
        &path,
        second_project["id"].as_str().unwrap(),
        "Alpha elsewhere",
    );

    let mut state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    for task in &mut state.tasks {
        task.activity_at = match task.id.as_str() {
            id if id == alpha["id"] => 3_000,
            id if id == beta["id"] => 2_000,
            id if id == done["id"] => 1_500,
            id if id == other["id"] => 1_000,
            _ => 0,
        };
        if task.id == beta["id"] {
            task.status = TaskStatus::Tracking;
            task.started_at = 2_000;
        }
        if task.id == done["id"] {
            task.status = TaskStatus::Done;
            task.completed_at = 1_500;
        }
    }
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    let first = call(
        &path,
        "task.list",
        json!({"project":first_project["id"],"state":"open","query":"ALPHA","limit":1}),
    );
    assert_eq!(first["total"], 1);
    assert_eq!(first["items"][0]["id"], alpha["id"]);
    assert_eq!(first["items"][0]["projectId"], first_project["id"]);
    assert_eq!(first["items"][0]["projectName"], "First");
    assert_eq!(first["items"][0]["activityAt"], 3_000);
    for field in [
        "createdAt",
        "lastTrackedAt",
        "durationSeconds",
        "rate",
        "rateSource",
        "entityRevision",
    ] {
        assert!(first["items"][0].get(field).is_some(), "missing {field}");
    }
    assert!(first["nextCursor"].is_null());

    for (filter, expected) in [("stopped", &alpha), ("tracking", &beta), ("done", &done)] {
        let result = call(
            &path,
            "task.list",
            json!({"project":first_project["id"],"state":filter}),
        );
        assert_eq!(result["total"], 1, "{filter}");
        assert_eq!(result["items"][0]["id"], expected["id"], "{filter}");
    }

    let paged = call(
        &path,
        "task.list",
        json!({"project":first_project["id"],"state":"all","limit":1}),
    );
    assert_eq!(paged["total"], 3);
    assert_eq!(paged["items"][0]["id"], alpha["id"]);
    let cursor = paged["nextCursor"].as_str().unwrap();
    assert!(!cursor.contains(alpha["id"].as_str().unwrap()));
    let next = call(
        &path,
        "task.list",
        json!({"project":first_project["id"],"state":"all","limit":1,"cursor":cursor}),
    );
    assert_eq!(next["items"][0]["id"], beta["id"]);
    assert!(next["nextCursor"].is_string());

    let legacy = call(
        &path,
        "task.list",
        json!({"project":first_project["id"],"limit":1}),
    );
    assert_eq!(legacy["offset"], 0);
    assert_eq!(legacy["nextOffset"], 1);
    assert!(legacy.get("nextCursor").is_none());

    call(
        &path,
        "project.delete",
        json!({"project":second_project["id"]}),
    );
    let all = call(
        &path,
        "task.list",
        json!({"allProjects":true,"state":"all"}),
    );
    assert_eq!(all["total"], 3);
    assert!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["projectId"] == first_project["id"])
    );
}

#[test]
fn cursor_catalogue_bounds_results_and_rejects_invalid_scope_or_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.json");
    let project = call(&path, "project.create", json!({"name":"Many"}));
    let project_id = project["id"].as_str().unwrap();
    let mut state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    state.tasks.extend((0..201).map(|index| Task {
        id: format!("task-{index:03}"),
        project_id: project_id.into(),
        title: format!("Task {index}"),
        created_at: index,
        activity_at: index,
        ..Task::default()
    }));
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    let bounded = call(
        &path,
        "task.list",
        json!({"project":project_id,"state":"all","limit":999}),
    );
    assert_eq!(bounded["total"], 201);
    assert_eq!(bounded["items"].as_array().unwrap().len(), 200);
    assert!(bounded["nextCursor"].is_string());

    for input in [
        json!({"state":"all"}),
        json!({"state":null}),
        json!({"allProjects":false,"state":"all"}),
        json!({"project":project_id,"allProjects":true,"state":"all"}),
        json!({"project":project_id,"state":"all","cursor":"bad"}),
        json!({"project":project_id,"state":"all","cursor":"€a"}),
    ] {
        let error = agent::execute(&path, "task.list", input, None).unwrap_err();
        assert_eq!(agent::error(&error)["error"]["code"], "INVALID_INPUT");
    }
}

#[test]
fn task_recency_is_updated_by_actions_and_historical_evidence_is_honest() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.json");
    let project = call(&path, "project.create", json!({"name":"Recency"}));
    let task = create(&path, project["id"].as_str().unwrap(), "Tracked");
    assert!(task["createdAt"].as_i64().unwrap() > 0);
    assert_eq!(task["activityAt"], task["createdAt"]);

    call(&path, "task.start", json!({"id":task["id"]}));
    let mut state = parse_state(&fs::read_to_string(&path).unwrap()).unwrap();
    state.tasks[0].started_at = omatracker::now_ms() - 61_000;
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    let stopped = call(&path, "task.stop", json!({"id":task["id"]}));
    assert!(stopped["lastTrackedAt"].as_i64().unwrap() > 0);
    let before_correction = stopped["activityAt"].as_i64().unwrap();
    let entry = parse_state(&fs::read_to_string(&path).unwrap())
        .unwrap()
        .entries[0]
        .clone();
    call(
        &path,
        "entry.correct",
        json!({"id":entry.id,"revision":0,"delta":1,"reason":"Correct duration"}),
    );
    let corrected = call(&path, "task.get", json!({"id":task["id"]}));
    assert!(corrected["activityAt"].as_i64().unwrap() >= before_correction);

    let historical: State = parse_state(r#"{"version":5,"activeProjectId":"project-unassigned","projects":[{"id":"project-unassigned","name":"Unassigned"}],"tasks":[{"id":"old","projectId":"project-unassigned","title":"Old"}],"entries":[{"id":"entry","projectId":"project-unassigned","taskId":"old","taskTitle":"Old","startedAt":1000,"endedAt":2000,"seconds":1}]}"#).unwrap();
    assert_eq!(historical.tasks[0].created_at, 0);
    assert_eq!(historical.tasks[0].last_tracked_at, 2_000);
    assert_eq!(historical.tasks[0].activity_at, 2_000);
}
