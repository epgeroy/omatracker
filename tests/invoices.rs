use omatracker::{agent, billing, parse_state};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct App {
    dir: tempfile::TempDir,
    path: PathBuf,
    project: String,
    task: String,
}
impl App {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let project = call(&path,"project.create",json!({"name":"Design","rate":"80","currency":"USD","effectiveAt":"2025-01-01T00:00:00Z"}))["id"].as_str().unwrap().to_owned();
        let task = call(
            &path,
            "task.create",
            json!({"project":project,"title":"Review"}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &path,
            "issuer.set",
            json!({"details":{"name":"Studio","paymentInstructions":"Bank transfer"}}),
        );
        let client = call(
            &path,
            "client.set",
            json!({"details":{"name":"Acme","address":"Test address"}}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &path,
            "project.configure",
            json!({"project":project,"client":client}),
        );
        Self {
            dir,
            path,
            project,
            task,
        }
    }
    fn add(&self, start: &str, seconds: i64) -> Value {
        call(
            &self.path,
            "entry.add",
            json!({"id":self.task,"start":start,"seconds":seconds}),
        )
    }
    fn draft(&self) -> Value {
        call(
            &self.path,
            "invoice.create",
            json!({"project":self.project,"from":"2025-08-01","to":"2025-09-01","currency":"USD"}),
        )
    }
    fn issue(&self, inv: &Value) -> Value {
        call(
            &self.path,
            "invoice.issue",
            json!({"id":inv["id"],"revision":inv["revision"],"date":"2025-09-01"}),
        )
    }
}
fn call(path: &Path, action: &str, input: Value) -> Value {
    agent::execute(path, action, input, None).unwrap_or_else(|e| panic!("{action}: {e:#}"))["data"]
        .clone()
}
fn fails(path: &Path, action: &str, input: Value, expected: &str) {
    let error = agent::execute(path, action, input, None).unwrap_err();
    assert!(
        format!("{error:#}").contains(expected),
        "{action}: {error:#}"
    );
}

#[test]
fn historical_rates_split_sessions_and_no_rate_is_distinct_from_zero() {
    let app = App::new();
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"rate":"100","currency":"USD","effectiveAt":"2025-08-15T12:00:00Z"}),
    );
    let entries = app.add("2025-08-15T11:00:00Z", 7200);
    assert_eq!(entries["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        entries["entries"][0]["billing"]["rate"]["amountMinor"],
        8000
    );
    assert_eq!(
        entries["entries"][1]["billing"]["rate"]["amountMinor"],
        10000
    );
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"noRate":true,"effectiveAt":"2025-08-16T00:00:00Z"}),
    );
    app.add("2025-08-16T10:00:00Z", 3600);
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"rate":"0","currency":"USD","effectiveAt":"2025-08-17T00:00:00Z"}),
    );
    app.add("2025-08-17T10:00:00Z", 3600);
    let inv = app.draft();
    assert_eq!(inv["totalMinor"], "18000");
    assert_eq!(inv["totalSeconds"], 10800);
    assert_eq!(inv["excluded"]["nonBillableSeconds"], 3600);
    assert_eq!(inv["lines"].as_array().unwrap().len(), 3);
}

#[test]
fn correction_undo_zero_and_issued_invoice_protection() {
    let app = App::new();
    let entry = app.add("2025-08-10T10:00:00Z", 3600)["entries"][0]["entry"]["id"].clone();
    let correction = call(
        &app.path,
        "entry.correct",
        json!({"id":entry,"revision":0,"delta":-3600,"reason":"Mistaken timer"}),
    );
    assert_eq!(app.draft()["totalSeconds"], 0);
    let state = parse_state(&fs::read_to_string(&app.path).unwrap()).unwrap();
    assert_eq!(state.entries[0].seconds, 0); // zero must survive normalization, not regain its wall-clock duration
    fails(
        &app.path,
        "entry.correct",
        json!({"id":entry,"revision":1,"delta":-1,"reason":"Too far"}),
        "negative",
    );
    call(
        &app.path,
        "entry.undo",
        json!({"id":correction["id"],"revision":1,"reason":"Restore"}),
    );
    fails(
        &app.path,
        "entry.undo",
        json!({"id":correction["id"],"revision":2,"reason":"Again"}),
        "already reversed",
    );
    let issued = app.issue(&app.draft());
    fails(
        &app.path,
        "entry.correct",
        json!({"id":entry,"revision":2,"delta":-1800,"reason":"Fix"}),
        "ENTRY_INVOICED",
    );
    let void = call(
        &app.path,
        "invoice.void",
        json!({"id":issued["id"],"revision":issued["revision"],"reason":"Correct time"}),
    );
    call(
        &app.path,
        "entry.correct",
        json!({"id":entry,"revision":2,"delta":-1800,"reason":"Fix"}),
    );
    let replacement = call(
        &app.path,
        "invoice.reissue",
        json!({"id":void["id"],"revision":void["revision"]}),
    );
    assert_eq!(replacement["replaces"], issued["id"]);
    assert_eq!(replacement["totalMinor"], "4000");
    assert_eq!(
        call(&app.path, "invoice.get", json!({"id":issued["id"]}))["totalMinor"],
        "8000"
    );
}

