//! Versioned, local snapshots. A restore first snapshots the state it replaces.
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;

#[derive(Subcommand)]
pub enum BackupCommand {
    /// Save the ledger, documents, templates, preferences and installed runtime.
    Create,
    /// List local snapshots for this ledger.
    List,
    /// Check the snapshot manifest and every file's SHA-256 digest.
    Verify { path: PathBuf },
    /// Preview or restore a verified snapshot. A recovery snapshot is made first.
    Restore {
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
        /// Refuse a rollback if the ledger changed since the upgrade began.
        #[arg(long)]
        expect_revision: Option<u64>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    slot: String,
    relative: String,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format: u32,
    ledger: PathBuf,
    version: String,
    revision: u64,
    created_at: i64,
    present_slots: BTreeSet<String>,
    entries: Vec<Entry>,
}

fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.into()
    } else {
        std::env::current_dir()?.join(path)
    };
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        bail!("BACKUP_PATH: parent components are not allowed")
    }
    Ok(path)
}

fn backup_root(ledger: &Path) -> PathBuf {
    PathBuf::from(format!("{}.backups", ledger.display()))
}

fn roots(
    ledger: &Path,
    projects: &[crate::Project],
    require_logos: bool,
) -> Result<BTreeMap<String, PathBuf>> {
    let home = crate::home_dir()?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let mut result = BTreeMap::new();
    for (name, path) in [
        ("ledger", ledger.to_owned()),
        (
            "pre-invoices",
            PathBuf::from(format!("{}.pre-invoices.bak", ledger.display())),
        ),
        (
            "pre-task-rates",
            PathBuf::from(format!("{}.pre-task-rates.bak", ledger.display())),
        ),
        (
            "feedback",
            PathBuf::from(format!("{}.feedback.json", ledger.display())),
        ),
        (
            "invoices",
            PathBuf::from(format!("{}.invoices", ledger.display())),
        ),
        (
            "workflows",
            PathBuf::from(format!("{}.workflows", ledger.display())),
        ),
        ("templates", config.join("omarchy/omatracker/templates")),
        ("reports", crate::cache_path()?),
        ("binary", data.join("omatracker/bin/omatracker")),
        ("bundled-templates", data.join("omatracker/templates")),
        ("plugin", config.join("omarchy/plugins/epgeroy.omatracker")),
    ] {
        result.insert(name.into(), path);
    }
    for (name, base) in [
        ("shared", home.join(".agents")),
        ("opencode", config.join("opencode")),
        (
            "claude",
            std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude")),
        ),
        ("gemini", home.join(".gemini")),
        ("cursor", home.join(".cursor")),
    ] {
        for skill in ["omatracker", "omatracker-upgrade"] {
            let path = base.join("skills").join(skill);
            result.insert(format!("skill-{name}-{skill}"), path);
        }
    }
    let mut logos = BTreeSet::new();
    for project in projects {
        if !project.logo_path.is_empty() {
            let logo = absolute(Path::new(&project.logo_path))?;
            if !logo.starts_with(&home) || (require_logos && !logo.is_file()) {
                bail!(
                    "BACKUP_SCOPE: referenced logo must be a local file under HOME: {}",
                    logo.display()
                )
            }
            logos.insert(logo);
        }
    }
    for (index, logo) in logos.into_iter().enumerate() {
        result.insert(format!("logo-{index}"), logo);
    }
    Ok(result)
}

