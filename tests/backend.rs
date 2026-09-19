use chrono::{Local, NaiveDate, TimeZone};
use omatracker::{DEFAULT_PROJECT_ID, Entry, State, Task, last_completed_period, now_ms};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn cli_command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
    command
        .env("HOME", home)
        .env("PATH", home.join("bin"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env(
            "OMATRACKER_TEMPLATE_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
        )
        .arg("--data-path")
        .arg(home.join("state.json"));
    command
}

fn cli(home: &Path, args: &[&str]) -> String {
    let output = cli_command(home).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn state(home: &Path) -> Value {
    serde_json::from_slice(&fs::read(home.join("state.json")).unwrap()).unwrap()
}

#[test]
fn project_rates_round_trip_and_stay_project_specific() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let first = cli(home, &["project", "create", "USD project"])
        .trim()
        .to_owned();
    let task = cli(home, &["task", "add", "Design"]).trim().to_owned();
    cli(home, &["task", "edit", &task, "--add", "1h30m"]);
    cli(
        home,
        &[
            "project",
            "update",
            &first,
            "--hourly-rate",
            "80",
            "--currency",
            "usd",
        ],
    );
    let compact: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    let full: Value = serde_json::from_str(&cli(home, &["status", "--json"])).unwrap();
    assert_eq!(compact["activeProjectEstimate"]["amountText"], "USD 120.00");
    assert_eq!(
        compact["activeProjectEstimate"],
        full["activeProjectEstimate"]
    );
    assert_eq!(compact["activeProject"]["rate"]["amountMinor"], 8000);
    cli(home, &["project", "update", &first, "--name", "Renamed"]);
    cli(home, &["project", "update", &first, "--hourly-rate", "100"]);
    let updated: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(updated["activeProjectEstimate"]["amountText"], "USD 150.00");

    let second = cli(home, &["project", "create", "JPY project"])
        .trim()
        .to_owned();
    cli(
        home,
        &[
            "project",
            "update",
            &second,
            "--hourly-rate",
            "125",
            "--currency",
            "JPY",
        ],
    );
    let task = cli(home, &["task", "add", "Review"]).trim().to_owned();
    cli(home, &["task", "edit", &task, "--add", "30m"]);
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(current["activeProjectEstimate"]["amountText"], "JPY 63");
    cli(home, &["project", "select", &first]);
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(current["activeProjectEstimate"]["amountText"], "USD 150.00");
    cli(home, &["project", "update", &first, "--hourly-rate", "0"]);
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(current["activeProjectEstimate"]["amountText"], "USD 0.00");
    cli(home, &["project", "update", &first, "--clear-rate"]);
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert!(current["activeProject"]["rate"].is_null());
    assert!(current["activeProjectEstimate"].is_null());
    assert_eq!(current["activeProjectSeconds"], 5400);
}

#[test]
fn invalid_rate_updates_leave_the_ledger_untouched() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    cli(home, &["task", "add", "Existing"]);
    for flags in [
        vec!["--hourly-rate", "80"],
        vec!["--currency", "USD"],
        vec!["--hourly-rate", "80", "--currency", "USD", "--clear-rate"],
        vec!["--hourly-rate", "-1", "--currency", "USD"],
        vec!["--hourly-rate", "1.001", "--currency", "USD"],
        vec!["--hourly-rate", "80", "--currency", "XYZ"],
    ] {
        let before = fs::read(home.join("state.json")).unwrap();
        let output = cli_command(home)
            .args([
                "project",
                "update",
                DEFAULT_PROJECT_ID,
                "--name",
                "Must not save",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
        assert_eq!(before, fs::read(home.join("state.json")).unwrap());
    }
}

#[test]
fn old_ledgers_default_to_no_rate_and_keep_legacy_time() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    for ledger in [
        r#"{"version":1,"tasks":[{"id":"old","title":"Legacy","seconds":5400}]}"#,
        r#"{"version":2,"activeProjectId":"project-unassigned","projects":[{"id":"project-unassigned","name":"Old"}],"tasks":[{"id":"old","projectId":"project-unassigned","title":"Legacy","legacySeconds":5400}]}"#,
    ] {
        fs::write(home.join("state.json"), ledger).unwrap();
        let current: Value =
            serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
        assert!(current["activeProject"]["rate"].is_null());
        assert!(current["activeProjectEstimate"].is_null());
        cli(
            home,
            &[
                "project",
                "update",
                DEFAULT_PROJECT_ID,
                "--hourly-rate",
                "80",
                "--currency",
                "USD",
            ],
        );
        let current: Value =
            serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
        assert_eq!(current["activeProjectEstimate"]["amountText"], "USD 120.00");
        cli(home, &["report", "export", "weekly"]);
        let ledger = state(home);
        let snapshot: Value = serde_json::from_slice(
            &fs::read(ledger["reports"][0]["dataPath"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot["totalSeconds"], 0);
        assert_eq!(snapshot["estimate"]["amountText"], "USD 0.00");
    }
}

#[test]
fn rate_estimates_follow_running_time_and_reset_counters() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let mut ledger = State::default();
    ledger.projects[0].rate = Some(omatracker::HourlyRate::parse("3600", "USD").unwrap());
    ledger.tasks.push(Task {
        id: "running".into(),
        project_id: DEFAULT_PROJECT_ID.into(),
        running: true,
        started_at: now_ms() - 2000,
        ..Task::default()
    });
    fs::write(
        home.join("state.json"),
        serde_json::to_vec(&ledger).unwrap(),
    )
    .unwrap();
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert!(current["activeProjectSeconds"].as_i64().unwrap() >= 2);
    assert_eq!(
        current["activeProjectEstimate"]["amountMinor"],
        (current["activeProjectSeconds"].as_i64().unwrap() * 100).to_string()
    );
    cli(home, &["task", "stop", "running"]);
    cli(home, &["task", "reset", "running"]);
    let current: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(current["activeProjectEstimate"]["amountText"], "USD 0.00");
    assert!(!state(home)["entries"].as_array().unwrap().is_empty());
    cli(home, &["task", "remove", "running"]);
    assert!(!state(home)["entries"].as_array().unwrap().is_empty());
}

#[test]
fn queued_rate_snapshots_survive_rate_changes_and_retries() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    executable(
        &home.join("bin/typst"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nexit 1\n",
    );
    executable(&home.join("bin/rclone"), "#!/bin/sh\nexit 0\n");
    let mut ledger = State::default();
    ledger.drive.remote = "test".into();
    ledger.projects[0].rate = Some(omatracker::HourlyRate::parse("80", "USD").unwrap());
    let period = last_completed_period("weekly", now_ms()).unwrap();
    ledger.entries.push(Entry {
        id: "dated".into(),
        project_id: DEFAULT_PROJECT_ID.into(),
        started_at: period.start_at + 3600000,
        ended_at: period.start_at + 9000000,
        seconds: 5400,
        ..Entry::default()
    });
    fs::write(
        home.join("state.json"),
        serde_json::to_vec(&ledger).unwrap(),
    )
    .unwrap();
    cli(home, &["report", "export", "weekly"]);
    let first = state(home);
    assert_eq!(first["reports"][0]["status"], "failed");
    let data_path = first["reports"][0]["dataPath"].as_str().unwrap();
    let bytes = fs::read(data_path).unwrap();
    let snapshot: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(snapshot["project"]["rate"]["amountMinor"], 8000);
    assert_eq!(snapshot["estimate"]["amountText"], "USD 120.00");
    cli(
        home,
        &[
            "project",
            "update",
            DEFAULT_PROJECT_ID,
            "--hourly-rate",
            "100",
            "--currency",
            "EUR",
        ],
    );
    executable(
        &home.join("bin/typst"),
        "#!/bin/sh\nif [ \"$1\" != --version ]; then printf pdf > \"$5\"; fi\n",
    );
    cli(home, &["report", "retry"]);
    assert_eq!(state(home)["reports"][0]["status"], "complete");
    assert_eq!(bytes, fs::read(data_path).unwrap());
    cli(home, &["report", "export", "weekly"]);
    assert_eq!(bytes, fs::read(data_path).unwrap());
    assert_eq!(state(home)["reports"].as_array().unwrap().len(), 1);
}

#[test]
fn upload_retries_reuse_the_pdf_and_rebuild_a_missing_artifact() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    executable(
        &home.join("bin/typst"),
        r#"#!/bin/sh
if [ "$1" = --version ]; then exit 0; fi
printf 'render\n' >> "$HOME/renders"
printf 'pdf' > "$5"
"#,
    );
    executable(
        &home.join("bin/rclone"),
        r#"#!/bin/sh
if [ "$1" = --version ]; then exit 0; fi
if [ -f "$HOME/fail-upload" ]; then exit 1; fi
"#,
    );
    fs::write(home.join("fail-upload"), "").unwrap();
    cli(home, &["drive", "update", "--remote", "test"]);
    cli(home, &["report", "export", "weekly"]);
    assert_eq!(state(home)["reports"][0]["status"], "failed");
    assert_eq!(state(home)["reports"][0]["rendered"], true);

    cli(home, &["report", "retry"]);
    assert_eq!(
        fs::read_to_string(home.join("renders")).unwrap(),
        "render\n"
    );
    let pdf = state(home)["reports"][0]["pdfPath"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::remove_file(pdf).unwrap();
    cli(home, &["report", "retry"]);
    assert_eq!(
        fs::read_to_string(home.join("renders")).unwrap(),
        "render\nrender\n"
    );

    fs::remove_file(home.join("fail-upload")).unwrap();
    cli(home, &["report", "retry"]);
    assert_eq!(state(home)["reports"][0]["status"], "complete");
    assert_eq!(
        fs::read_to_string(home.join("renders")).unwrap(),
        "render\nrender\n"
    );
}

#[test]
fn compact_status_omits_history_and_never_launches_diagnostics() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    for name in ["typst", "rclone", "systemctl"] {
        executable(
            &home.join("bin").join(name),
            "#!/bin/sh\nprintf 'check\\n' >> \"$HOME/checks\"\n",
        );
    }
    let mut ledger = State::default();
    ledger.tasks.push(Task {
        id: "task".into(),
        project_id: DEFAULT_PROJECT_ID.into(),
        ..Task::default()
    });
    ledger.entries.push(Entry {
        id: "entry".into(),
        task_id: "task".into(),
        project_id: DEFAULT_PROJECT_ID.into(),
        started_at: 1000,
        ended_at: 61000,
        seconds: 60,
        ..Entry::default()
    });
    fs::write(
        home.join("state.json"),
        serde_json::to_vec(&ledger).unwrap(),
    )
    .unwrap();
    let compact: Value =
        serde_json::from_str(&cli(home, &["status", "--json", "--compact"])).unwrap();
    assert_eq!(compact["activeTasks"][0]["displaySeconds"], 60);
    assert_eq!(compact["totalTrackedSeconds"], 60);
    for key in ["entries", "reports", "tasks"] {
        assert!(compact["state"].get(key).is_none());
    }
    assert!(!home.join("checks").exists());

    let full: Value = serde_json::from_str(&cli(home, &["status", "--json"])).unwrap();
    assert_eq!(full["state"]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(full["activeTasks"], compact["activeTasks"]);
    assert!(full.get("dependencies").is_some());
    assert_eq!(
        fs::read_to_string(home.join("checks")).unwrap(),
        "check\ncheck\n"
    );
}

#[test]
fn report_checks_reach_recent_periods_and_find_late_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    executable(
        &home.join("bin/typst"),
        "#!/bin/sh\nif [ \"$1\" != --version ]; then printf pdf > \"$5\"; fi\n",
    );
    executable(&home.join("bin/rclone"), "#!/bin/sh\nexit 0\n");
    let mut ledger = State::default();
    ledger.drive.remote = "test".into();
    let recent = last_completed_period("monthly", now_ms()).unwrap().start_at;
    let date = |year, month, day| {
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(year, month, day)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .timestamp_millis()
    };
    for start in [date(2020, 1, 10), recent + 86400000] {
        ledger.entries.push(Entry {
            id: format!("entry-{start}"),
            project_id: DEFAULT_PROJECT_ID.into(),
            started_at: start,
            ended_at: start + 60000,
            seconds: 60,
            ..Entry::default()
        });
    }
    fs::write(
        home.join("state.json"),
        serde_json::to_vec(&ledger).unwrap(),
    )
    .unwrap();
    cli(home, &["report", "archive-check"]);
    let first = state(home);
    let reports = first["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|report| report["status"] == "complete"));
    assert!(
        reports
            .iter()
            .any(|report| report["period"] == "monthly" && report["startAt"] == recent)
    );
    let bytes = fs::read(home.join("state.json")).unwrap();
    cli(home, &["report", "archive-check"]);
    assert_eq!(fs::read(home.join("state.json")).unwrap(), bytes);

    let mut ledger: State = serde_json::from_value(first).unwrap();
    let start = date(2023, 2, 2);
    ledger.entries.push(Entry {
        id: "late-entry".into(),
        project_id: DEFAULT_PROJECT_ID.into(),
        started_at: start,
        ended_at: start + 60000,
        seconds: 60,
        ..Entry::default()
    });
    fs::write(
        home.join("state.json"),
        serde_json::to_vec(&ledger).unwrap(),
    )
    .unwrap();
    cli(home, &["report", "archive-check"]);
    assert_eq!(state(home)["reports"].as_array().unwrap().len(), 6);
}

#[test]
fn sync_uses_a_stable_snapshot_while_task_mutations_continue() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    executable(
        &home.join("bin/rclone"),
        r#"#!/bin/sh
set -eu
if [ "$1" = --version ]; then exit 0; fi
test "$7" != "$HOME/state.json"
/bin/cp "$7" "$HOME/uploaded.json"
printf ready > "$HOME/upload-ready"
i=0
while [ ! -f "$HOME/release-upload" ]; do
  i=$((i + 1))
  test "$i" -lt 500
  /bin/sleep 0.01
done
/bin/cmp "$7" "$HOME/uploaded.json"
"#,
    );
    cli(home, &["drive", "update", "--remote", "test"]);
    let id = cli(home, &["task", "add", "Before upload"])
        .trim()
        .to_owned();
    let mut upload = cli_command(home)
        .arg("sync")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !home.join("upload-ready").exists() {
        if Instant::now() >= deadline {
            upload.kill().unwrap();
            panic!(
                "upload did not start: {:?}",
                upload.wait_with_output().unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cli(home, &["task", "edit", &id, "--title", "During upload"]);
    fs::write(home.join("release-upload"), "").unwrap();
    let output = upload.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(state(home)["tasks"][0]["title"], "During upload");
    let uploaded: Value =
        serde_json::from_slice(&fs::read(home.join("uploaded.json")).unwrap()).unwrap();
    assert_eq!(uploaded["tasks"][0]["title"], "Before upload");
}

#[test]
fn scheduler_diagnostics_are_scoped_to_the_ledger_and_detect_stopped_timers() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::create_dir(home.join("bin")).unwrap();
    executable(
        &home.join("bin/systemctl"),
        "#!/bin/sh\nif [ \"$2\" = is-active ] && [ -f \"$HOME/stopped\" ]; then exit 1; fi\n",
    );
    let units = home.join("config/systemd/user");
    fs::create_dir_all(&units).unwrap();
    let service = units.join("omatracker-report-check.service");
    fs::write(
        &service,
        "ExecStart=omatracker --data-path \"/another/ledger.json\" report check\n",
    )
    .unwrap();
    let diagnostics: Value = serde_json::from_str(&cli(home, &["diagnostics"])).unwrap();
    assert_eq!(diagnostics["backgroundChecksActive"], false);

    fs::write(
        &service,
        format!(
            "ExecStart=omatracker --data-path \"{}\" report check\n",
            home.join("state.json").display()
        ),
    )
    .unwrap();
    let diagnostics: Value = serde_json::from_str(&cli(home, &["diagnostics"])).unwrap();
    assert_eq!(diagnostics["backgroundChecksActive"], true);
    assert_eq!(diagnostics["backgroundChecksEnabled"], true);
    fs::write(home.join("stopped"), "").unwrap();
    let diagnostics: Value = serde_json::from_str(&cli(home, &["diagnostics"])).unwrap();
    assert_eq!(diagnostics["backgroundChecksActive"], false);
    assert_eq!(diagnostics["backgroundChecksEnabled"], true);
}