#[test]
fn overlapping_periods_allocate_only_remaining_time_and_stale_drafts_fail() {
    let app = App::new();
    app.add("2025-08-15T23:00:00Z", 7200);
    let early = call(
        &app.path,
        "invoice.create",
        json!({"project":app.project,"from":"2025-08-01","to":"2025-08-16","currency":"USD"}),
    );
    let all = app.draft();
    app.issue(&early);
    fails(
        &app.path,
        "invoice.issue",
        json!({"id":all["id"],"revision":1,"date":"2025-09-01"}),
        "STALE_DRAFT",
    );
    let refreshed = call(
        &app.path,
        "invoice.refresh",
        json!({"id":all["id"],"revision":1}),
    );
    assert_eq!(refreshed["totalSeconds"], 3600);
    assert_eq!(refreshed["excluded"]["alreadyBilledSeconds"], 3600);
    app.issue(&refreshed);
    assert_eq!(app.draft()["totalSeconds"], 0);
}

#[test]
fn idempotency_receipts_and_parallel_issuance_are_atomic() {
    let app = App::new();
    let input = json!({"id":app.task,"start":"2025-08-10T10:00:00Z","seconds":3600});
    let first = agent::execute(&app.path, "entry.add", input.clone(), Some("entry-one")).unwrap();
    let retry = agent::execute(&app.path, "entry.add", input, Some("entry-one")).unwrap();
    assert_eq!(first["data"], retry["data"]);
    assert_eq!(retry["replayed"], true);
    assert!(
        agent::execute(
            &app.path,
            "entry.add",
            json!({"seconds":1}),
            Some("entry-one")
        )
        .unwrap_err()
        .to_string()
        .contains("IDEMPOTENCY_CONFLICT")
    );
    let one = app.draft();
    let two = app.draft();
    let path = app.path.clone();
    let thread = std::thread::spawn(move || {
        agent::execute(
            &path,
            "invoice.issue",
            json!({"id":one["id"],"revision":1,"date":"2025-09-01"}),
            Some("issue-one"),
        )
    });
    let second = agent::execute(
        &app.path,
        "invoice.issue",
        json!({"id":two["id"],"revision":1,"date":"2025-09-01"}),
        Some("issue-two"),
    );
    let first = thread.join().unwrap();
    assert_ne!(first.is_ok(), second.is_ok());
    let state = parse_state(&fs::read_to_string(&app.path).unwrap()).unwrap();
    assert_eq!(state.billing.sequences["2025"], 1);
    assert_eq!(
        state
            .billing
            .invoices
            .iter()
            .filter(|i| i.state == "issued")
            .count(),
        1
    );
}

