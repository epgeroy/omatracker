use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct App {
    home: tempfile::TempDir,
    path: PathBuf,
    invoice_root: PathBuf,
    remote_pdf: String,
}
impl App {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("ledger.json");
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = bin.join("rclone");
        fs::write(&script, r#"#!/usr/bin/python3
import json, os, pathlib, shutil, sys
home = pathlib.Path(os.environ['HOME'])
args = [a for a in sys.argv[1:] if a != '--']
with (home / 'calls.jsonl').open('a') as log: log.write(json.dumps(args) + '\n')
if args[0] == '--version': print('fake-rclone'); sys.exit(0)
def remote(value):
    assert value.startswith('test:') and '..' not in value.split('/')
    return home / 'remote' / value[5:].lstrip('/')
if args[0] == 'lsjson':
    p = remote(args[-1])
    if not p.exists(): sys.exit(4)
    print(json.dumps({'IsDir':p.is_dir(),'Size':p.stat().st_size,'ModTime':str(p.stat().st_mtime_ns),'ID':args[-1]}))
elif args[0] == 'cat':
    p = remote(args[-1])
    if not p.exists(): sys.exit(4)
    sys.stdout.buffer.write(p.read_bytes())
elif args[0] == 'copyto':
    if os.environ.get('FAIL_BACKUP'): sys.exit(5)
    shutil.copyfile(remote(args[1]), args[2])
elif args[0] == 'deletefile':
    if os.environ.get('FAIL_DELETE') == args[1]: sys.exit(5)
    remote(args[1]).unlink()
else:
    raise Exception('Unsupported operation: ' + str(args))
"#).unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        let mut app = Self {
            home,
            path,
            invoice_root: PathBuf::new(),
            remote_pdf: String::new(),
        };
        let client = app.agent("client.set", json!({"details":{"name":"Old client"}}))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let project = app.agent("project.create",json!({"name":"Old project","client":client,"rate":"50","currency":"USD","effectiveAt":"2025-01-01T00:00:00Z"}))["id"].as_str().unwrap().to_owned();
        let task = app.agent("task.create", json!({"project":project,"title":"Old task"}))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        app.agent(
            "entry.add",
            json!({"id":task,"start":"2025-08-01T10:00:00Z","seconds":3600,"note":"Recorded work"}),
        );
        app.agent("issuer.set", json!({"details":{"name":"Studio"}}));
        app.agent(
            "drive.configure",
            json!({"remote":"test","driveFolder":"Tracker"}),
        );
        let draft = app.agent(
            "invoice.create",
            json!({"project":project,"from":"2025-08-01","to":"2025-09-01","currency":"USD"}),
        );
        let invoice = app.agent(
            "invoice.issue",
            json!({"id":draft["id"],"revision":1,"date":"2025-09-01"}),
        );
        fs::write(invoice["pdfPath"].as_str().unwrap(), b"%PDF-original").unwrap();
        app.agent(
            "invoice.create",
            json!({"project":project,"from":"2025-08-01","to":"2025-09-01","currency":"USD"}),
        );
        app.agent("project.remove", json!({"project":project}));
        app.agent("client.remove", json!({"id":client}));
        app.agent(
            "project.update",
            json!({"project":"project-unassigned","name":"Customized fallback"}),
        );
        app.invoice_root = PathBuf::from(format!("{}.invoices", app.path.display()));
        app.remote_pdf = format!("test:Tracker/invoices/{project}/INV-2025-00001.pdf");
        app.put_remote(&app.remote_pdf, b"%PDF-remote-original");
        // Orphaned invoice snapshot: removed ledger records must not hide its known remote file.
        let orphan = app.invoice_root.join("invoice-orphan/bundle-original");
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("data.json"),serde_json::to_vec(&json!({"invoice":{"id":"invoice-orphan","projectId":project,"number":"INV-2025-00002","remotePath":""}})).unwrap()).unwrap();
        app.put_remote(
            &format!("test:Tracker/invoices/{project}/INV-2025-00002.pdf"),
            b"%PDF-orphan",
        );
        app.put_remote(
            "test:Tracker/unrelated.txt",
            b"not owned by tracker records",
        );
        let template = app
            .home
            .path()
            .join("config/omarchy/omatracker/templates/personal");
        fs::create_dir_all(&template).unwrap();
        fs::write(template.join("template.typ"), "User template").unwrap();
        let checkpoint = json!({"preferences":{"hourlyClick":false,"volume":42,"reducedMotion":true},"claimedHours":10,"lastCheckedAt":1});
        fs::write(
            format!("{}.feedback.json", app.path.display()),
            serde_json::to_vec(&checkpoint).unwrap(),
        )
        .unwrap();
        app.put_remote("test:Tracker/state.json", &fs::read(&app.path).unwrap());
        app
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
        command
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env(
                "OMATRACKER_TEMPLATE_DIR",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            )
            .env("PATH", self.home.path().join("bin"))
            .arg("--data-path")
            .arg(&self.path);
        command
    }
    fn agent(&self, action: &str, input: Value) -> Value {
        ok(self
            .command()
            .args(["agent", action, "--input", &input.to_string()])
            .output()
            .unwrap())["data"]
            .clone()
    }
    fn remote(&self, target: &str) -> PathBuf {
        self.home
            .path()
            .join("remote")
            .join(target.strip_prefix("test:").unwrap())
    }
    fn put_remote(&self, target: &str, content: &[u8]) {
        let path = self.remote(target);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fn clear(&self, args: &[&str]) -> Value {
        ok(self
            .command()
            .args(["data", "clear", "--json"])
            .args(args)
            .output()
            .unwrap())
    }
    fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.path).unwrap()).unwrap()
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.home.path().join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}
fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn dry_run_identifies_exact_files_without_clearing_local_or_remote_data() {
    let app = App::new();
    let before = fs::read(&app.path).unwrap();
    let response = app.clear(&["--include-drive", "--dry-run"]);
    assert_eq!(response["dryRun"], true);
    assert_eq!(response["plan"]["records"]["userProjects"], 1);
    assert_eq!(response["plan"]["records"]["clients"], 1);
    assert_eq!(response["plan"]["records"]["invoices"], 2);
    assert_eq!(response["plan"]["remoteFiles"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read(&app.path).unwrap(), before);
    assert!(app.invoice_root.exists());
    assert!(app.remote(&app.remote_pdf).exists());
    assert!(!PathBuf::from(format!("{}.backups", app.path.display())).exists());
    assert!(
        app.calls()
            .iter()
            .all(|v| v[0] == "lsjson" || v[0] == "cat")
    );
}

#[test]
fn clear_really_removes_archived_records_drafts_and_documents_but_retains_configuration() {
    let app = App::new();
    let before = fs::read(&app.path).unwrap();
    let response = app.clear(&[]);
    let backup = Path::new(response["backupPath"].as_str().unwrap());
    assert_eq!(fs::read(backup.join("ledger.json")).unwrap(), before);
    assert!(!app.invoice_root.exists());
    assert!(app.remote(&app.remote_pdf).exists());
    assert!(app.calls().is_empty());
    let state = app.state();
    assert_eq!(state["projects"].as_array().unwrap().len(), 1);
    assert_eq!(state["projects"][0]["name"], "Unassigned");
    assert_eq!(state["projects"][0]["clientName"], "");
    for field in ["tasks", "entries", "reports"] {
        assert!(state[field].as_array().unwrap().is_empty());
    }
    for field in ["clients", "requests", "bindings", "taskRates"] {
        assert!(state["billing"][field].as_object().unwrap().is_empty());
    }
    for field in [
        "invoices",
        "archivedProjects",
        "archivedClients",
        "corrections",
        "taskRateAdjustments",
    ] {
        assert!(state["billing"][field].as_array().unwrap().is_empty());
    }
    assert_eq!(state["billing"]["issuer"]["name"], "Studio");
    assert_eq!(state["billing"]["sequences"]["2025"], 1);
    assert_eq!(state["drive"]["remote"], "test");
    let feedback: Value =
        serde_json::from_slice(&fs::read(format!("{}.feedback.json", app.path.display())).unwrap())
            .unwrap();
    assert_eq!(feedback["preferences"]["volume"], 42);
    assert_eq!(feedback["claimedHours"], 0);
    assert!(
        app.home
            .path()
            .join("config/omarchy/omatracker/templates/personal/template.typ")
            .is_file()
    );
    assert_eq!(response["remaining"]["userProjects"], 0);
    assert_eq!(response["systemWorkspace"]["protected"], true);
    assert!(
        backup
            .join("local/0/invoice-orphan/bundle-original/data.json")
            .is_file()
    );
}

#[test]
fn drive_clear_backs_up_all_targets_before_deletion_and_preserves_unrelated_files() {
    let app = App::new();
    let response = app.clear(&["--include-drive"]);
    assert_eq!(response["deletedRemoteFiles"].as_array().unwrap().len(), 3);
    assert!(!app.remote(&app.remote_pdf).exists());
    assert!(!app.remote("test:Tracker/state.json").exists());
    assert_eq!(
        fs::read(app.remote("test:Tracker/unrelated.txt")).unwrap(),
        b"not owned by tracker records"
    );
    let calls = app.calls();
    let first_delete = calls.iter().position(|c| c[0] == "deletefile").unwrap();
    assert_eq!(
        calls[..first_delete]
            .iter()
            .filter(|c| c[0] == "copyto")
            .count(),
        3
    );
    assert!(!calls.iter().any(|c| c[0] == "purge" || c[0] == "delete"));
    let backup = Path::new(response["backupPath"].as_str().unwrap());
    assert_eq!(fs::read_dir(backup.join("remote")).unwrap().count(), 3);
    let journal: Value =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(journal["complete"], true);
    assert_eq!(journal["localStateCleared"], true);
}

#[test]
fn failed_remote_backup_or_delete_keeps_local_records_and_can_be_retried() {
    let app = App::new();
    let before = fs::read(&app.path).unwrap();
    let failed = app
        .command()
        .env("FAIL_BACKUP", "1")
        .args(["data", "clear", "--include-drive", "--json"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    assert!(!app.calls().iter().any(|c| c[0] == "deletefile"));
    let failed = app
        .command()
        .env("FAIL_DELETE", "test:Tracker/state.json")
        .args(["data", "clear", "--include-drive", "--json"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    assert!(app.invoice_root.exists());
    let error: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(error["error"]["code"], "CLEAR_FAILED");
    let result = app.clear(&["--include-drive"]);
    assert_eq!(result["remaining"]["clients"], 0);
    assert!(!app.remote("test:Tracker/state.json").exists());
}

#[test]
fn refuses_foreign_remote_ledger_and_symlinked_artifact_directory() {
    use std::os::unix::fs::symlink;
    let app = App::new();
    let before = fs::read(&app.path).unwrap();
    app.put_remote(
        "test:Tracker/state.json",
        br#"{"version":4,"projects":[{"id":"foreign-project","name":"Someone else"}]}"#,
    );
    let output = app
        .command()
        .args(["data", "clear", "--include-drive", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    assert!(
        !app.calls()
            .iter()
            .any(|c| c[0] == "deletefile" || c[0] == "copyto")
    );
    let elsewhere = app.home.path().join("elsewhere");
    fs::rename(&app.invoice_root, &elsewhere).unwrap();
    symlink(&elsewhere, &app.invoice_root).unwrap();
    let output = app
        .command()
        .args(["data", "clear", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(elsewhere.is_dir());
    assert_eq!(fs::read(&app.path).unwrap(), before);
}

#[test]
fn agent_clear_requires_explicit_scope_and_rejects_automatic_retry_keys() {
    let app = App::new();
    let result = app.agent("data.clear", json!({"dryRun":true}));
    assert_eq!(result["dryRun"], true);
    assert_eq!(result["plan"]["records"]["userProjects"], 1);
    let before = fs::read(&app.path).unwrap();
    let output = app
        .command()
        .args(["agent", "data.clear", "--key", "old-retry-key"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    let output = app
        .command()
        .args([
            "agent",
            "data.clear",
            "--input",
            r#"{"project":"only-this-project"}"#,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(&app.path).unwrap(), before);
}

#[test]
fn protected_workspace_is_labeled_and_tracked_report_artifacts_are_scoped() {
    let app = App::new();
    let projects = app.agent("project.list", json!({"includeArchived":true}));
    assert!(
        projects["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "project-unassigned" && p["protected"] == true)
    );
    assert_eq!(
        app.agent("project.get", json!({"project":"project-unassigned"}))["protected"],
        true
    );
    let mut state: omatracker::State = serde_json::from_value(app.state()).unwrap();
    let project = state
        .projects
        .iter()
        .find(|p| p.id != "project-unassigned")
        .unwrap();
    let key = format!("{}:monthly:2025-08-01", project.id);
    let prefix = key.replace(':', "-");
    let cache = app.home.path().join(".cache/omarchy/omatracker");
    let bundle = cache.join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("data.json"), b"{}").unwrap();
    fs::write(bundle.join("report.typ"), b"report").unwrap();
    let pdf = cache.join(format!("{prefix}.pdf"));
    fs::write(&pdf, b"report pdf").unwrap();
    let unrelated = cache.join("other-ledger.txt");
    fs::write(&unrelated, b"Keep").unwrap();
    state.reports.push(omatracker::Report {
        key,
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        period: "monthly".into(),
        start_at: 1754006400000,
        end_at: 1756684800000,
        data_path: bundle.join("data.json").display().to_string(),
        typ_path: bundle.join("report.typ").display().to_string(),
        template_bundle: bundle.display().to_string(),
        pdf_path: pdf.display().to_string(),
        ..Default::default()
    });
    fs::write(&app.path, serde_json::to_vec(&state).unwrap()).unwrap();
    let other_report = "test:Tracker/reports/old-project/monthly/2025-08-01.pdf";
    app.put_remote(
        other_report,
        b"Report from another ledger with the same project name",
    );
    let result = app.clear(&["--include-drive"]);
    assert_eq!(result["cleared"]["reports"], 1);
    assert!(!bundle.exists());
    assert!(!pdf.exists());
    assert_eq!(fs::read(unrelated).unwrap(), b"Keep");
    assert!(app.remote(other_report).is_file()); // No upload target was recorded for our local report.
}

#[test]
fn clear_waits_for_ledger_upload_workers() {
    use fs2::FileExt;
    let app = App::new();
    let before = fs::read(&app.path).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(format!("{}.sync-worker.lock", app.path.display()))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let mut child = app
        .command()
        .args(["data", "clear", "--json"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(fs::read(&app.path).unwrap(), before);
    FileExt::unlock(&lock).unwrap();
    assert_eq!(
        ok(child.wait_with_output().unwrap())["remaining"]["userProjects"],
        0
    );
}

#[test]
fn an_empty_trackers_own_synced_snapshot_can_be_removed() {
    let app = App::new();
    app.clear(&[]);
    app.put_remote("test:Tracker/state.json", &fs::read(&app.path).unwrap());
    let result = app.clear(&["--include-drive"]);
    assert_eq!(
        result["deletedRemoteFiles"],
        json!(["test:Tracker/state.json"])
    );
    assert!(!app.remote("test:Tracker/state.json").exists());
    assert!(app.remote("test:Tracker/unrelated.txt").exists());
}
