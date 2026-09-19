use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

struct Desktop(tempfile::TempDir);

impl Desktop {
    fn new(opener: Option<&str>) -> Self {
        let desktop = Self(tempfile::tempdir().unwrap());
        fs::create_dir(desktop.0.path().join("bin")).unwrap();
        fs::write(desktop.0.path().join("Résumé #1 %.pdf"), b"%PDF-1.7\n").unwrap();
        if let Some(script) = opener {
            let path = desktop.0.path().join("bin/xdg-open");
            fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        desktop
    }

    fn command(&self, path: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
        command
            .current_dir(self.0.path())
            .env("PATH", self.0.path().join("bin"))
            .env("HOME", self.0.path())
            .env("DISPLAY", ":fake")
            .env_remove("WAYLAND_DISPLAY")
            .args([
                "--data-path",
                "ledger.json",
                "agent",
                "artifact.open",
                "--input",
                &json!({"path":path}).to_string(),
            ]);
        command
    }

    fn open(&self) -> Value {
        let output = self.command("Résumé #1 %.pdf").output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["ok"], true);
        assert!(!self.0.path().join("ledger.json").exists());
        response["data"].clone()
    }
}

#[test]
fn opening_encodes_local_paths_like_qml_and_reports_only_dispatch() {
    let desktop = Desktop::new(Some("printf '%s' \"$1\" > \"$HOME/argument\""));
    let data = desktop.open();
    assert_eq!(data["status"], "launch_requested");
    assert_eq!(data["launchRequested"], true);
    assert!(data["visiblyOpened"].is_null());
    assert_eq!(
        data["path"],
        desktop.0.path().join("Résumé #1 %.pdf").to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(desktop.0.path().join("argument")).unwrap(),
        format!(
            "file://{}/R%C3%A9sum%C3%A9%20%231%20%25.pdf",
            desktop.0.path().display()
        )
    );
}

#[test]
fn long_lived_viewer_and_inherited_output_do_not_delay_cli_completion() {
    // output() waits for EOF on both captured streams, so this detects both
    // waiting for the viewer and accidentally inheriting the CLI's pipe handles.
    let desktop = Desktop::new(Some("/bin/sleep 3 &\necho stdout\necho stderr >&2\nwait"));
    let started = Instant::now();
    let data = desktop.open();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(data["status"], "launch_requested");
}

#[test]
fn launch_failures_preserve_the_pdf_path_and_actionable_diagnostics() {
    for script in [
        None,
        Some("echo 'No PDF application associated' >&2\nexit 3"),
    ] {
        let desktop = Desktop::new(script);
        let data = desktop.open();
        assert_eq!(data["status"], "launch_failed");
        assert_eq!(data["launchRequested"], false);
        assert!(std::path::Path::new(data["path"].as_str().unwrap()).is_file());
        let message = data["message"].as_str().unwrap();
        if script.is_some() {
            assert!(message.contains("No PDF application associated"));
            assert_eq!(data["exitCode"], 3);
        } else {
            assert!(message.contains("Could not start xdg-open"));
        }
    }
}

#[test]
fn headless_sessions_do_not_invoke_the_opener() {
    let desktop = Desktop::new(Some("printf launched > \"$HOME/launched\""));
    let output = desktop
        .command("Résumé #1 %.pdf")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["status"], "launch_failed");
    assert!(
        response["data"]["message"]
            .as_str()
            .unwrap()
            .contains("No desktop session")
    );
    assert!(!desktop.0.path().join("launched").exists());
}

#[test]
fn rejects_missing_files_urls_non_pdfs_and_retry_keys_without_launching() {
    let desktop = Desktop::new(Some("printf launched > \"$HOME/launched\""));
    fs::write(desktop.0.path().join("fake.pdf"), "not a PDF").unwrap();
    fs::write(desktop.0.path().join("file.txt"), "%PDF-1.7\n").unwrap();
    for path in [
        "missing.pdf",
        "https://example.test/file.pdf",
        "file:///tmp/file.pdf",
        "fake.pdf",
        "file.txt",
        "bin",
    ] {
        let output = desktop.command(path).output().unwrap();
        assert!(!output.status.success(), "{path}");
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["code"], "INVALID_INPUT");
    }
    let output = desktop
        .command("Résumé #1 %.pdf")
        .args(["--key", "open-key"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!desktop.0.path().join("launched").exists());
}
