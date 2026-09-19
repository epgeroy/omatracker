//! User-level skill installation. The binary embeds its matching instructions and
//! references, so installation works from any working directory or standalone binary.
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST: &str = ".omatracker-install.json";
const OWNER: &str = "omatracker-skill";

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Harness {
    /// Cross-harness Agent Skills directory (also used by Codex).
    Shared,
    Opencode,
    #[value(alias = "claude-code")]
    Claude,
    Codex,
    #[value(alias = "gemini-cli")]
    Gemini,
    Cursor,
}

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Show supported harnesses and their resolved user-level destinations.
    Targets {
        #[arg(long)]
        json: bool,
    },
    /// Install/update the skill for the current user (no sudo needed).
    Install {
        /// Repeat this option or provide comma-separated names. Defaults to shared.
        #[arg(long, value_enum, value_delimiter = ',', default_value = "shared")]
        harness: Vec<Harness>,
        /// Print the installation plan without creating directories or files.
        #[arg(long)]
        dry_run: bool,
        /// Replace a modified/unmanaged installation, keeping a backup outside skills/.
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove a user-level skill installation (leaves the CLI and ledger intact).
    #[command(visible_alias = "uninstall")]
    Remove {
        /// Repeat this option or provide comma-separated names. Defaults to shared.
        #[arg(long, value_enum, value_delimiter = ',', default_value = "shared")]
        harness: Vec<Harness>,
        /// Print the removal plan without creating or removing any files.
        #[arg(long)]
        dry_run: bool,
        /// Back up a modified/unmanaged skill outside skills/ before removing it.
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    harness: Harness,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Installation {
    harnesses: Vec<Harness>,
    path: PathBuf,
    action: String,
    backup: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    owner: String,
    format_version: u32,
    version: String,
    executable: PathBuf,
    files: BTreeMap<String, String>,
}

fn absolute_env(name: &str, fallback: PathBuf) -> Result<PathBuf> {
    match std::env::var_os(name).filter(|v| !v.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("{name} must be an absolute path")
            }
            Ok(path)
        }
        None => Ok(fallback),
    }
}

fn destination(harness: Harness) -> Result<PathBuf> {
    let home = crate::home_dir()?;
    if !home.is_absolute() {
        bail!("HOME must be an absolute path")
    }
    let base = match harness {
        Harness::Shared | Harness::Codex => home.join(".agents"),
        Harness::Opencode => {
            absolute_env("XDG_CONFIG_HOME", home.join(".config"))?.join("opencode")
        }
        Harness::Claude => absolute_env("CLAUDE_CONFIG_DIR", home.join(".claude"))?,
        Harness::Gemini => home.join(".gemini"),
        Harness::Cursor => home.join(".cursor"),
    };
    Ok(base.join("skills/omatracker"))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bundle() -> Result<BTreeMap<String, Vec<u8>>> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let path = executable
        .to_str()
        .context("executable path is not valid UTF-8")?;
    if path.chars().any(char::is_control) {
        bail!("executable path contains control characters")
    }
    let quoted = format!("'{}'", path.replace('\'', "'\"'\"'"));
    let installation = format!(
        "# Local installation\n\nThis skill was installed by OmaTracker {}.\n\n\
         Use this exact executable from any working directory:\n\n\
         ```sh\n{quoted} agent help\n```\n\n\
         Keep the executable path quoted. Append `--data-path /absolute/path/ledger.json`\n\
         before `agent` when the user specifies a ledger. Never substitute a similarly\n\
         named program in the current project. If this executable moves, rerun\n\
         `skill install` from its new location to refresh this installation.\n\n\
         Read `../AGENT_API.md` for the matching request contract. The skill root also\n\
         includes `TEMPLATES.md` and `tests/manual-invoices.md`; resolve documentation\n\
         paths relative to the skill root, not the agent's current working directory.\n",
        env!("CARGO_PKG_VERSION")
    );
    let skill = include_str!("../skills/omatracker/SKILL.md").replacen(
        "# OmaTracker\n",
        "# OmaTracker\n\n**Installed skill:** first read `references/installation.md` for the exact CLI\npath. All documentation paths below resolve against this skill directory.\n",
        1,
    );
    let mut files: BTreeMap<String, Vec<u8>> = [
        ("SKILL.md", skill.as_str()),
        ("references/installation.md", installation.as_str()),
        (
            "references/workflows.md",
            include_str!("../skills/omatracker/references/workflows.md"),
        ),
        ("AGENT_API.md", include_str!("../AGENT_API.md")),
        ("TEMPLATES.md", include_str!("../TEMPLATES.md")),
        (
            "tests/manual-invoices.md",
            include_str!("../tests/manual-invoices.md"),
        ),
    ]
    .into_iter()
    .map(|(name, text)| (name.to_owned(), text.as_bytes().to_vec()))
    .collect();
    let manifest = Manifest {
        owner: OWNER.into(),
        format_version: 1,
        version: env!("CARGO_PKG_VERSION").into(),
        executable,
        files: files
            .iter()
            .map(|(name, bytes)| (name.clone(), digest(bytes)))
            .collect(),
    };
    files.insert(MANIFEST.into(), serde_json::to_vec_pretty(&manifest)?);
    Ok(files)
}

