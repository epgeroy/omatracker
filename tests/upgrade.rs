use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn call(home: &Path, ledger: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_omatracker"))
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .arg("--data-path")
        .arg(ledger)
        .args(args)
        .output()
        .unwrap()
}

fn candidate(home: &Path, fail: bool) -> std::path::PathBuf {
    let source = home.join("candidate");
    fs::create_dir_all(source.join("bin")).unwrap();
    fs::create_dir_all(source.join("scripts")).unwrap();
    fs::create_dir_all(source.join("templates")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_omatracker"),
        source.join("bin/omatracker"),
    )
    .unwrap();
    fs::write(
        source.join("manifest.json"),
        format!(
            r#"{{"id":"epgeroy.omatracker","version":"{}"}}"#,
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();
    fs::write(source.join("templates/invoice.typ"), "invoice-template").unwrap();
    fs::write(source.join("Panel.qml"), "new-widget").unwrap();
    fs::write(source.join("scripts/install-plugin.py"), if fail { "raise SystemExit('simulated installer failure')\n" }
        else { "import argparse, pathlib\np=argparse.ArgumentParser()\np.add_argument('--source'); p.add_argument('--destination'); p.add_argument('--backend'); p.add_argument('--backup-root'); a=p.parse_args()\nd=pathlib.Path(a.destination); d.mkdir(parents=True, exist_ok=True); (d/'Panel.qml').write_text((pathlib.Path(a.source)/'Panel.qml').read_text())\n" }).unwrap();
    source
}

#[test]
fn local_staged_upgrade_installs_widget_binary_skills_and_keeps_backup() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let old = home.join("data/omatracker/bin/omatracker");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(&old, b"old-backend").unwrap();
    let plugin = home.join("config/omarchy/plugins/epgeroy.omatracker");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("Panel.qml"), b"old-widget").unwrap();
    let source = candidate(home, false);
    let source = source.to_str().unwrap();
    let dry = call(
        home,
        &ledger,
        &["upgrade", "--source", source, "--dry-run", "--no-shell"],
    );
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert_eq!(fs::read(&old).unwrap(), b"old-backend");
    let result = call(
        home,
        &ledger,
        &["upgrade", "--source", source, "--no-shell"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    let backup = value["backup"].as_str().unwrap();
    assert_eq!(
        fs::read(Path::new(backup).join("files/binary/file")).unwrap(),
        b"old-backend"
    );
    assert_eq!(fs::read(plugin.join("Panel.qml")).unwrap(), b"new-widget");
    assert!(
        home.join(".agents/skills/omatracker-upgrade/SKILL.md")
            .exists()
    );
    assert!(
        call(home, &ledger, &["backup", "verify", backup])
            .status
            .success()
    );
    assert!(
        call(
            home,
            &ledger,
            &["upgrade", "--recover", backup, "--no-shell"]
        )
        .status
        .code()
            != Some(0)
    );
}

#[test]
fn failed_upgrade_restores_old_binary_and_keeps_recovery_journal() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let old = home.join("data/omatracker/bin/omatracker");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(&old, b"original-binary").unwrap();
    let source = candidate(home, true);
    let result = call(
        home,
        &ledger,
        &[
            "upgrade",
            "--source",
            source.to_str().unwrap(),
            "--no-shell",
        ],
    );
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("rolled back"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&old).unwrap(), b"original-binary");
    let backups = fs::read_dir(home.join("ledger.json.backups"))
        .unwrap()
        .map(|f| f.unwrap().path())
        .collect::<Vec<_>>();
    assert!(backups.iter().any(|p| p.join("upgrade.json").exists()));
}

#[test]
fn failure_after_migration_rolls_back_ledger_widget_and_new_skills() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let original = fs::read(&ledger).unwrap();
    let old = home.join("data/omatracker/bin/omatracker");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(&old, b"original-binary").unwrap();
    let plugin = home.join("config/omarchy/plugins/epgeroy.omatracker");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("Panel.qml"), b"old-widget").unwrap();
    let source = candidate(home, false);
    let result = call(
        home,
        &ledger,
        &[
            "upgrade",
            "--source",
            source.to_str().unwrap(),
            "--no-shell",
            "--fail-after-migration",
        ],
    );
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("rolled back"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(ledger).unwrap(), original);
    assert_eq!(fs::read(old).unwrap(), b"original-binary");
    assert_eq!(fs::read(plugin.join("Panel.qml")).unwrap(), b"old-widget");
    assert!(!home.join(".agents/skills/omatracker-upgrade").exists());
}

