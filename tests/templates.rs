use omatracker::DEFAULT_PROJECT_ID;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox(tempfile::TempDir);

impl Sandbox {
    fn new() -> Self {
        let sandbox = Self(tempfile::tempdir().unwrap());
        fs::create_dir(sandbox.path("bin")).unwrap();
        fs::create_dir(sandbox.path("bundled")).unwrap();
        for name in ["detailed", "summary", "invoice"] {
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("templates/{name}.typ")),
                sandbox.path(&format!("bundled/{name}.typ")),
            )
            .unwrap();
        }
        sandbox.executable(
            "typst",
            "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nprintf pdf > \"$5\"\n",
        );
        // A failing upload leaves the report available for retry assertions.
        sandbox.executable("rclone", "#!/bin/sh\nexit 1\n");
        sandbox
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn executable(&self, name: &str, text: &str) {
        let path = self.path(&format!("bin/{name}"));
        fs::write(&path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
        command
            .env("HOME", self.0.path())
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("OMATRACKER_TEMPLATE_DIR", self.path("bundled"))
            .env("PATH", self.path("bin"))
            .args(["--data-path", self.path("state.json").to_str().unwrap()]);
        command
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    fn fail(&self, args: &[&str]) -> Output {
        let output = self.command().args(args).output().unwrap();
        assert!(!output.status.success(), "unexpected success: {args:?}");
        output
    }

    fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.path("state.json")).unwrap()).unwrap()
    }

    fn custom(&self) -> PathBuf {
        self.run(&["template", "create", "client-report", "--from", "detailed"]);
        self.path("config/omarchy/omatracker/templates/client-report")
    }

    fn real_typst(&self) -> bool {
        let Some(typst) = std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|path| path.join("typst"))
                .find(|path| path.is_file())
        }) else {
            eprintln!(
                "Typst unavailable; real-compiler integration test skipped (make template-check requires Typst)"
            );
            return false;
        };
        fs::remove_file(self.path("bin/typst")).unwrap();
        symlink(typst, self.path("bin/typst")).unwrap();
        true
    }
}

#[test]
fn custom_templates_survive_reload_and_missing_sources_without_silent_fallback() {
    let sandbox = Sandbox::new();
    let folder = sandbox.custom();
    let catalog: Value =
        serde_json::from_str(&sandbox.run(&["template", "list", "--json"])).unwrap();
    assert_eq!(catalog.as_array().unwrap().len(), 4);
    assert_eq!(catalog[3]["id"], "user:client-report");
    assert_eq!(catalog[3]["builtin"], false);
    assert_eq!(
        sandbox.run(&["template", "path", "user:client-report"]),
        folder.join("template.typ").display().to_string()
    );
    sandbox.run(&[
        "project",
        "update",
        DEFAULT_PROJECT_ID,
        "--template-id",
        "user:client-report",
    ]);
    sandbox.run(&["report", "export", "weekly"]);
    let before = fs::read(sandbox.path("state.json")).unwrap();
    sandbox.fail(&["template", "create", "client-report"]);
    assert_eq!(fs::read(sandbox.path("state.json")).unwrap(), before);
    fs::remove_dir_all(folder).unwrap();
    sandbox.run(&["task", "add", "Still works"]);
    let state = sandbox.state();
    assert_eq!(state["projects"][0]["templateId"], "user:client-report");
    assert_eq!(state["reports"][0]["templateId"], "user:client-report");
    sandbox.fail(&["report", "export", "monthly"]);
    assert_eq!(sandbox.state()["reports"].as_array().unwrap().len(), 1);
}

