use omatracker::{State, agent};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

struct App {
    _home: tempfile::TempDir,
    path: PathBuf,
    project: String,
}
impl App {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("ledger.json");
        let project = agent::execute(
            &path,
            "project.create",
            json!({"name":"Batch project"}),
            None,
        )
        .unwrap()["data"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            _home: home,
            path,
            project,
        }
    }
    fn request(&self) -> Value {
        json!({"project":self.project,"items":[{"ref":"first","newTask":{"title":"First"},
            "entries":[{"start":"2026-09-01T09:00:00Z","end":"2026-09-01T13:00:00Z"}]}],
            "pricing":{"mode":"explicit","rate":"50","currency":"USD"},
            "summary":{"from":"2026-09-01","to":"2026-09-16"}})
    }
    fn batch(&self, input: Value, key: &str) -> Value {
        agent::execute(&self.path, "work.record-batch", input, Some(key)).unwrap()
    }
    fn state(&self) -> State {
        omatracker::parse_state(&fs::read_to_string(&self.path).unwrap()).unwrap()
    }
    fn rate(&self, rate: &str, at: &str) {
        agent::execute(
            &self.path,
            "project.rate",
            json!({"project":self.project,"rate":rate,
            "currency":"USD","effectiveAt":at}),
            None,
        )
        .unwrap();
    }
}

fn error(path: &Path, input: Value, key: Option<&str>) -> Value {
    agent::error(&agent::execute(path, "work.record-batch", input, key).unwrap_err())
}

