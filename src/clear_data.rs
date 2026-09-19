//! Explicit destructive reset, separate from normal archival entity operations.
//! Every remote deletion is an individual, backed-up, identified tracker file.
use crate::{DEFAULT_PROJECT_ID, State};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Subcommand)]
pub enum DataCommand {
    /// Clear all user records and generated documents, keeping an empty system workspace.
    Clear {
        /// List counts and exact paths without deleting anything or creating backups.
        #[arg(long)]
        dry_run: bool,
        /// Also back up and delete identified invoice/report files and the ledger snapshot on Drive.
        #[arg(long)]
        include_drive: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteFile {
    path: String,
    kind: String,
    metadata: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    ledger: PathBuf,
    records: Value,
    local_paths: Vec<PathBuf>,
    remote_files: Vec<RemoteFile>,
    missing_remote_files: Vec<String>,
    include_drive: bool,
}

fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => {
                bail!("LOCAL_SCOPE_ERROR: use a normalized path without .. for clear-all")
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn remote_valid(path: &str) -> Result<()> {
    let (remote, location) = path
        .split_once(':')
        .context("INVALID_REMOTE_PATH: expected a named rclone remote")?;
    if !crate::valid_remote(remote)
        || remote.starts_with('-')
        || location.is_empty()
        || location
            .chars()
            .any(|c| c.is_control() || c == ':' || c == '\\')
        || location.split('/').any(|part| part == "." || part == "..")
    {
        bail!("INVALID_REMOTE_PATH: cannot clear {path}")
    }
    Ok(())
}

fn remote_stat(path: &str) -> Result<Option<Value>> {
    let output = Command::new("rclone")
        .args(["lsjson", "--stat", "--", path])
        .output()
        .context("DEPENDENCY_MISSING: install rclone to clear Drive files")?;
    if matches!(output.status.code(), Some(3 | 4)) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!(
            "DRIVE_CHECK_FAILED: {path}: {}",
            crate::output_summary(&output.stdout, &output.stderr)
        )
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).context("INVALID_REMOTE_RESPONSE: rclone lsjson")?;
    if value["IsDir"] != false {
        bail!("REMOTE_SCOPE_ERROR: expected a file, not a directory: {path}")
    }
    Ok(Some(value))
}

fn identities(value: &Value) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for key in ["projects", "tasks", "entries"] {
        if let Some(items) = value[key].as_array() {
            for item in items {
                for field in ["id", "projectId", "taskId"] {
                    if let Some(id) = item[field]
                        .as_str()
                        .filter(|id| *id != DEFAULT_PROJECT_ID && !id.is_empty())
                    {
                        ids.insert(id.into());
                    }
                }
            }
        }
    }
    if let Some(clients) = value["billing"]["clients"].as_object() {
        ids.extend(clients.keys().cloned());
    }
    if let Some(invoices) = value["billing"]["invoices"].as_array() {
        ids.extend(
            invoices
                .iter()
                .filter_map(|i| i["id"].as_str())
                .map(str::to_owned),
        );
    }
    ids
}

fn check_remote_ledger(path: &str, state: &State) -> Result<()> {
    let output = Command::new("rclone").args(["cat", "--", path]).output()?;
    if !output.status.success() {
        bail!("DRIVE_CHECK_FAILED: cannot inspect {path}")
    }
    let remote: Value = serde_json::from_slice(&output.stdout)
        .context("REMOTE_SCOPE_ERROR: remote ledger is not JSON")?;
    let ours = identities(&serde_json::to_value(state)?);
    let theirs = identities(&remote);
    if ours.is_disjoint(&theirs) {
        // An empty tracker can still have its own synced state.json. Require an
        // exact data/configuration match, ignoring only sync bookkeeping.
        let mut remote_state = crate::parse_state(std::str::from_utf8(&output.stdout)?)?;
        let mut local_state = state.clone();
        remote_state.sync = Default::default();
        local_state.sync = Default::default();
        remote_state.billing.revision = 0;
        local_state.billing.revision = 0;
        if serde_json::to_value(remote_state)? != serde_json::to_value(local_state)? {
            bail!(
                "REMOTE_SCOPE_ERROR: cannot establish that {path} belongs to this ledger; no files were deleted"
            )
        }
    }
    Ok(())
}

fn invoice_remote(
    state: &State,
    invoice: &Value,
    targets: &mut BTreeMap<String, String>,
) -> Result<()> {
    let number = invoice["number"].as_str().unwrap_or("");
    if number.is_empty() {
        return Ok(());
    }
    let project = invoice["projectId"]
        .as_str()
        .context("INVALID_INVOICE: missing projectId")?;
    if !component(project) || !component(number) {
        bail!("REMOTE_SCOPE_ERROR: invalid invoice identifiers")
    }
    let suffix = format!("invoices/{project}/{number}.pdf");
    let recorded = invoice["remotePath"].as_str().unwrap_or("");
    let path = if recorded.is_empty() {
        let mut drive = state.drive.clone();
        if let Some(settings) = state
            .billing
            .projects
            .get(project)
            .filter(|p| !p.drive_folder.is_empty())
        {
            drive.folder = settings.drive_folder.clone();
        }
        crate::remote_path(&drive, &suffix)?
    } else {
        if !recorded.ends_with(&format!("/{suffix}")) {
            bail!("REMOTE_SCOPE_ERROR: unexpected invoice destination {recorded}")
        }
        recorded.into()
    };
    remote_valid(&path)?;
    targets.insert(path, "invoice".into());
    Ok(())
}

fn no_links(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        bail!(
            "LOCAL_SCOPE_ERROR: refusing symlink or special file {}",
            path.display()
        )
    }
    if metadata.is_dir() {
        for item in fs::read_dir(path)? {
            no_links(&item?.path())?;
        }
    }
    Ok(())
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn plan(path: &Path, state: &State, include_drive: bool) -> Result<Plan> {
    let ledger = absolute(path)?;
    let invoice_root = crate::billing::artifact_root(&ledger)?;
    let mut local = BTreeSet::new();
    let mut targets = BTreeMap::new();
    if include_drive && !crate::valid_remote(&state.drive.remote) {
        bail!(
            "DRIVE_NOT_CONFIGURED: configure Drive before using --include-drive, or clear locally without that option"
        )
    }
    if exists(&invoice_root)? {
        no_links(&invoice_root)?;
        local.insert(invoice_root.clone());
        if include_drive {
            // Captured issuance snapshots also identify orphaned PDFs whose ledger records were lost.
            for item in fs::read_dir(&invoice_root)? {
                let item = item?;
                if !item.file_type()?.is_dir()
                    || !item.file_name().to_string_lossy().starts_with("invoice-")
                {
                    continue;
                }
                for bundle in fs::read_dir(item.path())? {
                    let bundle = bundle?;
                    if !bundle.file_type()?.is_dir()
                        || !bundle.file_name().to_string_lossy().starts_with("bundle-")
                    {
                        continue;
                    }
                    let data_path = bundle.path().join("data.json");
                    if !data_path.is_file() {
                        continue;
                    }
                    let data: Value = serde_json::from_slice(&fs::read(&data_path)?)?;
                    if data["invoice"]["id"].as_str() != item.file_name().to_str() {
                        bail!("LOCAL_SCOPE_ERROR: invoice snapshot ID does not match its directory")
                    }
                    // Prefer the ledger's authoritative pinned destination if it still has the invoice.
                    if !state
                        .billing
                        .invoices
                        .iter()
                        .any(|i| Some(i.id.as_str()) == data["invoice"]["id"].as_str())
                    {
                        invoice_remote(state, &data["invoice"], &mut targets)?;
                    }
                }
            }
        }
    }
    let cache = absolute(&crate::cache_path()?)?;
    for report in &state.reports {
        let prefix = crate::safe_key(&report.key);
        if prefix.is_empty() {
            bail!("LOCAL_SCOPE_ERROR: empty report key")
        }
        let legacy_bundle = if report.template_bundle.is_empty() && !report.typ_path.is_empty() {
            PathBuf::from(&report.typ_path)
                .with_extension("bundle")
                .display()
                .to_string()
        } else {
            String::new()
        };
        for file in [
            &report.data_path,
            &report.typ_path,
            &report.pdf_path,
            &report.template_bundle,
            &legacy_bundle,
        ] {
            if file.is_empty() {
                continue;
            }
            let file = absolute(Path::new(file))?;
            let relative = file
                .strip_prefix(&cache)
                .context("LOCAL_SCOPE_ERROR: report file is outside the report cache")?;
            let first = relative
                .components()
                .next()
                .context("LOCAL_SCOPE_ERROR: report path points at the cache root")?;
            let name = first.as_os_str().to_string_lossy();
            let regular = ["pdf", "json", "typ"]
                .iter()
                .any(|ext| name == format!("{prefix}.{ext}"));
            let bundle = name == format!("{prefix}.bundle")
                || name
                    .strip_prefix(&format!("{prefix}-"))
                    .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok());
            if !regular && !bundle {
                bail!("LOCAL_SCOPE_ERROR: report path does not match its generated filename")
            }
            let artifact = cache.join(first.as_os_str());
            if exists(&artifact)? {
                no_links(&artifact)?;
                local.insert(artifact);
            }
        }
        // Legacy report paths use human-readable names, not unique project IDs.
        // Never guess a destination for a report with no recorded upload target.
        if include_drive && !report.remote_path.is_empty() {
            let expected = format!(
                "/reports/{}/{}/",
                crate::slug(&report.project_name),
                report.period
            );
            let date = report
                .remote_path
                .rsplit_once(&expected)
                .and_then(|(_, name)| name.strip_suffix(".pdf"));
            if date.is_none_or(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
            {
                bail!("REMOTE_SCOPE_ERROR: unexpected report destination")
            }
            let target = report.remote_path.clone();
            remote_valid(&target)?;
            targets.insert(target, "report".into());
        }
    }
    if include_drive {
        for invoice in &state.billing.invoices {
            invoice_remote(state, &serde_json::to_value(invoice)?, &mut targets)?;
        }
        let snapshot = crate::remote_path(&state.drive, "state.json")?;
        remote_valid(&snapshot)?;
        targets.insert(snapshot, "ledger".into());
    }
    let mut remote_files = Vec::new();
    let mut missing_remote_files = Vec::new();
    for (target, kind) in targets {
        if let Some(metadata) = remote_stat(&target)? {
            if kind == "ledger" {
                check_remote_ledger(&target, state)?;
            }
            remote_files.push(RemoteFile {
                path: target,
                kind,
                metadata,
            });
        } else {
            missing_remote_files.push(target);
        }
    }
    Ok(Plan {
        ledger,
        include_drive,
        local_paths: local.into_iter().collect(),
        remote_files,
        missing_remote_files,
        records: json!({"userProjects":state.projects.iter().filter(|p|p.id != DEFAULT_PROJECT_ID).count(),
            "clients":state.billing.clients.len(),"tasks":state.tasks.len(),"entries":state.entries.len(),
            "invoices":state.billing.invoices.len(),"reports":state.reports.len(),
            "archivedProjects":state.billing.archived_projects.len(),"archivedClients":state.billing.archived_clients.len()}),
    })
}

fn copy_local(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        fs::create_dir_all(destination)?;
        for file in fs::read_dir(source)? {
            let file = file?;
            copy_local(&file.path(), &destination.join(file.file_name()))?;
        }
    } else if metadata.is_file() {
        fs::copy(source, destination)?;
        fs::File::open(destination)?.sync_all()?;
    } else {
        bail!("LOCAL_SCOPE_ERROR: cannot back up symlink or special file")
    }
    Ok(())
}