#[test]
fn selection_and_creation_reject_invalid_ids_symlinks_and_missing_files() {
    let sandbox = Sandbox::new();
    for name in ["../escape", "/absolute", "a/b", "..", "", "two words"] {
        sandbox.fail(&["template", "create", name]);
    }
    for id in ["../summary", "user:../escape", "unknown", "user:missing"] {
        sandbox.fail(&["project", "update", DEFAULT_PROJECT_ID, "--template-id", id]);
        sandbox.fail(&["template", "path", id]);
    }
    assert!(!sandbox.path("state.json").exists());
    let folder = sandbox.custom();
    symlink(
        sandbox.path("bundled/detailed.typ"),
        folder.join("escape.typ"),
    )
    .unwrap();
    sandbox.fail(&["template", "validate", "user:client-report"]);
    sandbox.fail(&["template", "create", "copy", "--from", "user:client-report"]);
    assert!(!folder.parent().unwrap().join("copy").exists());
    fs::remove_file(folder.join("template.typ")).unwrap();
    symlink(
        sandbox.path("bundled/detailed.typ"),
        folder.join("template.typ"),
    )
    .unwrap();
    sandbox.fail(&["template", "path", "user:client-report"]);
}

#[test]
fn xdg_library_and_builtin_override_have_independent_resolution() {
    let sandbox = Sandbox::new();
    sandbox.custom();
    fs::remove_file(sandbox.path("bundled/detailed.typ")).unwrap();
    sandbox.fail(&["template", "path", "detailed"]);
    sandbox.run(&["template", "path", "user:client-report"]);
    let result = sandbox
        .command()
        .env_remove("XDG_CONFIG_HOME")
        .args(["template", "create", "fallback", "--from", "summary"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(
        sandbox
            .path(".config/omarchy/omatracker/templates/fallback/template.typ")
            .is_file()
    );
}

#[test]
fn queued_report_pins_sources_assets_logo_and_data_even_if_pdf_needs_rebuilding() {
    let sandbox = Sandbox::new();
    let folder = sandbox.custom();
    fs::create_dir(folder.join("assets")).unwrap();
    fs::write(folder.join("assets/note.txt"), "Original asset").unwrap();
    fs::write(sandbox.path("my  logo.svg"), "original logo").unwrap();
    sandbox.run(&[
        "project",
        "update",
        DEFAULT_PROJECT_ID,
        "--template-id",
        "user:client-report",
        "--accent-color",
        "#123abc",
        "--paper",
        "letter",
        "--logo-path",
        sandbox.path("my  logo.svg").to_str().unwrap(),
    ]);
    sandbox.run(&["report", "export", "weekly"]);
    let state = sandbox.state();
    let report = &state["reports"][0];
    let bundle = PathBuf::from(report["templateBundle"].as_str().unwrap());
    let captured_data = fs::read(bundle.join("data.json")).unwrap();
    let data: Value = serde_json::from_slice(&captured_data).unwrap();
    assert_eq!(data["project"]["accentColor"], "#123abc");
    assert_eq!(data["project"]["paper"], "letter");
    assert_eq!(data["project"]["logoPath"], "/project-logo.svg");
    let manifest = fs::read(bundle.join("manifest.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&manifest).unwrap()["sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    fs::remove_dir_all(folder).unwrap();
    fs::remove_file(sandbox.path("my  logo.svg")).unwrap();
    fs::remove_file(report["pdfPath"].as_str().unwrap()).unwrap();
    sandbox.run(&["report", "retry"]);
    assert!(Path::new(report["pdfPath"].as_str().unwrap()).is_file());
    assert_eq!(fs::read(bundle.join("data.json")).unwrap(), captured_data);
    assert_eq!(fs::read(bundle.join("manifest.json")).unwrap(), manifest);
    assert_eq!(
        fs::read_to_string(bundle.join("template/assets/note.txt")).unwrap(),
        "Original asset"
    );
    assert_eq!(
        fs::read_to_string(bundle.join("project-logo.svg")).unwrap(),
        "original logo"
    );
    // A lost captured bundle must fail, never substitute today's source.
    fs::remove_file(report["pdfPath"].as_str().unwrap()).unwrap();
    fs::remove_dir_all(&bundle).unwrap();
    sandbox.run(&["report", "retry"]);
    assert!(
        sandbox.state()["reports"][0]["lastError"]
            .as_str()
            .unwrap()
            .contains("captured report template is missing")
    );
}

#[test]
fn legacy_reports_capture_once_and_preserve_existing_pdfs() {
    let sandbox = Sandbox::new();
    sandbox.run(&["report", "export", "weekly"]);
    let mut state = sandbox.state();
    state["reports"][0]
        .as_object_mut()
        .unwrap()
        .remove("templateBundle");
    let report = state["reports"][0].clone();
    fs::write(
        sandbox.path("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let legacy_bundle = PathBuf::from(report["typPath"].as_str().unwrap()).with_extension("bundle");
    sandbox.run(&["report", "retry"]);
    assert!(!legacy_bundle.exists());
    fs::remove_file(report["pdfPath"].as_str().unwrap()).unwrap();
    sandbox.run(&["report", "retry"]);
    assert!(legacy_bundle.join("manifest.json").is_file());
    fs::remove_file(sandbox.path("bundled/detailed.typ")).unwrap();
    fs::remove_file(report["pdfPath"].as_str().unwrap()).unwrap();
    sandbox.run(&["report", "retry"]);
    assert!(Path::new(report["pdfPath"].as_str().unwrap()).is_file());
}

#[test]
fn real_typst_previews_relative_imports_logos_and_errors_without_ledger_or_upload_side_effects() {
    let sandbox = Sandbox::new();
    if !sandbox.real_typst() {
        return;
    }
    sandbox.executable(
        "rclone",
        "#!/bin/sh\nprintf called > \"$HOME/upload-called\"\nexit 1\n",
    );
    sandbox.run(&["template", "validate", "detailed"]);
    sandbox.run(&["template", "validate", "summary"]);
    assert!(!sandbox.path("state.json").exists());
    let folder = sandbox.custom();
    fs::write(
        folder.join("heading.typ"),
        "#let title = [Custom heading]\n",
    )
    .unwrap();
    fs::write(folder.join("template.typ"), "#import \"heading.typ\": title\n#let render(data) = { title; image(data.project.logoPath, width: 20mm); data.totalDuration }\n").unwrap();
    fs::write(sandbox.path("logo.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><rect width=\"20\" height=\"20\" fill=\"red\"/></svg>").unwrap();
    sandbox.run(&[
        "project",
        "update",
        DEFAULT_PROJECT_ID,
        "--logo-path",
        sandbox.path("logo.svg").to_str().unwrap(),
    ]);
    let before = fs::read(sandbox.path("state.json")).unwrap();
    for id in ["user:client-report", "detailed", "summary"] {
        let pdf = sandbox.run(&["template", "preview", id, "--project", DEFAULT_PROJECT_ID]);
        assert!(fs::read(pdf).unwrap().starts_with(b"%PDF"));
    }
    fs::write(
        folder.join("template.typ"),
        "#let render(data) = { syntax error !!! }",
    )
    .unwrap();
    let error = sandbox.fail(&["template", "preview", "user:client-report"]);
    assert!(String::from_utf8_lossy(&error.stderr).contains("Typst compilation failed"));
    assert_eq!(fs::read(sandbox.path("state.json")).unwrap(), before);
    assert!(!sandbox.path("upload-called").exists());
}

#[test]
fn appearance_validation_is_atomic_and_logo_can_be_cleared() {
    let sandbox = Sandbox::new();
    sandbox.run(&[
        "project",
        "update",
        DEFAULT_PROJECT_ID,
        "--accent-color",
        "#123456",
        "--paper",
        "letter",
    ]);
    let before = sandbox.state();
    for args in [
        ["--accent-color", "invalid"],
        ["--paper", "poster"],
        ["--logo-path", "/missing/logo.png"],
    ] {
        sandbox.fail(&[
            "project",
            "update",
            DEFAULT_PROJECT_ID,
            "--name",
            "Must not save",
            args[0],
            args[1],
        ]);
        assert_eq!(sandbox.state(), before);
    }
    sandbox.run(&["project", "update", DEFAULT_PROJECT_ID, "--logo-path", ""]);
    assert_eq!(sandbox.state()["projects"][0]["logoPath"], json!(""));
}
