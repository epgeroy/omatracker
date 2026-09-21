use omatracker::{STATE_VERSION, State, TaskStatus, agent};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct App {
    _home: tempfile::TempDir,
    path: PathBuf,
    project: String,
    task: String,
}
impl App {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("ledger.json");
        let project = call(&path, "project.create", json!({"name":"Unrated project"}))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let task = call(
            &path,
            "task.create",
            json!({"project":project,"title":"Unrated task"}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            _home: home,
            path,
            project,
            task,
        }
    }
    fn add(&self, start: &str, seconds: i64) -> String {
        call(
            &self.path,
            "entry.add",
            json!({"id":self.task,"start":start,"seconds":seconds}),
        )["entries"][0]["entry"]["id"]
            .as_str()
            .unwrap()
            .into()
    }
    fn state(&self) -> State {
        omatracker::parse_state(&fs::read_to_string(&self.path).unwrap()).unwrap()
    }
    fn rate(&self, input: Value) -> Value {
        let mut input = input;
        input["id"] = json!(self.task);
        call(&self.path, "task.rate", input)
    }
    fn draft(&self) -> Value {
        call(
            &self.path,
            "invoice.create",
            json!({"project":self.project,"from":"2025-08-01","to":"2025-09-01","currency":"USD"}),
        )
    }
}
fn call(path: &std::path::Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap_or_else(|e| panic!("{action}: {e:#}"))["data"]
        .clone()
}

#[test]
fn task_rate_applies_to_new_work_without_repricing_recorded_time_or_other_tasks() {
    let app = App::new();
    let old = app.add("2025-08-01T10:00:00Z", 3600);
    let result =
        app.rate(json!({"rate":"80","currency":"USD","effectiveAt":"2025-08-10T00:00:00Z"}));
    assert_eq!(result["hourlyRate"], "80.00");
    assert_eq!(result["rateSource"], "task");
    assert!(app.state().billing.entries[&old].rate.is_none());
    let new = app.add("2025-08-15T10:00:00Z", 3600);
    assert_eq!(
        app.state().billing.entries[&new]
            .rate
            .as_ref()
            .unwrap()
            .estimate(3600)
            .amount_text,
        "USD 80.00"
    );
    let other = call(
        &app.path,
        "task.create",
        json!({"project":app.project,"title":"Other"}),
    );
    assert!(other["rate"].is_null());
    assert_eq!(other["rateSource"], "project");
    assert!(
        call(&app.path, "project.get", json!({"project":app.project}))["project"]["rate"].is_null()
    );
    assert_eq!(app.draft()["totalMinor"], "8000");
    assert_eq!(app.draft()["excluded"]["nonBillableSeconds"], 3600);
}