#[test]
fn migration_requires_explicit_rate_and_preserves_original_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.json");
    let old = json!({"version":2,"activeProjectId":"p","projects":[{"id":"p","name":"Old","rate":{"amountMinor":8000,"currency":"USD"}}],
        "tasks":[{"id":"t","projectId":"p","title":"Old","legacySeconds":100}],
        "entries":[{"id":"e","projectId":"p","taskId":"t","taskTitle":"Old","startedAt":1754042400000_i64,"endedAt":1754046000000_i64,"seconds":3600}]});
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        call(&path, "migration.preview", json!({}))["unresolvedEntries"],
        1
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    call(&path, "migration.apply", json!({}));
    let backup = PathBuf::from(format!("{}.pre-invoices.bak", path.display()));
    assert_eq!(fs::read(&backup).unwrap(), bytes);
    call(
        &path,
        "migration.resolve",
        json!({"project":"p","from":"2025-08-01","to":"2025-09-01","rate":"70","currency":"USD","externallyBilled":true}),
    );
    assert_eq!(
        call(&path, "migration.preview", json!({}))["unresolvedEntries"],
        0
    );
    assert_eq!(
        call(
            &path,
            "invoice.create",
            json!({"project":"p","from":"2025-08-01","to":"2025-09-01","currency":"USD"})
        )["excluded"]["alreadyBilledSeconds"],
        3600
    );
    assert_eq!(fs::read(&backup).unwrap(), bytes);
    assert_eq!(
        parse_state(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .tasks[0]
            .legacy_seconds,
        100
    );
    assert!(parse_state(r#"{"version":999}"#).is_err());
}

#[test]
fn schedule_is_draft_only_monthly_idempotent_and_handles_late_entries() {
    let app = App::new();
    app.add("2025-08-10T10:00:00Z", 3600);
    let scheduled = call(&app.path, "invoice.check", json!({}));
    assert_eq!(scheduled["created"].as_array().unwrap().len(), 1);
    assert!(
        call(&app.path, "invoice.check", json!({}))["created"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = &scheduled["created"][0];
    let draft = call(&app.path, "invoice.get", json!({"id":id}));
    assert_eq!(draft["state"], "draft");
    assert_eq!(draft["number"], "");
    assert_eq!(draft["remotePath"], "");
    app.issue(&draft);
    app.add("2025-08-11T10:00:00Z", 1800);
    let late = call(&app.path, "invoice.check", json!({}));
    assert_eq!(late["created"].as_array().unwrap().len(), 1);
    assert_eq!(
        call(&app.path, "invoice.get", json!({"id":late["created"][0]}))["totalSeconds"],
        1800
    );
}

#[test]
fn currency_precision_and_dst_boundaries_are_explicit() {
    let (from, to) = billing::bounds("2025-03-30", "2025-03-31", "Europe/London").unwrap();
    assert_eq!((to - from) / 3600000, 23);
    let (from, to) = billing::bounds("2025-10-26", "2025-10-27", "Europe/London").unwrap();
    assert_eq!((to - from) / 3600000, 25);
    assert!(billing::bounds("2025-01-01", "2025-01-01", "UTC").is_err());
    let app = App::new();
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"rate":"125","currency":"JPY","effectiveAt":"2025-08-01T00:00:00Z"}),
    );
    app.add("2025-08-10T10:00:00Z", 1800);
    call(
        &app.path,
        "project.rate",
        json!({"project":app.project,"rate":"1.001","currency":"KWD","effectiveAt":"2025-08-11T00:00:00Z"}),
    );
    app.add("2025-08-11T10:00:00Z", 1800);
    let summary = call(
        &app.path,
        "summary",
        json!({"project":app.project,"from":"2025-08-01","to":"2025-09-01"}),
    );
    assert_eq!(summary["uninvoiced"][0]["amountText"], "JPY 63");
    assert_eq!(summary["uninvoiced"][1]["amountText"], "KWD 0.501");
}

#[test]
fn issued_bundles_and_rebuilt_pdf_survive_source_changes() {
    if Command::new("typst").arg("--version").output().is_err() {
        return;
    }
    let app = App::new();
    app.add("2025-08-10T10:00:00Z", 3600);
    let issued = app.issue(&app.draft());
    let bundle = Path::new(issued["bundle"].as_str().unwrap());
    let snapshot = fs::read(bundle.join("data.json")).unwrap();
    let id = issued["id"].as_str().unwrap();
    let pdf = billing::render(&app.path, id, false).unwrap();
    assert!(
        fs::read(pdf["path"].as_str().unwrap())
            .unwrap()
            .starts_with(b"%PDF")
    );
    call(
        &app.path,
        "issuer.set",
        json!({"details":{"name":"Changed Studio"}}),
    );
    fs::remove_file(pdf["path"].as_str().unwrap()).unwrap();
    billing::render(&app.path, id, false).unwrap();
    assert_eq!(fs::read(bundle.join("data.json")).unwrap(), snapshot);
    assert!(bundle.starts_with(app.dir.path()));
    assert_eq!(
        call(&app.path, "invoice.get", json!({"id":id}))["issuer"]["name"],
        "Studio"
    );
}

#[test]
fn cli_contract_reports_errors_and_does_not_change_ui_selection() {
    let app = App::new();
    let before = parse_state(&fs::read_to_string(&app.path).unwrap())
        .unwrap()
        .active_project_id;
    let result = Command::new(env!("CARGO_BIN_EXE_omatracker"))
        .arg("--data-path")
        .arg(&app.path)
        .args([
            "agent",
            "project.create",
            "--input",
            r#"{"name":"Other"}"#,
            "--key",
            "new-project",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["schemaVersion"], 1);
    assert_eq!(
        parse_state(&fs::read_to_string(&app.path).unwrap())
            .unwrap()
            .active_project_id,
        before
    );
    let result = Command::new(env!("CARGO_BIN_EXE_omatracker"))
        .arg("--data-path")
        .arg(&app.path)
        .args(["agent", "task.start", "--input", r#"{"id":"missing"}"#])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(error["error"]["code"], "TASK_NOT_FOUND");
    fails(
        &app.path,
        "project.create",
        json!({"naem":"typo"}),
        "INVALID_INPUT",
    );
}

#[test]
fn upload_failure_preserves_pdf_and_retry_key_pins_destination() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("typst").arg("--version").output().is_err() {
        return;
    }
    let app = App::new();
    app.add("2025-08-10T10:00:00Z", 3600);
    let issued = app.issue(&app.draft());
    call(
        &app.path,
        "drive.configure",
        json!({"remote":"test","driveFolder":"Invoices"}),
    );
    let bin = app.dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let rclone = bin.join("rclone");
    fs::write(
        &rclone,
        r#"#!/bin/sh
if [ "$1" = --version ]; then exit 0; fi
printf attempt >> "$HOME/attempts"
if [ ! -f "$HOME/allow-upload" ]; then printf 'offline' >&2; exit 1; fi
/bin/cp "$5" "$HOME/uploaded.pdf"
printf '%s' "$6" > "$HOME/destination"
"#,
    )
    .unwrap();
    fs::set_permissions(&rclone, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let path = std::env::join_paths(paths).unwrap();
    let input = json!({"id":issued["id"]}).to_string();
    let upload = || {
        Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .env("HOME", app.dir.path())
            .env("PATH", &path)
            .arg("--data-path")
            .arg(&app.path)
            .args([
                "agent",
                "invoice.upload",
                "--input",
                &input,
                "--key",
                "upload-once",
            ])
            .output()
            .unwrap()
    };
    let first = upload();
    assert!(!first.status.success());
    let failed = call(&app.path, "invoice.get", json!({"id":issued["id"]}));
    assert_eq!(failed["uploadStatus"], "failed");
    assert_eq!(failed["renderStatus"], "complete");
    let original = fs::read(issued["pdfPath"].as_str().unwrap()).unwrap();
    // A configuration change must not redirect an in-flight invoice retry.
    call(
        &app.path,
        "drive.configure",
        json!({"remote":"other","driveFolder":"Different"}),
    );
    fs::write(app.dir.path().join("allow-upload"), b"yes").unwrap();
    assert!(upload().status.success());
    let replay: Value = serde_json::from_slice(&upload().stdout).unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        fs::read(app.dir.path().join("attempts")).unwrap(),
        b"attemptattempt"
    );
    assert_eq!(
        fs::read(app.dir.path().join("uploaded.pdf")).unwrap(),
        original
    );
    assert!(
        fs::read_to_string(app.dir.path().join("destination"))
            .unwrap()
            .starts_with("test:Invoices/")
    );
}

#[test]
fn custom_invoice_assets_are_managed_and_captured() {
    if Command::new("typst").arg("--version").output().is_err() {
        return;
    }
    let app = App::new();
    let cli = |action: &str, input: Value| -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .env("HOME", app.dir.path())
            .env("XDG_CONFIG_HOME", app.dir.path().join("config"))
            .env(
                "OMATRACKER_TEMPLATE_DIR",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            )
            .arg("--data-path")
            .arg(&app.path)
            .args(["agent", action, "--input", &input.to_string()])
            .output()
            .unwrap();
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{value}");
        value["data"].clone()
    };
    let source = app.dir.path().join("logo.svg");
    fs::write(&source,r##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="30"><rect width="30" height="30" fill="#336699"/></svg>"##).unwrap();
    let template = cli("template.create", json!({"name":"branded"}));
    let asset = cli(
        "template.asset",
        json!({"id":template["id"],"source":source}),
    );
    assert!(Path::new(asset["path"].as_str().unwrap()).is_file());
    cli(
        "project.configure",
        json!({"project":app.project,"template":template["id"],"logo":source}),
    );
    fs::remove_file(&source).unwrap(); // managed logo copy must remain usable
    app.add("2025-08-10T10:00:00Z", 3600);
    let issued = cli(
        "invoice.issue",
        json!({"id":app.draft()["id"],"revision":1,"date":"2025-09-01"}),
    );
    let capture = Path::new(issued["bundle"].as_str().unwrap());
    assert!(capture.join("project-logo.svg").is_file());
    assert!(
        capture
            .join("template")
            .join(asset["reference"].as_str().unwrap())
            .is_file()
    );
    fs::remove_dir_all(
        Path::new(template["path"].as_str().unwrap())
            .parent()
            .unwrap(),
    )
    .unwrap();
    let pdf = cli("invoice.render", json!({"id":issued["id"]}));
    assert!(
        fs::read(pdf["path"].as_str().unwrap())
            .unwrap()
            .starts_with(b"%PDF")
    );
}