fn tree_hashes(root: &Path, directory: &Path, hashes: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            tree_hashes(root, &entry.path(), hashes)?;
        } else if kind.is_file() {
            let name = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("skill filename is not UTF-8")?
                .to_owned();
            hashes.insert(name, digest(&fs::read(entry.path())?));
        } else {
            bail!(
                "skill contains a symlink or special file: {}",
                entry.path().display()
            )
        }
    }
    Ok(())
}

fn action(path: &Path, files: &BTreeMap<String, Vec<u8>>, force: bool) -> Result<String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("install".into()),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!(
            "{} is not a regular directory; move it aside before installing",
            path.display()
        )
    }
    let mut hashes = BTreeMap::new();
    let inspected = tree_hashes(path, path, &mut hashes).is_ok();
    let expected: BTreeMap<_, _> = files
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect();
    if inspected && hashes == expected {
        return Ok("unchanged".into());
    }
    let managed = fs::read(path.join(MANIFEST))
        .ok()
        .and_then(|v| serde_json::from_slice::<Manifest>(&v).ok());
    hashes.remove(MANIFEST);
    if inspected
        && managed.is_some_and(|m| m.owner == OWNER && m.format_version == 1 && m.files == hashes)
    {
        return Ok("update".into());
    }
    if force {
        return Ok("replace".into());
    }
    bail!(
        "{} contains an unmanaged or locally modified skill; use --force to back it up and replace it",
        path.display()
    )
}

fn install(plan: &mut Installation, files: &BTreeMap<String, Vec<u8>>, force: bool) -> Result<()> {
    let parent = plan
        .path
        .parent()
        .context("skill destination has no parent")?;
    fs::create_dir_all(parent)?;
    let _lock = crate::lock_file(&parent.join(".omatracker-install"))?;
    plan.action = action(&plan.path, files, force)?;
    if plan.action == "unchanged" {
        return Ok(());
    }
    let staging = tempfile::Builder::new()
        .prefix(".omatracker-install-")
        .tempdir_in(parent)?;
    for (name, content) in files {
        let file = staging.path().join(name);
        fs::create_dir_all(file.parent().unwrap())?;
        fs::write(file, content)?;
    }
    if plan.action != "install" {
        // Outside skills/ so harnesses cannot discover backup SKILL.md files.
        let backups = parent
            .parent()
            .context("skills root has no parent")?
            .join("omatracker-skill-backups");
        fs::create_dir_all(&backups)?;
        let backup = backups.join(crate::make_id("omatracker"));
        fs::rename(&plan.path, &backup)?;
        plan.backup = Some(backup);
    }
    if let Err(error) = fs::rename(staging.path(), &plan.path) {
        if let Some(backup) = &plan.backup {
            fs::rename(backup, &plan.path).with_context(|| {
                format!(
                    "installation failed ({error}); restore skill from {}",
                    backup.display()
                )
            })?;
        }
        return Err(error.into());
    }
    Ok(())
}

fn removal_action(path: &Path, force: bool) -> Result<String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("absent".into()),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!(
            "{} is not a regular directory; move it aside before removing",
            path.display()
        )
    }
    // Compare with the installed manifest, not this binary's current bundle:
    // older installs and installs pointing at a different executable are valid.
    let mut hashes = BTreeMap::new();
    let inspected = tree_hashes(path, path, &mut hashes).is_ok();
    let managed = fs::read(path.join(MANIFEST))
        .ok()
        .and_then(|v| serde_json::from_slice::<Manifest>(&v).ok());
    hashes.remove(MANIFEST);
    if inspected
        && managed.is_some_and(|m| m.owner == OWNER && m.format_version == 1 && m.files == hashes)
    {
        return Ok("remove".into());
    }
    if force {
        return Ok("remove-with-backup".into());
    }
    bail!(
        "{} contains an unmanaged or locally modified skill; use --force to back it up and remove it",
        path.display()
    )
}