fn safe_relative(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("BACKUP_PATH: unsafe snapshot member {path}")
    }
    Ok(p.into())
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn walk(
    source: &Path,
    slot: &str,
    rel: &Path,
    staging: &Path,
    entries: &mut Vec<Entry>,
) -> Result<()> {
    let kind = fs::symlink_metadata(source)?.file_type();
    if kind.is_symlink() {
        // The plugin executable is a link to the backed-up standalone binary.
        if slot == "plugin" && rel == Path::new("bin/omatracker") {
            return Ok(());
        }
        bail!("BACKUP_SCOPE: refusing symlink {}", source.display())
    }
    if kind.is_dir() {
        for item in fs::read_dir(source)? {
            let item = item?;
            let child = rel.join(item.file_name());
            // A development checkout can also be the live plugin directory.
            if slot == "plugin" {
                let name = item.file_name().to_string_lossy().into_owned();
                if rel.as_os_str().is_empty()
                    && name != "manifest.json"
                    && name != "templates"
                    && name != "sounds"
                    && !name.ends_with(".qml")
                    && !name.ends_with(".js")
                {
                    continue;
                }
            }
            walk(&item.path(), slot, &child, staging, entries)?;
        }
    } else if kind.is_file() {
        let bytes = fs::read(source)?;
        let relative = if rel.as_os_str().is_empty() {
            "file".into()
        } else {
            rel.to_string_lossy().into_owned()
        };
        let target = staging.join("files").join(slot).join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(&target, &bytes)?;
        entries.push(Entry {
            slot: slot.into(),
            relative,
            sha256: sha(&bytes),
        });
    } else {
        bail!("BACKUP_SCOPE: refusing special file {}", source.display())
    }
    Ok(())
}

fn manifest(path: &Path, ledger: &Path) -> Result<Manifest> {
    if !path.starts_with(backup_root(ledger))
        || fs::symlink_metadata(path)?.file_type().is_symlink()
        || fs::symlink_metadata(backup_root(ledger))?
            .file_type()
            .is_symlink()
    {
        bail!("BACKUP_PATH: snapshot must be inside this ledger's backup directory")
    }
    let m: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    if m.format != 1 || m.ledger != ledger {
        bail!("BACKUP_FORMAT: wrong ledger or format")
    }
    let original = fs::read(path.join("files/ledger/file"))?;
    if !m
        .entries
        .iter()
        .any(|e| e.slot == "ledger" && e.relative == "file" && e.sha256 == sha(&original))
    {
        bail!("BACKUP_CORRUPT: ledger checksum mismatch")
    }
    let saved = crate::parse_state(std::str::from_utf8(&original)?)?;
    let allowed = roots(ledger, &saved.projects, false)?;
    if !m
        .present_slots
        .iter()
        .all(|slot| allowed.contains_key(slot))
    {
        bail!("BACKUP_FORMAT: unknown slot")
    }
    let mut seen = BTreeSet::new();
    for entry in &m.entries {
        if !m.present_slots.contains(&entry.slot) || !seen.insert((&entry.slot, &entry.relative)) {
            bail!("BACKUP_FORMAT: unknown or duplicate member")
        }
        let file_slot = matches!(
            entry.slot.as_str(),
            "ledger" | "feedback" | "binary" | "pre-invoices" | "pre-task-rates"
        ) || entry.slot.starts_with("logo-");
        if (entry.relative == "file") != file_slot {
            bail!("BACKUP_FORMAT: invalid file slot")
        }
        let relative = safe_relative(&entry.relative)?;
        let file = path.join("files").join(&entry.slot).join(relative);
        if !file.starts_with(path.join("files"))
            || !fs::symlink_metadata(&file)?.is_file()
            || sha(&fs::read(&file)?) != entry.sha256
        {
            bail!("BACKUP_CORRUPT: {}", file.display())
        }
    }
    if !m.entries.iter().any(|e| e.slot == "ledger") {
        bail!("BACKUP_FORMAT: missing ledger")
    }
    Ok(m)
}

fn locks(ledger: &Path) -> Result<Vec<fs::File>> {
    let mut held = Vec::new();
    for suffix in [
        "workflows-worker",
        "agent-external",
        "invoices-worker",
        "reports",
        "sync-worker",
    ] {
        held.push(crate::lock_file(&PathBuf::from(format!(
            "{}.{}",
            ledger.display(),
            suffix
        )))?);
    }
    held.push(crate::lock_file(ledger)?);
    Ok(held)
}