#[test]
fn migration_rejects_a_ledger_revision_changed_after_backup() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"First"}"#]
        )
        .status
        .success()
    );
    let before = fs::read(&ledger).unwrap();
    let stale = call(
        home,
        &ledger,
        &["agent", "migration.apply", "--input", r#"{"revision":0}"#],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stdout).contains("REVISION_CONFLICT"));
    assert_eq!(fs::read(&ledger).unwrap(), before);
}

#[test]
fn dirty_live_plugin_is_preserved_before_any_upgrade_write() {
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let old = home.join("data/omatracker/bin/omatracker");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(&old, b"old-binary").unwrap();
    let plugin = home.join("config/omarchy/plugins/epgeroy.omatracker");
    fs::create_dir_all(&plugin).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg(&plugin)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(plugin.join("Panel.qml"), b"user work").unwrap();
    let source = candidate(home, false);
    let result = call(
        home,
        &ledger,
        &[
            "upgrade",
            "--source",
            source.to_str().unwrap(),
            "--no-shell",
        ],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("local changes"));
    assert_eq!(fs::read(plugin.join("Panel.qml")).unwrap(), b"user work");
    assert!(!home.join("ledger.json.backups").exists());
}

#[cfg(unix)]
#[test]
fn edge_dry_run_reports_the_pinned_main_commit_without_installing() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let source = candidate(home, false);
    fs::write(
        source.join("Cargo.toml"),
        format!(
            "[package]\nname = \"omatracker\"\nversion = \"{}\"\n",
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();
    let fake = home.join("fake-bin");
    fs::create_dir_all(&fake).unwrap();
    let git = fake.join("git");
    fs::write(&git, "#!/bin/sh\nif [ \"$1\" = clone ]; then\n  for dest; do :; done\n  cp -a \"$OMATRACKER_TEST_SOURCE\" \"$dest\"\nelse\n  printf 'abc123edgecommit\\n'\nfi\n").unwrap();
    let cargo = fake.join("cargo");
    fs::write(&cargo, "#!/bin/sh\nprev=\nfor value; do\n  if [ \"$prev\" = --manifest-path ]; then\n    dest=$(dirname \"$value\")\n    mkdir -p \"$dest/target/release\"\n    cp \"$dest/bin/omatracker\" \"$dest/target/release/omatracker\"\n  fi\n  prev=$value\ndone\n").unwrap();
    for file in [&git, &cargo] {
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_omatracker"))
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("OMATRACKER_TEST_SOURCE", &source)
        .env(
            "PATH",
            format!("{}:{}", fake.display(), std::env::var("PATH").unwrap()),
        )
        .arg("--data-path")
        .arg(&ledger)
        .args(["upgrade", "--edge", "--dry-run", "--no-shell"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["reference"], "main@abc123edgecommit");
    assert!(!home.join("ledger.json.backups").exists());
}

#[cfg(unix)]
#[test]
fn release_dry_run_checks_the_tagged_binary_digest() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let home = work.path();
    let ledger = home.join("ledger.json");
    assert!(
        call(
            home,
            &ledger,
            &["agent", "project.create", "--input", r#"{"name":"Keep"}"#]
        )
        .status
        .success()
    );
    let source = candidate(home, false);
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(source.join("bin/omatracker")).unwrap())
    );
    let fake = home.join("fake-bin");
    fs::create_dir_all(&fake).unwrap();
    let git = fake.join("git");
    fs::write(&git, "#!/bin/sh\nif [ \"$1\" = clone ]; then\n  for dest; do :; done\n  cp -a \"$OMATRACKER_TEST_SOURCE\" \"$dest\"\nelse\n  printf 'tagcommit123\\n'\nfi\n").unwrap();
    let gh = fake.join("gh");
    fs::write(&gh, "#!/bin/sh\nif [ \"$2\" = view ]; then\n  printf 'v%s\\n' \"$OMATRACKER_TEST_VERSION\"\nelse\n  prev=\n  for value; do\n    if [ \"$prev\" = --pattern ]; then pattern=$value; fi\n    if [ \"$prev\" = --dir ]; then dir=$value; fi\n    prev=$value\n  done\n  if [ \"$pattern\" = omatracker-linux-x86_64 ]; then\n    cp \"$OMATRACKER_TEST_SOURCE/bin/omatracker\" \"$dir/$pattern\"\n  else\n    printf '%s  omatracker-linux-x86_64\\n' \"$OMATRACKER_TEST_SHA\" > \"$dir/$pattern\"\n  fi\nfi\n").unwrap();
    for file in [&git, &gh] {
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let run = |sha: &str| {
        Command::new(env!("CARGO_BIN_EXE_omatracker"))
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("OMATRACKER_TEST_SOURCE", &source)
            .env("OMATRACKER_TEST_SHA", sha)
            .env("OMATRACKER_TEST_VERSION", env!("CARGO_PKG_VERSION"))
            .env(
                "PATH",
                format!("{}:{}", fake.display(), std::env::var("PATH").unwrap()),
            )
            .arg("--data-path")
            .arg(&ledger)
            .args(["upgrade", "--dry-run", "--no-shell"])
            .output()
            .unwrap()
    };
    let valid = run(&digest);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let response: Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(
        response["reference"],
        format!("v{}@tagcommit123", env!("CARGO_PKG_VERSION"))
    );
    let corrupt = run("bad-checksum");
    assert!(!corrupt.status.success());
    assert!(String::from_utf8_lossy(&corrupt.stderr).contains("UPGRADE_CHECKSUM"));
    assert!(!home.join("ledger.json.backups").exists());
}
