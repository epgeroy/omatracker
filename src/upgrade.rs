//! Staged user-level upgrades. All mutable installation paths are backed up first.
use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const REPO: &str = "epgeroy/omatracker";
const URL: &str = "https://github.com/epgeroy/omatracker.git";
const ASSET: &str = "omatracker-linux-x86_64";

#[derive(Args)]
pub struct Upgrade {
    /// Build and install the main branch (pinned to the resolved commit).
    #[arg(long, conflicts_with_all = ["tag", "source"])]
    edge: bool,
    /// Install a specific published release tag instead of the latest release.
    #[arg(long, conflicts_with = "source")]
    tag: Option<String>,
    /// Use a local, prebuilt source checkout (useful for offline validation).
    #[arg(long, hide = true)]
    source: Option<PathBuf>,
    /// Recover an interrupted upgrade from its pre-upgrade backup.
    #[arg(long, conflicts_with_all = ["source", "tag", "edge", "dry_run"])]
    recover: Option<PathBuf>,
    /// Resolve and validate the candidate without touching the installation.
    #[arg(long)]
    dry_run: bool,
    /// Skip shell restart for a headless installation.
    #[arg(long)]
    no_shell: bool,
    /// Exercise rollback after migration (isolated upgrade integration tests).
    #[arg(long, hide = true)]
    fail_after_migration: bool,
}

fn output<P, I, S>(program: P, args: I) -> Result<String>
where
    P: AsRef<OsStr>,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let result = Command::new(program).args(args).output()?;
    if !result.status.success() {
        bail!(
            "UPGRADE_COMMAND: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
    }
    Ok(String::from_utf8(result.stdout)?.trim().into())
}

fn tag(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        bail!("UPGRADE_TAG: invalid release tag")
    }
    Ok(value)
}

fn install_path() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let home = crate::home_dir()?;
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    Ok((
        data.join("omatracker"),
        config.join("omarchy/plugins/epgeroy.omatracker"),
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/state"))
            .join("omatracker/plugin-backups"),
    ))
}

fn prepare(args: &Upgrade, stage: &Path) -> Result<(PathBuf, String)> {
    if let Some(source) = &args.source {
        let source = source.canonicalize()?;
        if !source.join("manifest.json").is_file()
            || !source.join("scripts/install-plugin.py").is_file()
        {
            bail!("UPGRADE_SOURCE: expected an OmaTracker source checkout")
        }
        return Ok((source, "local".into()));
    }
    let reference = if args.edge {
        "main".into()
    } else if let Some(value) = &args.tag {
        tag(value)?.to_owned()
    } else {
        tag(&output(
            "gh",
            [
                "release", "view", "--repo", REPO, "--json", "tagName", "--jq", ".tagName",
            ],
        )?)?
        .into()
    };
    let checkout = stage.join("source");
    output(
        "git",
        [
            "clone",
            "--depth",
            "1",
            "--branch",
            &reference,
            URL,
            checkout
                .to_str()
                .context("UPGRADE_PATH: source is not UTF-8")?,
        ],
    )?;
    let sha = output(
        "git",
        ["-C", checkout.to_str().unwrap(), "rev-parse", "HEAD"],
    )?;
    if args.edge {
        // The checkout commits to the exact SHA that will be installed, even if main moves later.
        output(
            "cargo",
            [
                "build",
                "--release",
                "--manifest-path",
                checkout.join("Cargo.toml").to_str().unwrap(),
            ],
        )?;
        fs::copy(
            checkout.join("target/release/omatracker"),
            checkout.join("bin/omatracker"),
        )?;
    } else {
        let asset = stage.join(ASSET);
        let checksum = stage.join(format!("{ASSET}.sha256"));
        for name in [ASSET, "omatracker-linux-x86_64.sha256"] {
            output(
                "gh",
                [
                    "release",
                    "download",
                    &reference,
                    "--repo",
                    REPO,
                    "--pattern",
                    name,
                    "--dir",
                    stage.to_str().unwrap(),
                    "--clobber",
                ],
            )?;
        }
        let wanted = fs::read_to_string(checksum)?
            .split_whitespace()
            .next()
            .context("UPGRADE_CHECKSUM: empty checksum")?
            .to_owned();
        let actual = format!("{:x}", Sha256::digest(fs::read(&asset)?));
        if actual != wanted {
            bail!("UPGRADE_CHECKSUM: downloaded release does not match")
        }
        fs::copy(asset, checkout.join("bin/omatracker"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                checkout.join("bin/omatracker"),
                fs::Permissions::from_mode(0o755),
            )?;
        }
    }
    Ok((checkout, format!("{reference}@{sha}")))
}

