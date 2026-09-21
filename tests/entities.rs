use omatracker::{DEFAULT_PROJECT_ID, State, agent};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct App {
    home: tempfile::TempDir,
    path: PathBuf,
    project: String,
    task: String,
    client: String,
}

fn request(path: &Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap_or_else(|e| panic!("{action}: {e:#}"))
}

impl App {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("ledger.json");
        let client = request(&path, "client.set", json!({"details": {
            "name":"Original Client", "address":"Original address", "email":"client@example.test",
            "registrationId":"REG-1", "paymentInstructions":"Bank transfer"
        }}))["data"]["id"].as_str().unwrap().to_owned();
        let project = request(
            &path,
            "project.create",
            json!({"name":"Original Project", "client":client,
            "rate":"80", "currency":"USD", "effectiveAt":"2025-01-01T00:00:00Z"}),
        )["data"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let task = request(
            &path,
            "task.create",
            json!({"project":project, "title":"Original Task"}),
        )["data"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        request(
            &path,
            "issuer.set",
            json!({"details":{"name":"Test Studio"}}),
        );
        request(
            &path,
            "entry.add",
            json!({"id":task,"start":"2025-08-10T10:00:00Z","seconds":3600}),
        );
        Self {
            home,
            path,
            project,
            task,
            client,
        }
    }
    fn call(&self, action: &str, input: Value) -> Value {
        request(&self.path, action, input)["data"].clone()
    }
    fn state(&self) -> State {
        omatracker::parse_state(&fs::read_to_string(&self.path).unwrap()).unwrap()
    }
    fn fail(&self, action: &str, input: Value, code: &str) {
        let before = fs::read(&self.path).unwrap();
        let error = agent::execute(&self.path, action, input, None).unwrap_err();
        assert_eq!(agent::error(&error)["error"]["code"], code, "{error:#}");
        assert_eq!(fs::read(&self.path).unwrap(), before);
    }
    fn running(&self) {
        self.call("task.start", json!({"id":self.task}));
        let mut state = self.state();
        state
            .tasks
            .iter_mut()
            .find(|t| t.id == self.task)
            .unwrap()
            .started_at = omatracker::now_ms() - 61_000;
        fs::write(&self.path, serde_json::to_vec(&state).unwrap()).unwrap();
    }
    fn draft(&self) -> Value {
        self.call(
            "invoice.create",
            json!({"project":self.project,"from":"2025-08-01","to":"2025-09-01","currency":"USD"}),
        )
    }
    fn issue(&self) -> Value {
        let draft = self.draft();
        self.call(
            "invoice.issue",
            json!({"id":draft["id"],"revision":draft["revision"],"date":"2025-09-01"}),
        )
    }
}

#[test]
fn task_update_supports_name_and_title_without_changing_history_or_timer() {
    let app = App::new();
    app.running();
    let before = app.state();
    let renamed = app.call(
        "task.update",
        json!({"id":app.task,"name":"  New   Task  "}),
    );
    assert_eq!(renamed["title"], "New Task");
    assert_eq!(renamed["id"], app.task);
    assert_eq!(renamed["startedAt"], before.tasks[0].started_at);
    assert_eq!(renamed["running"], true);
    assert_eq!(app.state().entries[0].task_title, "Original Task");
    let again = request(
        &app.path,
        "task.update",
        json!({"id":app.task,"title":"New Task"}),
    );
    assert_eq!(again["changed"], false);
    app.fail(
        "task.update",
        json!({"id":app.task,"name":" ","title":" "}),
        "INVALID_INPUT",
    );
    app.fail(
        "task.update",
        json!({"id":app.task,"name":"A","title":"B"}),
        "INVALID_INPUT",
    );
    app.fail(
        "task.update",
        json!({"id":"missing","name":"New"}),
        "TASK_NOT_FOUND",
    );
    assert_eq!(
        app.call("task.get", json!({"id":app.task}))["title"],
        "New Task"
    );
}

#[test]
fn rename_projects_and_clients_preserves_details_and_issued_invoices() {
    let app = App::new();
    let issued = app.issue();
    let bytes = fs::read(Path::new(issued["bundle"].as_str().unwrap()).join("data.json")).unwrap();
    let old_settings = serde_json::to_value(&app.state().billing.projects[&app.project]).unwrap();
    let client = app.call(
        "client.update",
        json!({"id":app.client,"name":"Renamed Client"}),
    );
    assert_eq!(client["details"]["address"], "Original address");
    assert_eq!(client["details"]["email"], "client@example.test");
    assert_eq!(client["details"]["registrationId"], "REG-1");
    assert_eq!(client["details"]["paymentInstructions"], "Bank transfer");
    let project = app.call(
        "project.update",
        json!({"project":app.project,"name":"Renamed Project"}),
    );
    assert_eq!(project["project"]["id"], app.project);
    assert_eq!(project["project"]["clientName"], "Renamed Client");
    assert_eq!(project["billing"], old_settings);
    let invoice = app.call("invoice.get", json!({"id":issued["id"]}));
    assert_eq!(invoice["project"]["name"], "Original Project");
    assert_eq!(invoice["client"]["name"], "Original Client");
    assert_eq!(
        fs::read(Path::new(issued["bundle"].as_str().unwrap()).join("data.json")).unwrap(),
        bytes
    );
    app.fail(
        "project.update",
        json!({"project":app.project,"name":"\n\t"}),
        "INVALID_INPUT",
    );
    app.fail(
        "client.update",
        json!({"id":app.client,"name":" "}),
        "INVALID_INPUT",
    );
    app.fail(
        "client.update",
        json!({"id":"missing","name":"X"}),
        "CLIENT_NOT_FOUND",
    );
}

#[test]
fn client_updates_sync_linked_projects_and_require_draft_refresh() {
    let app = App::new();
    let draft = app.draft();
    let second = app.call(
        "project.create",
        json!({"name":"Second","client":app.client}),
    );
    app.call(
        "client.update",
        json!({"id":app.client,"name":"New client name"}),
    );
    assert_eq!(
        app.call("project.get", json!({"project":second["id"]}))["project"]["clientName"],
        "New client name"
    );
    app.fail(
        "invoice.issue",
        json!({"id":draft["id"],"revision":1,"date":"2025-09-01"}),
        "STALE_DRAFT",
    );
    let refreshed = app.call("invoice.refresh", json!({"id":draft["id"],"revision":1}));
    assert_eq!(refreshed["client"]["name"], "New client name");
    app.call(
        "client.set",
        json!({"id":app.client,"details":{"name":"Replacement name","address":"New address"}}),
    );
    assert_eq!(
        app.call("project.get", json!({"project":app.project}))["project"]["clientName"],
        "Replacement name"
    );
}

#[test]
fn task_delete_records_running_time_and_is_retry_safe() {
    let app = App::new();
    let issued = app.issue();
    app.running();
    let input = json!({"id":app.task});
    let removed =
        agent::execute(&app.path, "task.delete", input.clone(), Some("remove-task")).unwrap();
    assert_eq!(removed["data"]["removed"], true);
    let state = app.state();
    assert!(state.tasks.iter().all(|t| t.id != app.task));
    assert_eq!(state.entries.len(), 2);
    assert!(state.entries[1].seconds >= 60);
    assert_eq!(state.entries[0].seconds, 3600);
    let repeat = agent::execute(&app.path, "task.delete", input, Some("remove-task")).unwrap();
    assert_eq!(repeat["replayed"], true);
    assert_eq!(app.state().entries.len(), 2);
    assert_eq!(
        app.call("invoice.get", json!({"id":issued["id"]}))["totalMinor"],
        "8000"
    );
    app.fail("task.get", json!({"id":app.task}), "TASK_NOT_FOUND");
    app.fail("task.remove", json!({"id":"missing"}), "TASK_NOT_FOUND");
}

#[test]
fn task_archive_hides_task_preserves_identity_and_can_be_restored() {
    let app = App::new();
    app.running();
    let archived = app.call("task.archive", json!({"id":app.task}));
    assert_eq!(archived["archived"], true);
    assert_eq!(archived["changed"], true);

    let state = app.state();
    assert_eq!(state.tasks[0].id, app.task);
    assert!(!state.tasks[0].running);
    assert_eq!(state.entries.len(), 2);
    assert_eq!(
        app.call("task.list", json!({"project":app.project}))["total"],
        0
    );
    assert_eq!(
        app.call(
            "task.list",
            json!({"project":app.project,"includeArchived":true})
        )["items"][0]["archived"],
        true
    );
    assert_eq!(
        app.call("task.get", json!({"id":app.task}))["archived"],
        true
    );
    assert_eq!(
        omatracker::presentation_status(&app.path)
            .unwrap()
            .active_tasks
            .len(),
        0
    );
    assert_eq!(app.call("context", json!({}))["runningTasks"], json!([]));
    app.fail("task.start", json!({"id":app.task}), "TASK_ARCHIVED");
    app.fail(
        "task.update",
        json!({"id":app.task,"name":"New"}),
        "TASK_ARCHIVED",
    );
    app.fail(
        "task.rate",
        json!({"id":app.task,"rate":"90","currency":"USD"}),
        "TASK_ARCHIVED",
    );
    app.fail(
        "entry.add",
        json!({"id":app.task,"start":"2025-08-11T10:00:00Z","seconds":60}),
        "TASK_ARCHIVED",
    );

    let restored = app.call("task.restore", json!({"id":app.task}));
    assert_eq!(restored["restored"], true);
    assert_eq!(
        app.call("task.get", json!({"id":app.task}))["archived"],
        false
    );
    assert_eq!(
        app.call("task.list", json!({"project":app.project}))["total"],
        1
    );
    assert_eq!(
        app.call("task.restore", json!({"id":app.task}))["restored"],
        false
    );
}

#[test]
fn project_delete_archives_history_stops_timers_and_clears_active_bindings() {
    let app = App::new();
    app.running();
    omatracker::select_project(&app.path, &app.project).unwrap();
    app.call(
        "repository.bind",
        json!({"project":app.project,"repository":app.home.path()}),
    );
    let result = app.call("project.delete", json!({"project":app.project}));
    assert_eq!(result["archived"], true);
    let state = app.state();
    assert_eq!(state.active_project_id, DEFAULT_PROJECT_ID);
    assert!(!state.tasks[0].running);
    assert_eq!(state.entries.len(), 2);
    assert!(state.entries.iter().all(|e| e.project_id == app.project));
    assert!(state.billing.bindings.is_empty());
    assert_eq!(state.billing.projects[&app.project].cadence, "manual");
    assert!(
        !app.call("project.list", json!({}))["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == app.project)
    );
    let archived = app.call("project.list", json!({"includeArchived":true}));
    assert!(
        archived["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == app.project && p["archived"] == true)
    );
    let presentation = omatracker::presentation_status(&app.path).unwrap();
    assert!(
        !presentation
            .state
            .projects
            .iter()
            .any(|p| p.id == app.project)
    );
    assert_eq!(presentation.running_timers, 0);
    assert_eq!(presentation.total_tracked_seconds, 0);
    assert_eq!(
        app.call(
            "summary",
            json!({"project":app.project,"from":"2025-08-01","to":"2025-09-01"})
        )["recordedSeconds"],
        3600
    );
    assert!(
        app.call("invoice.check", json!({}))["created"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    omatracker::check_reports(&app.path).unwrap();
    assert!(app.state().reports.is_empty());
    assert_eq!(
        app.call("project.remove", json!({"project":app.project}))["removed"],
        false
    );
    app.fail("task.start", json!({"id":app.task}), "PROJECT_ARCHIVED");
    app.fail(
        "task.create",
        json!({"project":app.project,"title":"New"}),
        "PROJECT_ARCHIVED",
    );
    app.fail(
        "entry.add",
        json!({"id":app.task,"start":"2025-08-10T10:00:00Z","seconds":60}),
        "PROJECT_ARCHIVED",
    );
    app.fail(
        "project.update",
        json!({"project":app.project,"name":"New"}),
        "PROJECT_ARCHIVED",
    );
    assert!(omatracker::select_project(&app.path, &app.project).is_err());
    assert!(omatracker::start_task(&app.path, &app.task).is_err());
    assert!(omatracker::edit_task(&app.path, &app.task, Some("New"), None).is_err());
}

#[test]
fn client_delete_requires_unlinking_active_projects_and_preserves_archived_billing() {
    let app = App::new();
    app.fail("client.remove", json!({"id":app.client}), "CLIENT_IN_USE");
    app.call("project.remove", json!({"project":app.project}));
    app.call("client.delete", json!({"id":app.client}));
    assert_eq!(app.call("client.list", json!({}))["total"], 0);
    assert_eq!(
        app.call("client.list", json!({"includeArchived":true}))["items"][0]["archived"],
        true
    );
    assert_eq!(
        app.call("client.get", json!({"id":app.client}))["details"]["address"],
        "Original address"
    );
    let invoice = app.issue();
    assert_eq!(invoice["client"]["name"], "Original Client");
    assert_eq!(invoice["client"]["address"], "Original address");
    app.fail(
        "project.create",
        json!({"name":"New","client":app.client}),
        "CLIENT_ARCHIVED",
    );
    app.fail(
        "client.update",
        json!({"id":app.client,"name":"New"}),
        "CLIENT_ARCHIVED",
    );
    assert_eq!(
        app.call("client.remove", json!({"id":app.client}))["removed"],
        false
    );

    let active = App::new();
    active.call(
        "project.update",
        json!({"project":active.project,"client":""}),
    );
    let settings = active.call("project.get", json!({"project":active.project}));
    assert_eq!(settings["project"]["clientName"], "");
    assert_eq!(settings["billing"]["clientId"], "");
    assert_eq!(
        active.call("client.remove", json!({"id":active.client}))["removed"],
        true
    );
}

#[test]
fn entity_operations_validate_revisions_and_protect_default_project() {
    let app = App::new();
    let old_revision = app.state().billing.revision;
    app.call("task.update", json!({"id":app.task,"name":"A change"}));
    for (action, input) in [
        (
            "task.update",
            json!({"id":app.task,"name":"Other","revision":old_revision}),
        ),
        (
            "task.remove",
            json!({"id":app.task,"revision":old_revision}),
        ),
        (
            "project.remove",
            json!({"project":app.project,"revision":old_revision}),
        ),
        (
            "client.update",
            json!({"id":app.client,"name":"Other","revision":old_revision}),
        ),
    ] {
        app.fail(action, input, "REVISION_CONFLICT");
    }
    app.fail(
        "project.remove",
        json!({"project":DEFAULT_PROJECT_ID}),
        "PROTECTED_PROJECT",
    );
    app.fail(
        "project.delete",
        json!({"project":"missing"}),
        "PROJECT_NOT_FOUND",
    );
    app.fail("client.remove", json!({"id":"missing"}), "CLIENT_NOT_FOUND");
    let no_op = agent::execute(
        &app.path,
        "task.update",
        json!({"id":app.task,"name":"A change"}),
        Some("no-op-update"),
    )
    .unwrap();
    assert_eq!(no_op["changed"], false);
    assert_eq!(no_op["revision"], app.state().billing.revision);
}
