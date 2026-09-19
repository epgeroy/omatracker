use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox(tempfile::TempDir);

#[test]
fn installed_retry_guidance_matches_the_embedded_batch_key_contract() {
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--harness", "opencode", "--json"]);
    let root = sandbox.root().join("config/opencode/skills/omatracker");
    for (file, source) in [
        ("SKILL.md", include_str!("../skills/omatracker/SKILL.md")),
        ("AGENT_API.md", include_str!("../AGENT_API.md")),
        (
            "references/workflows.md",
            include_str!("../skills/omatracker/references/workflows.md"),
        ),
    ] {
        let installed = fs::read_to_string(root.join(file)).unwrap();
        if file == "SKILL.md" {
            // Installation prepends executable-path guidance to the skill body.
            assert!(installed.ends_with(source.split_once("# OmaTracker\n").unwrap().1));
        } else {
            assert_eq!(installed, source);
        }
        assert!(installed.contains("request.keys"));
    }
    let response = sandbox.run(&[
        "agent",
        "request.keys",
        "--input",
        r#"{"labels":["create-task","add-entry","price-entry"]}"#,
    ]);
    assert_eq!(response["data"]["keys"].as_object().unwrap().len(), 3);
    assert!(!sandbox.root().join("must-not-open").exists());
}