fn validate(source: &Path, ledger: &Path) -> Result<String> {
    let bin = source.join("bin/omatracker");
    let version = output(&bin, ["--version"])?;
    let manifest: Value = serde_json::from_slice(&fs::read(source.join("manifest.json"))?)?;
    if manifest["id"] != "epgeroy.omatracker"
        || version != format!("omatracker {}", manifest["version"].as_str().unwrap_or(""))
    {
        bail!("UPGRADE_VERSION: binary and widget manifest disagree")
    }
    // A read-only compatibility check against the real ledger; writes only happen after backup.
    if ledger.exists() {
        output(
            &bin,
            [
                "--data-path",
                ledger
                    .to_str()
                    .context("UPGRADE_PATH: ledger is not UTF-8")?,
                "status",
                "--json",
                "--compact",
            ],
        )?;
    }
    Ok(version)
}

fn replace(source: &Path, ledger: &Path) -> Result<()> {
    let (data, plugin, backups) = install_path()?;
    let binary = data.join("bin/omatracker");
    fs::create_dir_all(binary.parent().unwrap())?;
    // Stage on the destination filesystem, then rename. The old process can finish in Linux.
    let staged = tempfile::NamedTempFile::new_in(binary.parent().unwrap())?;
    fs::copy(source.join("bin/omatracker"), staged.path())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staged.path(), fs::Permissions::from_mode(0o755))?;
    }
    staged.persist(&binary).map_err(|e| e.error)?;
    let bundled = data.join("templates");
    fs::create_dir_all(&bundled)?;
    for file in fs::read_dir(source.join("templates"))? {
        let file = file?;
        if file.path().extension().is_some_and(|v| v == "typ") {
            crate::atomic_write(&bundled.join(file.file_name()), &fs::read(file.path())?)?;
        }
    }
    output(
        "python3",
        [
            source.join("scripts/install-plugin.py").as_os_str(),
            OsStr::new("--source"),
            source.as_os_str(),
            OsStr::new("--destination"),
            plugin.as_os_str(),
            OsStr::new("--backend"),
            binary.as_os_str(),
            OsStr::new("--backup-root"),
            backups.as_os_str(),
        ],
    )?;
    // Existing harnesses are updated; an unconfigured user gets the shared skill.
    let home = crate::home_dir()?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let mut harnesses = Vec::new();
    for (name, dir) in [
        ("shared", home.join(".agents")),
        ("opencode", config.join("opencode")),
        ("claude", home.join(".claude")),
        ("gemini", home.join(".gemini")),
        ("cursor", home.join(".cursor")),
    ] {
        if dir
            .join("skills/omatracker/.omatracker-install.json")
            .is_file()
        {
            harnesses.push(name);
        }
    }
    if harnesses.is_empty() {
        harnesses.push("shared");
    }
    let mut skill = Command::new(&binary);
    skill.args(["skill", "install"]);
    for harness in &harnesses {
        skill.args(["--harness", harness]);
    }
    let installed = skill.output()?;
    if !installed.status.success() {
        bail!(
            "UPGRADE_SKILL: {}",
            String::from_utf8_lossy(&installed.stderr)
        )
    }
    let mut upgrade_skill = Command::new(&binary);
    upgrade_skill.args(["skill", "install-upgrade"]);
    for harness in &harnesses {
        upgrade_skill.args(["--harness", harness]);
    }
    let installed = upgrade_skill.output()?;
    if !installed.status.success() {
        bail!(
            "UPGRADE_SKILL: {}",
            String::from_utf8_lossy(&installed.stderr)
        )
    }
    output(
        &binary,
        [
            "--data-path",
            ledger.to_str().unwrap(),
            "status",
            "--json",
            "--compact",
        ],
    )?;
    Ok(())
}