#[test]
fn six_september_tasks_record_24_hours_and_1200_usd_in_one_call() {
    let app = App::new();
    let mut request = app.request();
    request["items"] = json!([1, 3, 5, 9, 12, 15].map(|day| json!({"ref":format!("task-{day}"),
        "newTask":{"title":format!("September task {day}")},"entries":[{
        "start":format!("2026-09-{day:02}T09:00:00Z"),"end":format!("2026-09-{day:02}T13:00:00Z")}]})));
    let started = Instant::now();
    let result = app.batch(request.clone(), "september-six");
    eprintln!(
        "six-task batch: one execution call, 12 internal mutations, helper wall time {:?}; model latency not measured",
        started.elapsed()
    );
    assert_eq!(result["data"]["status"], "completed");
    assert_eq!(result["data"]["summary"]["recordedSeconds"], 86400);
    assert_eq!(
        result["data"]["summary"]["uninvoiced"][0]["amountMinor"],
        "120000"
    );
    assert_eq!(
        result["data"]["summary"]["excluded"]["nonBillableSeconds"],
        0
    );
    assert_eq!(
        result["data"]["summary"]["excluded"]["unresolvedSeconds"],
        0
    );
    let state = app.state();
    assert_eq!(state.tasks.len(), 6);
    assert_eq!(state.entries.len(), 6);
    assert!(state.billing.task_rates.is_empty());
    assert!(state.billing.task_rate_adjustments.is_empty());
    assert!(
        state
            .projects
            .iter()
            .find(|p| p.id == app.project)
            .unwrap()
            .rate
            .is_none()
    );
    let bytes = fs::read(&app.path).unwrap();
    let replay = app.batch(request.clone(), "september-six");
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["changed"], false);
    assert_eq!(replay["data"], result["data"]);
    assert_eq!(fs::read(&app.path).unwrap(), bytes);
    request["items"][0]["newTask"]["title"] = json!("Changed intent");
    assert_eq!(
        error(&app.path, request, Some("september-six"))["error"]["code"],
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn malformed_last_item_and_invalid_contracts_fail_before_any_mutation() {
    let app = App::new();
    let original = fs::read(&app.path).unwrap();
    let mut cases = Vec::new();
    for (pointer, value) in [
        ("/items/0/entries/0/start", json!("not-a-date")),
        ("/items/0/entries/0/end", json!("2026-09-01T08:00:00Z")),
        ("/items/0/entries/0/end", json!("2099-09-01T13:00:00Z")),
        ("/items/0/entries/0/start", json!("2026-09-01T09:00:00.1Z")),
        ("/items/0/entries/0/start", json!("2026-07-01T09:00:00Z")),
        ("/items/0/newTask/title", json!("  ")),
        ("/items/0/newTask/title", json!("a".repeat(161))),
        ("/items/0/newTask/title", json!("two\nlines")),
        ("/items/0/ref", json!("bad ref")),
        ("/items/0/entries", json!([])),
        ("/items", json!([])),
        ("/pricing", json!({"mode":"explicit","rate":"50"})),
        (
            "/pricing",
            json!({"mode":"explicit","rate":"-1","currency":"USD"}),
        ),
        (
            "/pricing",
            json!({"mode":"historical-inheritance","rate":"50","currency":"USD"}),
        ),
        ("/summary/to", json!("2026-08-01")),
        ("/project", json!("missing-project")),
    ] {
        let mut input = app.request();
        *input.pointer_mut(pointer).unwrap() = value;
        cases.push(input);
    }
    let mut missing = app.request();
    missing.as_object_mut().unwrap().remove("pricing");
    cases.push(missing);
    let mut duplicate = app.request();
    duplicate["items"] = json!([duplicate["items"][0], duplicate["items"][0]]);
    cases.push(duplicate);
    let mut bad_last = app.request();
    let mut last = bad_last["items"][0].clone();
    last["ref"] = json!("last");
    last["entries"][0]["end"] = json!("bad");
    bad_last["items"].as_array_mut().unwrap().push(last);
    cases.push(bad_last);
    let mut too_many = app.request();
    too_many["items"] = json!(
        (0..51)
            .map(|i| {
                let mut item = app.request()["items"][0].clone();
                item["ref"] = json!(format!("item-{i}"));
                item
            })
            .collect::<Vec<_>>()
    );
    cases.push(too_many);
    let mut note = app.request();
    note["items"][0]["entries"][0]["note"] = json!("n".repeat(241));
    cases.push(note);
    for (field, value) in [
        ("unexpected", json!(true)),
        ("revision", json!(1)),
        ("repository", json!("/tmp")),
    ] {
        let mut input = app.request();
        input[field] = value;
        cases.push(input);
    }
    let mut too_many = app.request();
    too_many["items"][0]["entries"] = json!(vec![too_many["items"][0]["entries"][0].clone(); 201]);
    cases.push(too_many);
    for input in cases {
        let failure = error(&app.path, input, Some("invalid"));
        assert_eq!(failure["ok"], false);
        assert_eq!(fs::read(&app.path).unwrap(), original);
    }
    assert_eq!(
        error(&app.path, app.request(), None)["error"]["code"],
        "INVALID_INPUT"
    );
}

#[test]
fn dry_run_surfaces_unknown_history_and_does_not_reserve_keys_or_write_ledger() {
    let app = App::new();
    let mut state = app.state();
    state
        .billing
        .projects
        .get_mut(&app.project)
        .unwrap()
        .rates
        .clear();
    fs::write(&app.path, serde_json::to_vec(&state).unwrap()).unwrap();
    let before = fs::read(&app.path).unwrap();
    let mut input = app.request();
    input["pricing"] = json!({"mode":"historical-inheritance"});
    input["dryRun"] = json!(true);
    let result = app.batch(input.clone(), "dry-run");
    assert_eq!(result["changed"], false);
    assert_eq!(result["data"]["status"], "validated");
    assert_eq!(
        result["data"]["steps"][1]["billingSegments"][0]["billing"]["resolved"],
        false
    );
    assert_eq!(
        result["data"]["summary"]["excluded"]["unresolvedSeconds"],
        14400
    );
    assert_eq!(fs::read(&app.path).unwrap(), before);
    assert!(!PathBuf::from(format!("{}.workflows", app.path.display())).exists());
    input["dryRun"] = json!(false);
    let result = app.batch(input, "dry-run");
    assert_eq!(
        result["data"]["summary"]["excluded"]["unresolvedSeconds"],
        14400
    );
}

#[test]
fn inherited_mixed_zero_and_missing_rates_keep_native_boundary_accounting() {
    let app = App::new();
    app.rate("20", "2026-09-01T10:00:00Z");
    app.rate("0", "2026-09-01T11:00:00Z");
    app.rate("40", "2026-09-01T12:00:00Z");
    let mut input = app.request();
    input["pricing"] = json!({"mode":"historical-inheritance"});
    let result = app.batch(input, "history");
    let summary = &result["data"]["summary"];
    assert_eq!(summary["recordedSeconds"], 14400);
    assert_eq!(summary["excluded"]["nonBillableSeconds"], 3600);
    assert_eq!(summary["uninvoiced"][0]["seconds"], 10800);
    assert_eq!(summary["uninvoiced"][0]["amountMinor"], "6000");
    assert_eq!(
        result["data"]["items"][0]["entries"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(app.state().billing.task_rates.is_empty());
}

#[test]
fn explicit_entry_pricing_overrides_history_but_leaves_future_work_inherited() {
    let app = App::new();
    app.rate("20", "2026-09-01T01:00:00Z");
    app.rate("0", "2026-09-01T11:00:00Z");
    let before = serde_json::to_value(app.state().billing.projects).unwrap();
    let result = app.batch(app.request(), "explicit");
    assert_eq!(
        result["data"]["summary"]["uninvoiced"][0]["amountMinor"],
        "20000"
    );
    assert_eq!(
        serde_json::to_value(app.state().billing.projects).unwrap(),
        before
    );
    let task = &result["data"]["items"][0]["taskId"];
    let future = agent::execute(
        &app.path,
        "entry.add",
        json!({"id":task,"start":"2026-09-02T09:00:00Z","seconds":3600}),
        None,
    )
    .unwrap();
    assert_eq!(
        future["data"]["entries"][0]["billing"]["rate"],
        json!(omatracker::HourlyRate::parse("0", "USD").unwrap())
    );
    let mut zero = app.request();
    zero["pricing"]["rate"] = json!("0");
    let result = app.batch(zero, "explicit-zero");
    for entry in result["data"]["items"][0]["entries"].as_array().unwrap() {
        assert!(!entry["billing"]["rate"].is_null());
    }
}

#[test]
fn copied_journal_and_removed_targets_are_never_resumed_as_new_work() {
    let app = App::new();
    let result = app.batch(app.request(), "original");
    let copied = app._home.path().join("copied.json");
    fs::copy(&app.path, &copied).unwrap();
    let before = fs::read(&copied).unwrap();
    assert_eq!(
        error(&copied, app.request(), Some("original"))["error"]["code"],
        "WORKFLOW_LEDGER_MISMATCH"
    );
    assert_eq!(fs::read(&copied).unwrap(), before);
    agent::execute(
        &app.path,
        "task.remove",
        json!({"id":result["data"]["items"][0]["taskId"]}),
        None,
    )
    .unwrap();
    let failure = error(&app.path, app.request(), Some("original"));
    assert_eq!(failure["error"]["code"], "REQUEST_TARGET_REMOVED");
    assert_eq!(failure["data"]["status"], "partial");
    assert!(app.state().tasks.is_empty());
}

#[test]
fn concurrent_identical_batches_and_cli_discovery_are_retry_safe() {
    let app = App::new();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| app.batch(app.request(), "concurrent")))
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap()["data"]["status"], "completed");
        }
    });
    assert_eq!(app.state().tasks.len(), 1);
    assert_eq!(app.state().entries.len(), 1);
    let help = agent::execute(&app.path, "help", json!({}), None).unwrap();
    assert!(
        help["data"]["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("work.record-batch"))
    );
    let output = Command::new(env!("CARGO_BIN_EXE_omatracker"))
        .args([
            "--data-path",
            app.path.to_str().unwrap(),
            "agent",
            "work.record-batch",
            "--input",
            &app.request().to_string(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output["error"]["code"], "INVALID_INPUT");
}