impl Sandbox {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_omatracker"));
        command
            .current_dir(self.root())
            .env("HOME", self.root())
            .env("XDG_CONFIG_HOME", self.root().join("config"))
            .env_remove("CLAUDE_CONFIG_DIR")
            .env(
                "OMATRACKER_DATA_PATH",
                self.root().join("must-not-open/ledger.json"),
            );
        command
    }
    fn run(&self, args: &[&str]) -> Value {
        let output = self.command().args(args).output().unwrap();
        success(output)
    }
    fn installed(&self) -> PathBuf {
        self.root().join(".agents/skills/omatracker")
    }
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn targets_and_dry_run_do_not_write_and_honor_config_locations() {
    let sandbox = Sandbox::new();
    let targets = sandbox.run(&["skill", "targets", "--json"]);
    assert_eq!(targets["targets"].as_array().unwrap().len(), 6);
    let opencode = targets["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["harness"] == "opencode")
        .unwrap();
    assert_eq!(
        opencode["path"],
        sandbox
            .root()
            .join("config/opencode/skills/omatracker")
            .to_str()
            .unwrap()
    );
    let response = sandbox.run(&[
        "skill",
        "install",
        "--harness",
        "opencode,claude,codex,gemini,cursor",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(response["dryRun"], true);
    assert_eq!(fs::read_dir(sandbox.root()).unwrap().count(), 0);
    let custom = success(
        sandbox
            .command()
            .env("CLAUDE_CONFIG_DIR", sandbox.root().join("claude-custom"))
            .args(["skill", "install", "--harness", "claude", "--json"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        custom["installations"][0]["path"],
        sandbox
            .root()
            .join("claude-custom/skills/omatracker")
            .to_str()
            .unwrap()
    );
}

#[test]
fn global_install_is_self_contained_deduplicated_and_repeatable() {
    let sandbox = Sandbox::new();
    let result = sandbox.run(&[
        "skill",
        "install",
        "--harness",
        "shared,codex,opencode,claude,gemini,cursor",
        "--json",
    ]);
    assert_eq!(result["installations"].as_array().unwrap().len(), 5);
    assert_eq!(
        result["installations"][0]["harnesses"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for install in result["installations"].as_array().unwrap() {
        let root = Path::new(install["path"].as_str().unwrap());
        for file in [
            "SKILL.md",
            "AGENT_API.md",
            "TEMPLATES.md",
            "references/workflows.md",
            "references/installation.md",
            "tests/manual-invoices.md",
        ] {
            assert!(root.join(file).is_file(), "missing {file}");
        }
        assert!(
            fs::read_to_string(root.join("SKILL.md"))
                .unwrap()
                .starts_with("---\nname: omatracker\n")
        );
    }
    let before = fs::read(sandbox.installed().join("SKILL.md")).unwrap();
    let again = sandbox.run(&["skill", "install", "--json"]);
    assert_eq!(again["installations"][0]["action"], "unchanged");
    assert_eq!(
        fs::read(sandbox.installed().join("SKILL.md")).unwrap(),
        before
    );
    assert!(!sandbox.root().join("must-not-open").exists());
}

#[test]
fn conflict_preflight_preserves_other_targets_and_force_keeps_a_backup() {
    let sandbox = Sandbox::new();
    let existing = sandbox.root().join(".claude/skills/omatracker");
    fs::create_dir_all(&existing).unwrap();
    fs::write(existing.join("SKILL.md"), "My customized skill").unwrap();
    fs::write(existing.join("notes.txt"), "Keep these notes").unwrap();
    let output = sandbox
        .command()
        .args(["skill", "install", "--harness", "shared,claude", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!sandbox.installed().exists());
    assert_eq!(
        fs::read_to_string(existing.join("SKILL.md")).unwrap(),
        "My customized skill"
    );
    let response = sandbox.run(&[
        "skill",
        "install",
        "--harness",
        "claude",
        "--force",
        "--json",
    ]);
    let backup = Path::new(response["installations"][0]["backup"].as_str().unwrap());
    assert!(!backup.starts_with(existing.parent().unwrap()));
    assert_eq!(
        fs::read_to_string(backup.join("notes.txt")).unwrap(),
        "Keep these notes"
    );
    assert_eq!(
        fs::read_to_string(backup.join("SKILL.md")).unwrap(),
        "My customized skill"
    );
    assert!(existing.join("AGENT_API.md").is_file());
}

#[test]
fn local_edits_are_not_overwritten_during_an_update() {
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--json"]);
    fs::write(
        sandbox.installed().join("references/workflows.md"),
        "My local workflow",
    )
    .unwrap();
    let output = sandbox
        .command()
        .args(["skill", "install", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(sandbox.installed().join("references/workflows.md")).unwrap(),
        "My local workflow"
    );
}

#[test]
fn standalone_binary_installs_from_any_directory_and_refreshes_its_absolute_path() {
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--json"]);
    let binary = sandbox.root().join("tracker with 'quotes'");
    fs::copy(env!("CARGO_BIN_EXE_omatracker"), &binary).unwrap();
    let response = success(
        Command::new(&binary)
            .current_dir(sandbox.root())
            .env("HOME", sandbox.root())
            .args(["skill", "install", "--json"])
            .output()
            .unwrap(),
    );
    assert_eq!(response["installations"][0]["action"], "update");
    let instructions =
        fs::read_to_string(sandbox.installed().join("references/installation.md")).unwrap();
    assert!(instructions.contains("tracker with '\"'\"'quotes'\"'\"'' agent help"));
    let command = instructions
        .split("```sh\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    let executed = Command::new("sh").args(["-c", command]).output().unwrap();
    assert!(executed.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&executed.stdout).unwrap()["ok"],
        true
    );
    let backup = Path::new(response["installations"][0]["backup"].as_str().unwrap());
    assert!(backup.join("SKILL.md").exists());
}

#[test]
fn symlink_destinations_are_never_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let sandbox = Sandbox::new();
    let target = sandbox.root().join("owned-elsewhere");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "Keep me").unwrap();
    fs::create_dir_all(sandbox.installed().parent().unwrap()).unwrap();
    symlink(&target, sandbox.installed()).unwrap();
    let output = sandbox
        .command()
        .args(["skill", "install", "--force", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "Keep me"
    );
    assert!(
        fs::symlink_metadata(sandbox.installed())
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn concurrent_installers_publish_one_complete_skill() {
    let sandbox = Sandbox::new();
    let mut first = sandbox
        .command()
        .args(["skill", "install", "--json"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let second = sandbox.run(&["skill", "install", "--json"]);
    assert!(first.wait().unwrap().success());
    assert!(
        ["install", "unchanged"].contains(&second["installations"][0]["action"].as_str().unwrap())
    );
    let manifest: Value = serde_json::from_slice(
        &fs::read(sandbox.installed().join(".omatracker-install.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["files"].as_object().unwrap().len(), 6);
    assert_eq!(
        sandbox.run(&["skill", "install", "--json"])["installations"][0]["action"],
        "unchanged"
    );
}

#[test]
fn remove_only_selected_harnesses_preserves_data_and_other_skills() {
    let sandbox = Sandbox::new();
    sandbox.run(&[
        "skill",
        "install",
        "--harness",
        "shared,opencode,claude,gemini,cursor",
        "--json",
    ]);
    let ledger = sandbox.root().join("must-not-open/ledger.json");
    fs::create_dir_all(ledger.parent().unwrap()).unwrap();
    fs::write(&ledger, "untouched ledger").unwrap();
    let other_skill = sandbox.root().join("config/opencode/skills/another-skill");
    fs::create_dir_all(&other_skill).unwrap();
    fs::write(other_skill.join("SKILL.md"), "Another skill").unwrap();
    let result = sandbox.run(&["skill", "remove", "--harness", "opencode,claude", "--json"]);
    assert_eq!(result["removals"].as_array().unwrap().len(), 2);
    for removal in result["removals"].as_array().unwrap() {
        assert_eq!(removal["action"], "removed");
        assert!(removal["backup"].is_null());
        assert!(!Path::new(removal["path"].as_str().unwrap()).exists());
    }
    for folder in [".agents", ".gemini", ".cursor"] {
        assert!(
            sandbox
                .root()
                .join(folder)
                .join("skills/omatracker/SKILL.md")
                .is_file()
        );
    }
    assert_eq!(fs::read_to_string(&ledger).unwrap(), "untouched ledger");
    assert!(!ledger.with_extension("json.lock").exists());
    assert_eq!(
        fs::read_to_string(other_skill.join("SKILL.md")).unwrap(),
        "Another skill"
    );
    assert!(
        sandbox
            .command()
            .arg("--version")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn remove_supports_alias_shared_deduplication_and_missing_directories() {
    let sandbox = Sandbox::new();
    let absent = sandbox.run(&[
        "skill",
        "remove",
        "--harness",
        "opencode,claude,codex,gemini,cursor",
        "--json",
    ]);
    assert!(
        absent["removals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["action"] == "absent")
    );
    assert_eq!(fs::read_dir(sandbox.root()).unwrap().count(), 0);
    sandbox.run(&["skill", "install", "--json"]);
    let removed = sandbox.run(&["skill", "uninstall", "--harness", "codex,shared", "--json"]);
    assert_eq!(removed["removals"].as_array().unwrap().len(), 1);
    assert_eq!(
        removed["removals"][0]["harnesses"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(removed["removals"][0]["action"], "removed");
    assert_eq!(
        sandbox.run(&["skill", "remove", "--json"])["removals"][0]["action"],
        "absent"
    );
}

#[test]
fn remove_preflights_conflicts_and_force_preserves_customized_files() {
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--harness", "shared,claude", "--json"]);
    let customized = sandbox.root().join(".claude/skills/omatracker");
    fs::write(customized.join("notes.txt"), "Keep my notes").unwrap();
    let failed = sandbox
        .command()
        .args(["skill", "remove", "--harness", "shared,claude", "--json"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&failed.stdout).unwrap()["ok"],
        false
    );
    assert!(sandbox.installed().is_dir());
    let dry = sandbox.run(&[
        "skill",
        "remove",
        "--harness",
        "claude",
        "--force",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(dry["removals"][0]["action"], "remove-with-backup");
    assert!(dry["removals"][0]["backup"].is_null());
    assert!(
        !sandbox
            .root()
            .join(".claude/omatracker-skill-backups")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(customized.join("notes.txt")).unwrap(),
        "Keep my notes"
    );
    let result = sandbox.run(&[
        "skill",
        "remove",
        "--harness",
        "claude",
        "--force",
        "--json",
    ]);
    let backup = Path::new(result["removals"][0]["backup"].as_str().unwrap());
    assert_eq!(result["removals"][0]["action"], "removed");
    assert!(!customized.exists());
    assert!(!backup.starts_with(customized.parent().unwrap()));
    assert_eq!(
        fs::read_to_string(backup.join("notes.txt")).unwrap(),
        "Keep my notes"
    );
}

#[test]
fn remove_dry_run_writes_nothing_and_honors_custom_locations() {
    let sandbox = Sandbox::new();
    let result = sandbox.run(&["skill", "remove", "--dry-run", "--json"]);
    assert_eq!(result["dryRun"], true);
    assert_eq!(fs::read_dir(sandbox.root()).unwrap().count(), 0);
    let config = sandbox.root().join("custom claude");
    success(
        sandbox
            .command()
            .env("CLAUDE_CONFIG_DIR", &config)
            .args(["skill", "install", "--harness", "claude", "--json"])
            .output()
            .unwrap(),
    );
    let lock = config.join("skills/.omatracker-install.lock");
    fs::remove_file(&lock).unwrap();
    let result = success(
        sandbox
            .command()
            .env("CLAUDE_CONFIG_DIR", &config)
            .args([
                "skill",
                "remove",
                "--harness",
                "claude",
                "--dry-run",
                "--json",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["removals"][0]["action"], "remove");
    assert!(config.join("skills/omatracker/SKILL.md").exists());
    assert!(!lock.exists());
    success(
        sandbox
            .command()
            .env("CLAUDE_CONFIG_DIR", &config)
            .args(["skill", "remove", "--harness", "claude", "--json"])
            .output()
            .unwrap(),
    );
    assert!(!config.join("skills/omatracker").exists());
}

#[test]
fn remove_checks_installed_manifest_instead_of_current_bundle() {
    use sha2::{Digest, Sha256};
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--json"]);
    let manifest = sandbox.installed().join(".omatracker-install.json");
    let mut old: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    let previous = b"Documentation shipped by an older version";
    fs::write(sandbox.installed().join("AGENT_API.md"), previous).unwrap();
    old["version"] = "0.1.0".into();
    old["executable"] = "/previous/installation/omatracker".into();
    old["files"]["AGENT_API.md"] = format!("{:x}", Sha256::digest(previous)).into();
    fs::write(manifest, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        sandbox.run(&["skill", "remove", "--json"])["removals"][0]["action"],
        "removed"
    );
}

#[test]
fn remove_preserves_unmanaged_skills_and_never_follows_symlink_destinations() {
    use std::os::unix::fs::symlink;
    let sandbox = Sandbox::new();
    fs::create_dir_all(sandbox.installed()).unwrap();
    fs::write(sandbox.installed().join("SKILL.md"), "Unmanaged skill").unwrap();
    assert!(
        !sandbox
            .command()
            .args(["skill", "remove", "--json"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let forced = sandbox.run(&["skill", "remove", "--force", "--json"]);
    let backup = Path::new(forced["removals"][0]["backup"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(backup.join("SKILL.md")).unwrap(),
        "Unmanaged skill"
    );
    symlink(backup, sandbox.installed()).unwrap();
    assert!(
        !sandbox
            .command()
            .args(["skill", "remove", "--force", "--json"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        fs::symlink_metadata(sandbox.installed())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(backup.join("SKILL.md")).unwrap(),
        "Unmanaged skill"
    );
    fs::remove_file(sandbox.installed()).unwrap();
    symlink(sandbox.root().join("nonexistent"), sandbox.installed()).unwrap();
    assert!(
        !sandbox
            .command()
            .args(["skill", "remove", "--force", "--json"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn concurrent_removers_are_repeatable() {
    let sandbox = Sandbox::new();
    sandbox.run(&["skill", "install", "--json"]);
    let first = sandbox
        .command()
        .args(["skill", "remove", "--json"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let second = sandbox.run(&["skill", "remove", "--json"]);
    let first = success(first.wait_with_output().unwrap());
    let actions = [
        first["removals"][0]["action"].as_str().unwrap(),
        second["removals"][0]["action"].as_str().unwrap(),
    ];
    assert!(actions.contains(&"removed"));
    assert!(actions.contains(&"absent"));
    assert!(!sandbox.installed().exists());
}