fn remove(plan: &mut Installation, force: bool) -> Result<()> {
    if plan.action == "absent" {
        return Ok(());
    }
    let parent = plan
        .path
        .parent()
        .context("skill destination has no parent")?;
    let _lock = crate::lock_file(&parent.join(".omatracker-install"))?;
    // Recheck while holding the installer's lock, including concurrent removals.
    plan.action = removal_action(&plan.path, force)?;
    match plan.action.as_str() {
        "absent" => return Ok(()),
        "remove-with-backup" => {
            let backups = parent
                .parent()
                .context("skills root has no parent")?
                .join("omatracker-skill-backups");
            fs::create_dir_all(&backups)?;
            let backup = backups.join(crate::make_id("omatracker"));
            fs::rename(&plan.path, &backup)?;
            plan.backup = Some(backup);
        }
        "remove" => fs::remove_dir_all(&plan.path)?,
        _ => unreachable!("removal_action returns only removal actions"),
    }
    plan.action = "removed".into();
    Ok(())
}

fn removal_preflight(path: &Path, force: bool, dry_run: bool) -> Result<String> {
    if dry_run || fs::symlink_metadata(path).is_err() {
        return removal_action(path, force);
    }
    let parent = path.parent().context("skill destination has no parent")?;
    let _lock = crate::lock_file(&parent.join(".omatracker-install"))?;
    removal_action(path, force)
}

fn execute(command: &SkillCommand) -> Result<Value> {
    match command {
        SkillCommand::Targets { .. } => {
            let targets = Harness::value_variants()
                .iter()
                .map(|&harness| {
                    Ok(Target {
                        harness,
                        path: destination(harness)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({"schemaVersion":1,"ok":true,"scope":"user","targets":targets}))
        }
        SkillCommand::Remove {
            harness,
            dry_run,
            force,
            ..
        } => {
            let mut plans: Vec<Installation> = Vec::new();
            for &harness in harness {
                let path = destination(harness)?;
                if let Some(plan) = plans.iter_mut().find(|p| p.path == path) {
                    plan.harnesses.push(harness);
                } else {
                    plans.push(Installation {
                        harnesses: vec![harness],
                        action: removal_preflight(&path, *force, *dry_run)?,
                        path,
                        backup: None,
                    });
                }
            }
            // As with install, preflight every target before changing any of them.
            if !dry_run {
                for plan in &mut plans {
                    remove(plan, *force)?;
                }
            }
            Ok(
                json!({"schemaVersion":1,"ok":true,"scope":"user","dryRun":dry_run,"removals":plans,
                "nextStep":"Quit and restart OpenCode; restart/reload skills in other harnesses to refresh the skill list."}),
            )
        }
        SkillCommand::Install {
            harness,
            dry_run,
            force,
            ..
        } => {
            let files = bundle()?;
            let mut plans: Vec<Installation> = Vec::new();
            for &harness in harness {
                let path = destination(harness)?;
                if let Some(plan) = plans.iter_mut().find(|p| p.path == path) {
                    plan.harnesses.push(harness);
                } else {
                    plans.push(Installation {
                        harnesses: vec![harness],
                        action: action(&path, &files, *force)?,
                        path,
                        backup: None,
                    });
                }
            }
            // Preflight every target before writing any of them; each publication is locked.
            if !dry_run {
                for plan in &mut plans {
                    install(plan, &files, *force)?;
                }
            }
            Ok(
                json!({"schemaVersion":1,"ok":true,"scope":"user","dryRun":dry_run,"installations":plans,
                "nextStep":"Quit and restart OpenCode; restart/reload skills in other harnesses to discover OmaTracker."}),
            )
        }
    }
}

pub fn run(command: &SkillCommand) -> Result<()> {
    let as_json = match command {
        SkillCommand::Targets { json }
        | SkillCommand::Install { json, .. }
        | SkillCommand::Remove { json, .. } => *json,
    };
    let value = match execute(command) {
        Ok(value) => value,
        Err(error) if as_json => {
            println!("{}", crate::agent::error(&error));
            std::process::exit(1);
        }
        Err(error) => return Err(error),
    };
    if as_json {
        println!("{value}");
    } else if let Some(targets) = value["targets"].as_array() {
        for target in targets {
            println!(
                "{}\t{}",
                target["harness"].as_str().unwrap(),
                target["path"].as_str().unwrap()
            );
        }
    } else {
        let prefix = if value["dryRun"] == true {
            "Dry run: "
        } else {
            ""
        };
        let operations = value["installations"]
            .as_array()
            .or_else(|| value["removals"].as_array())
            .unwrap();
        for item in operations {
            println!(
                "{prefix}{}: {}",
                item["action"].as_str().unwrap(),
                item["path"].as_str().unwrap()
            );
            if let Some(backup) = item["backup"].as_str() {
                println!("Backup: {backup}");
            }
        }
        println!("{}", value["nextStep"].as_str().unwrap());
    }
    Ok(())
}
