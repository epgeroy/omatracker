use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

struct Sandbox {
    home: tempfile::TempDir,
    ledger: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let ledger = home.path().join("tracker.json");
        Self { home, ledger }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }

    fn call(&self, args: &[&str]) -> Value {
        let result = Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("data"))
            .arg("--data-path")
            .arg(&self.ledger)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }

    fn fails(&self, args: &[&str], error: &str) {
        let result = Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("data"))
            .arg("--data-path")
            .arg(&self.ledger)
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(error),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn snapshot_restores_records_documents_and_templates_with_a_recovery_copy() {
    let s = Sandbox::new();
    let project = s.call(&[
        "agent",
        "project.create",
        "--input",
        r#"{"name":"Backed up"}"#,
    ]);
    assert_eq!(project["ok"], true);
    let original = fs::read(&s.ledger).unwrap();
    let invoices = s.path("tracker.json.invoices/bundle");
    fs::create_dir_all(&invoices).unwrap();
    fs::write(invoices.join("issued.pdf"), b"original PDF").unwrap();
    let workflows = s.path("tracker.json.workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(workflows.join("journal.json"), b"original workflow").unwrap();
    let templates = s.path("config/omarchy/omatracker/templates/user");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("template.typ"), b"original template").unwrap();
    let plugin = s.path("config/omarchy/plugins/epgeroy.omatracker");
    fs::create_dir_all(plugin.join(".git")).unwrap();
    fs::create_dir_all(plugin.join("bin")).unwrap();
    fs::write(plugin.join(".git/HEAD"), b"user checkout metadata").unwrap();
    fs::write(plugin.join("Panel.qml"), b"original widget").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        s.path("data/omatracker/bin/omatracker"),
        plugin.join("bin/omatracker"),
    )
    .unwrap();
    let snapshot = s.call(&["backup", "create"]);
    let backup = snapshot["path"].as_str().unwrap();
    assert_eq!(s.call(&["backup", "verify", backup])["valid"], true);
    assert_eq!(
        s.call(&["backup", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::write(invoices.join("issued.pdf"), b"changed PDF").unwrap();
    fs::write(workflows.join("journal.json"), b"changed workflow").unwrap();
    fs::write(invoices.join("unrelated.pdf"), b"newer PDF").unwrap();
    fs::write(templates.join("template.typ"), b"changed template").unwrap();
    fs::write(plugin.join("Panel.qml"), b"changed widget").unwrap();
    s.call(&[
        "agent",
        "project.create",
        "--input",
        r#"{"name":"Newer work"}"#,
    ]);
    let dry = s.call(&["backup", "restore", backup, "--dry-run"]);
    assert_eq!(dry["dryRun"], true);
    s.fails(
        &["backup", "restore", backup, "--expect-revision", "0"],
        "BACKUP_CHANGED",
    );
    let restored = s.call(&["backup", "restore", backup]);
    assert_eq!(fs::read(&s.ledger).unwrap(), original);
    assert_eq!(
        fs::read(invoices.join("issued.pdf")).unwrap(),
        b"original PDF"
    );
    assert!(!invoices.join("unrelated.pdf").exists());
    assert_eq!(
        fs::read(workflows.join("journal.json")).unwrap(),
        b"original workflow"
    );
    assert_eq!(
        fs::read(templates.join("template.typ")).unwrap(),
        b"original template"
    );
    assert_eq!(
        fs::read(plugin.join("Panel.qml")).unwrap(),
        b"original widget"
    );
    assert_eq!(
        fs::read(plugin.join(".git/HEAD")).unwrap(),
        b"user checkout metadata"
    );
    let recovery = restored["recovery"].as_str().unwrap();
    assert_eq!(s.call(&["backup", "verify", recovery])["valid"], true);
    assert_ne!(
        fs::read(PathBuf::from(recovery).join("files/ledger/file")).unwrap(),
        original
    );
}

#[test]
fn corrupt_snapshot_and_symlinked_source_are_rejected() {
    let s = Sandbox::new();
    s.call(&[
        "agent",
        "project.create",
        "--input",
        &json!({"name":"A"}).to_string(),
    ]);
    let snapshot = s.call(&["backup", "create"]);
    let backup = snapshot["path"].as_str().unwrap();
    fs::write(PathBuf::from(backup).join("files/ledger/file"), b"tampered").unwrap();
    s.fails(&["backup", "verify", backup], "BACKUP_CORRUPT");
    s.fails(&["backup", "restore", backup], "BACKUP_CORRUPT");
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let template = s.path("config/omarchy/omatracker/templates");
        fs::create_dir_all(template.parent().unwrap()).unwrap();
        symlink(&s.ledger, &template).unwrap();
        s.fails(&["backup", "create"], "BACKUP_SCOPE");
    }
}

#[test]
fn referenced_logo_under_home_is_restored_even_if_deleted() {
    let s = Sandbox::new();
    s.call(&["agent", "project.create", "--input", r#"{"name":"Logo"}"#]);
    let logo = s.path("art/logo.png");
    fs::create_dir_all(logo.parent().unwrap()).unwrap();
    fs::write(&logo, b"original logo").unwrap();
    let mut ledger: Value = serde_json::from_slice(&fs::read(&s.ledger).unwrap()).unwrap();
    ledger["projects"][1]["logoPath"] = json!(logo);
    fs::write(&s.ledger, serde_json::to_vec_pretty(&ledger).unwrap()).unwrap();
    let backup = s.call(&["backup", "create"]);
    let path = backup["path"].as_str().unwrap();
    fs::remove_file(&logo).unwrap();
    assert_eq!(s.call(&["backup", "verify", path])["valid"], true);
    s.call(&["backup", "restore", path]);
    assert_eq!(fs::read(logo).unwrap(), b"original logo");
}

#[test]
fn deleted_ledger_can_be_restored_from_an_existing_snapshot() {
    let s = Sandbox::new();
    s.call(&[
        "agent",
        "project.create",
        "--input",
        r#"{"name":"Recover"}"#,
    ]);
    let original = fs::read(&s.ledger).unwrap();
    let snapshot = s.call(&["backup", "create"]);
    fs::remove_file(&s.ledger).unwrap();
    let restored = s.call(&["backup", "restore", snapshot["path"].as_str().unwrap()]);
    assert!(restored["recovery"].is_null());
    assert_eq!(fs::read(&s.ledger).unwrap(), original);
}