fn create_snapshot(ledger: &Path, allow_missing_logo: bool) -> Result<Value> {
    let ledger = absolute(ledger)?;
    let _held = locks(&ledger)?;
    let state = crate::read_state(&ledger)?;
    if !ledger.is_file() {
        bail!("BACKUP_MISSING: ledger does not exist")
    }
    let root = backup_root(&ledger);
    if fs::symlink_metadata(&root).is_ok_and(|s| s.file_type().is_symlink()) {
        bail!("BACKUP_SCOPE: backup directory cannot be a symlink")
    }
    fs::create_dir_all(&root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    let staging = TempDir::new_in(&root)?;
    let mut entries = Vec::new();
    let mut present_slots = BTreeSet::new();
    for (slot, path) in roots(&ledger, &state.projects, !allow_missing_logo)? {
        if slot.starts_with("skill-") && !path.join(".omatracker-install.json").is_file() {
            continue;
        }
        if path.exists() {
            present_slots.insert(slot.clone());
            walk(&path, &slot, Path::new(""), staging.path(), &mut entries)?;
        }
    }
    let snapshot = Manifest {
        format: 1,
        ledger: ledger.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        revision: state.billing.revision,
        created_at: crate::now_ms(),
        present_slots,
        entries,
    };
    fs::write(
        staging.path().join("manifest.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    let path = root.join(format!("backup-{}", uuid::Uuid::new_v4()));
    fs::rename(staging.keep(), &path)?;
    manifest(&path, &ledger)?;
    Ok(
        json!({"path":path,"revision":snapshot.revision,"files":snapshot.entries.len(),"version":snapshot.version}),
    )
}

pub fn create(ledger: &Path) -> Result<Value> {
    create_snapshot(ledger, false)
}

pub fn verify(ledger: &Path, path: &Path) -> Result<Value> {
    let ledger = absolute(ledger)?;
    let path = absolute(path)?;
    let m = manifest(&path, &ledger)?;
    Ok(
        json!({"path":path,"revision":m.revision,"files":m.entries.len(),"version":m.version,"createdAt":m.created_at,"valid":true}),
    )
}

pub fn list(ledger: &Path) -> Result<Value> {
    let ledger = absolute(ledger)?;
    let root = backup_root(&ledger);
    let mut items = Vec::new();
    if root.exists() {
        for item in fs::read_dir(root)? {
            let path = item?.path();
            if path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|v| v.to_string_lossy().starts_with("backup-"))
            {
                items.push(match manifest(&path, &ledger) {
                    Ok(m) => json!({"path":path,"valid":true,"revision":m.revision,
                        "version":m.version,"createdAt":m.created_at,"files":m.entries.len()}),
                    Err(_) => json!({"path":path,"valid":false}),
                });
            }
        }
    }
    Ok(json!({"items":items}))
}

pub fn restore(
    ledger: &Path,
    source: &Path,
    dry_run: bool,
    expected: Option<u64>,
) -> Result<Value> {
    let ledger = absolute(ledger)?;
    let source = absolute(source)?;
    let m = manifest(&source, &ledger)?;
    let current = crate::read_state(&ledger)?;
    if let Some(expected) = expected
        && current.billing.revision != expected
    {
        bail!("BACKUP_CHANGED: ledger revision changed; inspect newer work before restoring")
    }
    if dry_run {
        let saved = crate::parse_state(std::str::from_utf8(&fs::read(
            source.join("files/ledger/file"),
        )?)?)?;
        let destinations = roots(&ledger, &saved.projects, false)?;
        let affected: Vec<_> = destinations
            .iter()
            .filter(|(slot, path)| m.present_slots.contains(*slot) || path.exists())
            .map(|(slot, path)| {
                json!({"slot":slot,"path":path,
                "action":if m.present_slots.contains(slot) { "replace" } else { "remove" }})
            })
            .collect();
        return Ok(
            json!({"dryRun":true,"path":source,"files":m.entries.len(),"currentRevision":current.billing.revision,"affected":affected}),
        );
    }
    // Preserve the state that would otherwise be overwritten, even for an explicit restore.
    // A deleted ledger has no prior state to snapshot; restore remains possible.
    let recovery = if ledger.exists() {
        Some(
            create_snapshot(&ledger, true)
                .context("BACKUP_RECOVERY: current state could not be saved")?,
        )
    } else {
        None
    };
    let _held = locks(&ledger)?;
    if crate::read_state(&ledger)?.billing.revision != current.billing.revision {
        bail!("BACKUP_CHANGED: ledger changed while creating recovery snapshot")
    }
    let saved = crate::parse_state(std::str::from_utf8(&fs::read(
        source.join("files/ledger/file"),
    )?)?)?;
    let slots = roots(&ledger, &saved.projects, false)?;
    // Stage permissions and paths before the first replacement; never follow a
    // symlink in an ancestor of a target (including a replaced template folder).
    for entry in &m.entries {
        let root = &slots[&entry.slot];
        let target = if entry.relative == "file" {
            root.clone()
        } else {
            root.join(safe_relative(&entry.relative)?)
        };
        for ancestor in target.ancestors() {
            if fs::symlink_metadata(ancestor).is_ok_and(|s| s.file_type().is_symlink()) {
                bail!(
                    "BACKUP_SCOPE: refusing symlink ancestor {}",
                    ancestor.display()
                )
            }
        }
    }
    // Restoring a directory means replacing its whole managed contents. This
    // also removes a new skill introduced by a failed upgrade. Plugin Git
    // metadata, user files and its binary link are deliberately kept intact.
    for (slot, root) in &slots {
        if matches!(
            slot.as_str(),
            "ledger" | "feedback" | "binary" | "pre-invoices" | "pre-task-rates"
        ) || slot.starts_with("logo-")
        {
            if !m.present_slots.contains(slot) && root.is_file() {
                fs::remove_file(root)?;
            }
            continue;
        }
        if slot == "plugin" {
            if root.is_dir() {
                for item in fs::read_dir(root)? {
                    let item = item?;
                    let name = item.file_name().to_string_lossy().into_owned();
                    if name == "manifest.json"
                        || name == "templates"
                        || name == "sounds"
                        || name.ends_with(".qml")
                        || name.ends_with(".js")
                    {
                        if item.file_type()?.is_symlink() {
                            bail!("BACKUP_SCOPE: plugin runtime is a symlink")
                        }
                        if item.file_type()?.is_dir() {
                            fs::remove_dir_all(item.path())?;
                        } else {
                            fs::remove_file(item.path())?;
                        }
                    }
                }
            }
        } else if root.exists() {
            if slot.starts_with("skill-") && !root.join(".omatracker-install.json").is_file() {
                bail!(
                    "BACKUP_SCOPE: refusing to remove an unmanaged skill {}",
                    root.display()
                )
            }
            if fs::symlink_metadata(root)?.file_type().is_symlink() {
                bail!("BACKUP_SCOPE: directory is a symlink")
            }
            fs::remove_dir_all(root)?;
        }
        if m.present_slots.contains(slot) {
            fs::create_dir_all(root)?;
        }
    }
    for entry in &m.entries {
        let target_root = &slots[&entry.slot];
        let target = if entry.relative == "file" {
            target_root.clone()
        } else {
            target_root.join(safe_relative(&entry.relative)?)
        };
        if fs::symlink_metadata(&target).is_ok_and(|s| s.file_type().is_symlink()) {
            bail!(
                "BACKUP_SCOPE: refusing to overwrite symlink {}",
                target.display()
            )
        }
        fs::create_dir_all(target.parent().unwrap())?;
        crate::atomic_write(
            &target,
            &fs::read(source.join("files").join(&entry.slot).join(&entry.relative))?,
        )?;
    }
    Ok(
        json!({"restored":source,"recovery":recovery.as_ref().map(|r| &r["path"]),"files":m.entries.len()}),
    )
}

pub fn run(path: &Path, cmd: BackupCommand) -> Result<()> {
    let value = match cmd {
        BackupCommand::Create => create(path)?,
        BackupCommand::List => list(path)?,
        BackupCommand::Verify { path: snapshot } => verify(path, &snapshot)?,
        BackupCommand::Restore {
            path: snapshot,
            dry_run,
            expect_revision,
        } => restore(path, &snapshot, dry_run, expect_revision)?,
    };
    println!("{value}");
    Ok(())
}