pub fn run(ledger: &Path, args: Upgrade) -> Result<()> {
    if let Some(path) = &args.recover {
        let journal: Value = serde_json::from_slice(&fs::read(path.join("upgrade.json"))?)?;
        if journal["status"] == "healthy" || journal["status"] == "rolledBack" {
            bail!("UPGRADE_RECOVERY: completed upgrades require an explicit backup restore")
        }
        let revision = journal["revision"]
            .as_u64()
            .context("UPGRADE_RECOVERY: missing revision")?;
        let restored = crate::backup::restore(ledger, path, false, Some(revision))?;
        if !args.no_shell {
            output("omarchy", ["restart", "shell"])?;
        }
        crate::atomic_write(
            &path.join("upgrade.json"),
            &serde_json::to_vec_pretty(
                &json!({"status":"rolledBack","recovery":restored["recovery"]}),
            )?,
        )?;
        println!("{restored}");
        return Ok(());
    }
    let stage = tempfile::tempdir()?;
    let (source, reference) = prepare(&args, stage.path())?;
    let version = validate(&source, ledger)?;
    if args.dry_run {
        println!(
            "{}",
            json!({"dryRun":true,"reference":reference,"version":version})
        );
        return Ok(());
    }
    let (data, plugin, _) = install_path()?;
    if !data.join("bin/omatracker").is_file() {
        bail!(
            "UPGRADE_INSTALL: install the standalone CLI with `make install` first; no binary to roll back"
        )
    }
    if plugin.join(".git").exists() {
        let dirty = output(
            "git",
            [
                "-C",
                plugin
                    .to_str()
                    .context("UPGRADE_PATH: plugin is not UTF-8")?,
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
        )?;
        if !dirty.is_empty() {
            bail!(
                "UPGRADE_INSTALL: live plugin checkout has local changes; preserve or commit them before upgrading"
            )
        }
    }
    let backup = crate::backup::create(ledger)?;
    let before = backup["revision"]
        .as_u64()
        .context("UPGRADE_BACKUP: missing revision")?;
    let expected = std::cell::Cell::new(before);
    let path = backup["path"]
        .as_str()
        .context("UPGRADE_BACKUP: no path")?
        .to_owned();
    crate::backup::verify(ledger, Path::new(&path))?;
    let journal_path = Path::new(&path).join("upgrade.json");
    let journal = |status: &str| {
        crate::atomic_write(
            &journal_path,
            &serde_json::to_vec_pretty(
                &json!({"status":status,"reference":reference,"version":version,
            "revision":expected.get(),"backup":path}),
            )?,
        )
    };
    journal("installing")?;
    let live_widget = !args.no_shell && output("omarchy-shell", ["omatracker", "status"]).is_ok();
    let result = (|| -> Result<()> {
        replace(&source, ledger)?;
        let input = serde_json::to_string(&json!({"revision":before}))?;
        let migration: Value = serde_json::from_str(&output(
            source.join("bin/omatracker"),
            [
                "--data-path",
                ledger
                    .to_str()
                    .context("UPGRADE_PATH: ledger is not UTF-8")?,
                "agent",
                "migration.apply",
                "--input",
                &input,
            ],
        )?)?;
        if migration["ok"] != true {
            bail!("UPGRADE_MIGRATION: migration failed")
        }
        expected.set(
            migration["revision"]
                .as_u64()
                .context("UPGRADE_MIGRATION: no revision")?,
        );
        journal("migrated")?;
        if args.fail_after_migration {
            bail!("UPGRADE_TEST: injected health failure")
        }
        if live_widget {
            output("omarchy", ["restart", "shell"])?;
            let mut healthy = false;
            for _ in 0..40 {
                if let Ok(reply) = output("omarchy-shell", ["omatracker", "status"])
                    && let Ok(status) = serde_json::from_str::<Value>(&reply)
                    && status["loaded"] == true
                    && status["error"].as_str().is_none_or(str::is_empty)
                {
                    healthy = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            if !healthy {
                bail!("UPGRADE_HEALTH: installed widget did not load cleanly")
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        match crate::backup::restore(ledger, Path::new(&path), false, Some(expected.get())) {
            Ok(recovery) => {
                journal("rolledBack")?;
                if live_widget {
                    let _ = output("omarchy", ["restart", "shell"]);
                }
                bail!(
                    "UPGRADE_FAILED: {error}; rolled back from {path}; recovery: {}",
                    recovery["recovery"]
                )
            }
            Err(rollback) => {
                journal("needsRecovery")?;
                bail!(
                    "UPGRADE_FAILED: {error}; automatic rollback stopped: {rollback}; backup: {path}"
                )
            }
        }
    }
    journal("healthy")?;
    println!(
        "{}",
        json!({"upgraded":true,"reference":reference,"version":version,"backup":path})
    );
    Ok(())
}