#[test]
fn explicit_backfill_prices_unrated_time_and_records_an_audit_without_changing_duration() {
    let app = App::new();
    let entry = app.add("2025-08-01T10:00:00Z", 3600);
    let before = serde_json::to_value(&app.state().entries).unwrap();
    let stale_draft = app.draft();
    let result = app.rate(
        json!({"rate":"50","currency":"USD","applyExisting":true,"reason":"Agreed task rate"}),
    );
    assert_eq!(result["rateChange"]["appliedEntryIds"], json!([entry]));
    let state = app.state();
    assert_eq!(serde_json::to_value(&state.entries).unwrap(), before);
    assert_eq!(state.billing.entries[&entry].revision, 1);
    assert_eq!(
        state.billing.task_rate_adjustments[0].reason,
        "Agreed task rate"
    );
    assert!(
        state.billing.task_rate_adjustments[0].previous_billing[&entry]
            .rate
            .is_none()
    );
    assert_eq!(app.draft()["totalMinor"], "5000");
    let failure = agent::execute(
        &app.path,
        "invoice.issue",
        json!({"id":stale_draft["id"],"revision":1,"date":"2025-09-01"}),
        None,
    )
    .unwrap_err();
    assert_eq!(agent::error(&failure)["error"]["code"], "STALE_DRAFT");
    let audit = call(
        &app.path,
        "entry.list",
        json!({"project":app.project,"id":entry}),
    );
    assert_eq!(
        audit["items"][0]["rateAdjustments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn backfill_skips_invoiced_priced_zero_rate_and_externally_billed_entries() {
    let app = App::new();
    let invoiced = app.add("2025-08-01T10:00:00Z", 3600);
    app.rate(json!({"rate":"80","currency":"USD","applyExisting":true}));
    call(
        &app.path,
        "issuer.set",
        json!({"details":{"name":"Studio"}}),
    );
    omatracker::update_project(
        &app.path,
        &app.project,
        omatracker::ProjectChanges {
            client_name: Some("Client".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let draft = app.draft();
    let invoice = call(
        &app.path,
        "invoice.issue",
        json!({"id":draft["id"],"revision":1,"date":"2025-09-01"}),
    );
    let eligible = app.add("2025-08-02T10:00:00Z", 3600);
    app.rate(json!({"rate":"0","currency":"USD","effectiveAt":"2025-08-03T00:00:00Z"}));
    let zero = app.add("2025-08-04T10:00:00Z", 3600);
    let external = app.add("2025-08-02T12:00:00Z", 3600);
    let mut state = app.state();
    state
        .billing
        .entries
        .get_mut(&external)
        .unwrap()
        .externally_billed = true;
    fs::write(&app.path, serde_json::to_vec(&state).unwrap()).unwrap();
    let result = app.rate(json!({"rate":"100","currency":"USD","applyExisting":true}));
    assert_eq!(result["rateChange"]["appliedEntryIds"], json!([eligible]));
    assert_eq!(
        result["rateChange"]["skipped"],
        json!({"invoiced":1,"alreadyRated":1,"externallyBilled":1})
    );
    let state = app.state();
    assert_eq!(
        state.billing.entries[&invoiced]
            .rate
            .as_ref()
            .unwrap()
            .estimate(3600)
            .amount_text,
        "USD 80.00"
    );
    assert_eq!(
        state.billing.entries[&zero]
            .rate
            .as_ref()
            .unwrap()
            .estimate(3600)
            .amount_text,
        "USD 0.00"
    );
    assert!(state.billing.entries[&external].rate.is_none());
    assert_eq!(
        call(&app.path, "invoice.get", json!({"id":invoice["id"]}))["totalMinor"],
        "8000"
    );
}

#[test]
fn rate_timeline_splits_entries_and_can_return_to_project_inheritance_or_no_rate() {
    let app = App::new();
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"rate":"100","currency":"USD","effectiveAt":"2025-08-01T00:00:00Z"}),
    );
    app.rate(json!({"rate":"80","currency":"USD","effectiveAt":"2025-08-05T00:00:00Z"}));
    app.add("2025-08-04T23:00:00Z", 7200);
    app.rate(json!({"inheritRate":true,"effectiveAt":"2025-08-06T00:00:00Z"}));
    app.add("2025-08-07T10:00:00Z", 3600);
    app.rate(json!({"noRate":true,"effectiveAt":"2025-08-08T00:00:00Z"}));
    let no_rate = app.add("2025-08-09T10:00:00Z", 3600);
    let state = app.state();
    assert_eq!(state.entries.len(), 4);
    let amounts: Vec<_> = state
        .entries
        .iter()
        .map(|e| {
            state.billing.entries[&e.id]
                .rate
                .as_ref()
                .map(|r| r.estimate(e.seconds).amount_text)
        })
        .collect();
    assert_eq!(
        amounts,
        vec![
            Some("USD 100.00".into()),
            Some("USD 80.00".into()),
            Some("USD 100.00".into()),
            None
        ]
    );
    assert!(state.billing.entries[&no_rate].resolved);
    assert_eq!(app.draft()["totalMinor"], "28000");
}

#[test]
fn running_unrated_time_can_be_backfilled_without_stopping_the_timer() {
    let app = App::new();
    call(&app.path, "task.start", json!({"id":app.task}));
    let mut state = app.state();
    state.tasks[0].started_at = omatracker::now_ms() - 3_600_000;
    fs::write(&app.path, serde_json::to_vec(&state).unwrap()).unwrap();
    let result = app.rate(json!({"rate":"50","currency":"USD","applyExisting":true}));
    assert_eq!(result["status"], "tracking");
    assert_eq!(
        result["rateChange"]["appliedEntryIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let state = app.state();
    assert_eq!(state.tasks[0].status, TaskStatus::Tracking);
    assert!(state.entries[0].seconds >= 3600);
    assert!(state.tasks[0].started_at >= state.entries[0].ended_at);
    assert_eq!(
        state.billing.entries[&state.entries[0].id]
            .rate
            .as_ref()
            .unwrap()
            .currency(),
        "USD"
    );
}

#[test]
fn task_editor_update_is_atomic_and_presentation_exposes_effective_rate_and_token() {
    let app = App::new();
    app.add("2025-08-01T10:00:00Z", 3600);
    let before = fs::read(&app.path).unwrap();
    let invalid = agent::execute(
        &app.path,
        "task.update",
        json!({"id":app.task,"title":"Renamed","add":"1h","rate":"bad","currency":"USD","applyExisting":true}),
        None,
    );
    assert!(invalid.is_err());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    let updated = call(
        &app.path,
        "task.update",
        json!({"id":app.task,"title":"Renamed","add":"1h","rate":"80","currency":"USD","applyExisting":true}),
    );
    assert_eq!(
        updated["rateChange"]["appliedEntryIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    omatracker::select_project(&app.path, &app.project).unwrap();
    let status = serde_json::to_value(omatracker::presentation_status(&app.path).unwrap()).unwrap();
    assert_eq!(status["activeTasks"][0]["hourlyRate"], "80.00");
    assert_eq!(status["activeTasks"][0]["rateSource"], "task");
    assert_eq!(
        status["activeTasks"][0]["entityRevision"],
        updated["entityRevision"]
    );
    for input in [
        json!({"noRate":true,"applyExisting":true}),
        json!({"rate":"80","currency":"USD","inheritRate":true}),
    ] {
        let mut input = input;
        input["id"] = json!(app.task);
        let before = fs::read(&app.path).unwrap();
        assert!(agent::execute(&app.path, "task.rate", input, None).is_err());
        assert_eq!(fs::read(&app.path).unwrap(), before);
    }
}

#[test]
fn legacy_upgrade_preserves_billing_and_uses_a_separate_backup() {
    let app = App::new();
    app.add("2025-08-01T10:00:00Z", 3600);
    let mut old = serde_json::to_value(app.state()).unwrap();
    old["version"] = json!(3);
    old["billing"]["migrationLog"] = json!([{"action":"upgrade","at":1}]);
    old["billing"].as_object_mut().unwrap().remove("taskRates");
    old["billing"]
        .as_object_mut()
        .unwrap()
        .remove("taskRateAdjustments");
    let original = serde_json::to_vec_pretty(&old).unwrap();
    fs::write(&app.path, &original).unwrap();
    let invoice_backup = PathBuf::from(format!("{}.pre-invoices.bak", app.path.display()));
    fs::write(&invoice_backup, b"{\"version\":2}").unwrap();
    let result = call(&app.path, "migration.apply", json!({}));
    let backup = PathBuf::from(format!("{}.pre-task-rates.bak", app.path.display()));
    assert_eq!(result["backup"], json!(backup));
    assert_eq!(fs::read(&backup).unwrap(), original);
    assert_eq!(fs::read(&invoice_backup).unwrap(), b"{\"version\":2}");
    let state = app.state();
    assert_eq!(state.version, STATE_VERSION);
    let bytes = fs::read(&app.path).unwrap();
    call(&app.path, "migration.apply", json!({}));
    assert_eq!(fs::read(&app.path).unwrap(), bytes);
    assert_eq!(
        serde_json::to_value(&state.entries).unwrap(),
        old["entries"]
    );
    assert_eq!(
        serde_json::to_value(&state.billing.projects).unwrap(),
        old["billing"]["projects"]
    );
    app.rate(json!({"rate":"50","currency":"USD","applyExisting":true}));
    assert_eq!(fs::read(&backup).unwrap(), original);
}