fn remote_unchanged(file: &RemoteFile) -> Result<()> {
    let current =
        remote_stat(&file.path)?.context("REMOTE_CHANGED: file disappeared after backup")?;
    for key in ["ID", "Size", "ModTime"] {
        if current[key] != file.metadata[key] {
            bail!(
                "REMOTE_CHANGED: {} changed after inventory; local data was not cleared",
                file.path
            )
        }
    }
    Ok(())
}

pub fn clear(path: &Path, dry_run: bool, include_drive: bool) -> Result<Value> {
    absolute(path)?;
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("LOCAL_SCOPE_ERROR: choose the real ledger path, not a symlink")
    }
    // Match external_receipt -> invoice worker -> ledger ordering; also exclude old
    // report workers and ledger snapshot uploads from recreating deleted remote files.
    let mut workers = Vec::new();
    for suffix in [
        "agent-external",
        "invoices-worker",
        "reports",
        "sync-worker",
    ] {
        workers.push(crate::lock_file(&PathBuf::from(format!(
            "{}.{}",
            path.display(),
            suffix
        )))?);
    }
    let _ledger = crate::lock_file(path)?;
    let state = crate::read_state(path)?;
    let next_revision = state
        .billing
        .revision
        .checked_add(1)
        .context("revision overflow")?;
    crate::feedback::preferences(path)?;
    let plan = plan(path, &state, include_drive)?;
    let system = json!({"id":DEFAULT_PROJECT_ID,"name":"Unassigned","protected":true,
        "reason":"Internal fallback workspace; not a user project or a permission restriction"});
    if dry_run {
        return Ok(
            json!({"schemaVersion":1,"ok":true,"dryRun":true,"plan":plan,"systemWorkspace":system}),
        );
    }

    let backup =
        PathBuf::from(format!("{}.backups", plan.ledger.display())).join(crate::make_id("clear"));
    fs::create_dir_all(backup.join("local"))?;
    fs::create_dir_all(backup.join("remote"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&backup, fs::Permissions::from_mode(0o700))?;
    }
    let result = (|| -> Result<Value> {
        let original = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                serde_json::to_vec_pretty(&state)?
            }
            Err(e) => return Err(e.into()),
        };
        crate::atomic_write(&backup.join("ledger.json"), &original)?;
        let feedback = PathBuf::from(format!("{}.feedback.json", path.display()));
        if feedback.is_file() {
            copy_local(&feedback, &backup.join("feedback.json"))?;
        }
        let mut journal = json!({"createdAt":crate::now_ms(),"plan":plan,"localStateCleared":false,"deletedRemoteFiles":[],"removedLocalPaths":[]});
        let save = |value: &Value| {
            crate::atomic_write(
                &backup.join("manifest.json"),
                &serde_json::to_vec_pretty(value)?,
            )
        };
        save(&journal)?;
        for (index, source) in plan.local_paths.iter().enumerate() {
            copy_local(source, &backup.join("local").join(index.to_string()))?;
        }
        // Download every remote target before any deletion begins.
        for (index, file) in plan.remote_files.iter().enumerate() {
            let destination = backup.join("remote").join(index.to_string());
            crate::run_command(
                "rclone",
                [
                    "copyto".into(),
                    "--".into(),
                    file.path.clone(),
                    destination.display().to_string(),
                ],
            )?;
            if !destination.is_file() {
                bail!("REMOTE_BACKUP_FAILED: backup was not created")
            }
            if let Some(size) = file.metadata["Size"].as_u64()
                && fs::metadata(&destination)?.len() != size
            {
                bail!("REMOTE_BACKUP_FAILED: backup size mismatch")
            }
            fs::File::open(destination)?.sync_all()?;
        }
        for file in &plan.remote_files {
            remote_unchanged(file)?;
        }
        for file in &plan.remote_files {
            remote_unchanged(file)?;
            crate::run_command(
                "rclone",
                ["deletefile".into(), "--".into(), file.path.clone()],
            )?;
            if remote_stat(&file.path)?.is_some() {
                bail!(
                    "REMOTE_DELETE_FAILED: {} still exists; local data was not cleared",
                    file.path
                )
            }
            journal["deletedRemoteFiles"]
                .as_array_mut()
                .unwrap()
                .push(json!(file.path));
            save(&journal)?;
        }
        // Configuration survives; user records, histories, drafts, and receipts do not.
        let mut fresh = State {
            drive: state.drive.clone(),
            ..Default::default()
        };
        fresh.billing.issuer = state.billing.issuer.clone();
        fresh.billing.sequences = state.billing.sequences.clone(); // Never reuse issued invoice numbers.
        fresh.billing.revision = next_revision;
        crate::billing::initialize(&mut fresh);
        crate::atomic_write(path, &serde_json::to_vec_pretty(&fresh)?)?;
        journal["localStateCleared"] = json!(true);
        save(&journal)?;
        crate::feedback::clear_history(path)?;
        for source in &plan.local_paths {
            if source.is_dir() {
                fs::remove_dir_all(source)?;
            } else {
                fs::remove_file(source)?;
            }
            journal["removedLocalPaths"]
                .as_array_mut()
                .unwrap()
                .push(json!(source));
            save(&journal)?;
        }
        journal["complete"] = json!(true);
        save(&journal)?;
        Ok(
            json!({"schemaVersion":1,"ok":true,"dryRun":false,"backupPath":backup,
            "cleared":plan.records,"deletedRemoteFiles":journal["deletedRemoteFiles"],"missingRemoteFiles":plan.missing_remote_files,"removedLocalPaths":plan.local_paths,
            "remaining":{"userProjects":0,"clients":0,"tasks":0,"entries":0,"invoices":0,"reports":0},
            "systemWorkspace":system,"preserved":["issuer profile","invoice numbering","Drive configuration","templates","preferences","backups"]}),
        )
    })();
    result.with_context(|| {
        format!(
            "CLEAR_FAILED: operation may be partially complete; backup and progress are at {}",
            backup.display()
        )
    })
}

pub fn run(path: &Path, command: DataCommand) -> Result<()> {
    let DataCommand::Clear {
        dry_run,
        include_drive,
        json: as_json,
    } = command;
    let value = match clear(path, dry_run, include_drive) {
        Ok(value) => value,
        Err(error) if as_json => {
            println!("{}", crate::agent::error(&error));
            std::process::exit(1);
        }
        Err(error) => return Err(error),
    };
    if as_json {
        println!("{value}");
    } else if dry_run {
        println!("Clear-all preview: {}", value["plan"]["records"]);
        println!("Local generated paths: {}", value["plan"]["localPaths"]);
        println!("Drive files: {}", value["plan"]["remoteFiles"]);
        println!(
            "Remote candidates already missing: {}",
            value["plan"]["missingRemoteFiles"]
        );
        println!("No data cleared. An empty internal Unassigned workspace will remain.");
    } else {
        println!(
            "Cleared all user projects, clients, tasks, time entries, and invoice/report records."
        );
        println!("Remaining: 0 user projects; 1 empty internal Unassigned workspace.");
        println!("Backup: {}", value["backupPath"].as_str().unwrap());
        println!(
            "Drive files removed: {}",
            value["deletedRemoteFiles"].as_array().unwrap().len()
        );
    }
    Ok(())
}
