//! User-owned Typst sources and self-contained report render bundles.
use crate::{cache_path, home_dir, template_path, typst_root_path, typst_string};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub builtin: bool,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

pub(crate) fn valid_id(id: &str) -> bool {
    matches!(id, "detailed" | "summary" | "invoice")
        || id.strip_prefix("user:").is_some_and(valid_name)
}

pub fn directory() -> Result<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or(home_dir()?.join(".config"));
    Ok(config.join("omarchy/omatracker/templates"))
}

pub(crate) fn user_path(id: &str) -> Result<PathBuf> {
    let name = id
        .strip_prefix("user:")
        .filter(|name| valid_name(name))
        .context("invalid custom template ID; use user:<name> with letters, digits, - or _")?;
    let root = directory()?;
    let folder = root.join(name);
    let path = folder.join("template.typ");
    // Disallow symlinked template roots and entrypoints as well as nested links.
    for item in [&folder, &path] {
        let metadata = fs::symlink_metadata(item)
            .with_context(|| format!("template {id} is missing at {}", item.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("template symlinks are not supported: {}", item.display())
        }
    }
    if !path.is_file() {
        bail!("template {} is not a file", path.display())
    }
    Ok(path)
}

pub fn path(id: &str) -> Result<PathBuf> {
    template_path(id)?
        .canonicalize()
        .context("could not resolve template path")
}

pub fn list() -> Result<Vec<TemplateInfo>> {
    let mut items = Vec::new();
    for (id, name) in [
        ("detailed", "Detailed"),
        ("summary", "Summary"),
        ("invoice", "Invoice"),
    ] {
        // Keep the catalog usable even when an override is temporarily missing.
        items.push(TemplateInfo {
            id: id.into(),
            name: name.into(),
            path: template_path(id)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            builtin: true,
        });
    }
    let root = directory()?;
    if root.exists() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if valid_name(&name) && entry.file_type()?.is_dir() {
                let id = format!("user:{name}");
                if let Ok(path) = user_path(&id) {
                    items.push(TemplateInfo {
                        id,
                        name,
                        path: path.display().to_string(),
                        builtin: false,
                    });
                }
            }
        }
    }
    items[3..].sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
            bail!(
                "template assets must be regular files or directories: {}",
                entry.path().display()
            )
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_source(id: &str, destination: &Path) -> Result<()> {
    let entry = template_path(id)?;
    let source = entry
        .parent()
        .context("template has no parent")?
        .canonicalize()?;
    fs::create_dir_all(destination)?;
    if destination.canonicalize()?.starts_with(&source) {
        bail!("template source directory cannot contain its destination")
    }
    copy_tree(&source, destination)?;
    if entry.file_name().is_some_and(|name| name != "template.typ") {
        // Put the actual editable layout in the entrypoint. Relative imports
        // still resolve beside it, exactly as in the original source directory.
        fs::copy(&entry, destination.join("template.typ"))?;
    }
    Ok(())
}

pub fn create(name: &str, from: &str) -> Result<TemplateInfo> {
    if !valid_name(name) {
        bail!(
            "template name must start with a letter or digit and contain only letters, digits, - or _ (max 80)"
        )
    }
    let root = directory()?;
    fs::create_dir_all(&root)?;
    // Share a lock with other creators: rename must never overwrite a user copy.
    let lock = crate::open_lock_file(&root.join(".create"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let destination = root.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        bail!("template {name} already exists")
    }
    let temporary = tempfile::tempdir_in(&root)?;
    copy_source(from, temporary.path())?;
    fs::rename(temporary.path(), &destination)?;
    Ok(TemplateInfo {
        id: format!("user:{name}"),
        name: name.into(),
        path: destination.join("template.typ").display().to_string(),
        builtin: false,
    })
}

fn hash_tree(root: &Path, path: &Path, hash: &mut Sha256) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if entry.file_type()?.is_dir() {
            hash_tree(root, &entry.path(), hash)?;
        } else {
            let bytes = fs::read(entry.path())?;
            hash.update(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .as_bytes(),
            );
            hash.update([0]);
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    Ok(())
}

/// Capture before publishing a report to the ledger. A failed capture is never visible.
pub(crate) fn capture(id: &str, data: &Value, destination: &Path) -> Result<()> {
    let parent = destination.parent().context("bundle has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::tempdir_in(parent)?;
    let root = temporary.path();
    copy_source(id, &root.join("template"))?;
    let mut data = data.clone();
    if let Some(logo) = data["project"]["logoPath"]
        .as_str()
        .filter(|p| !p.is_empty())
    {
        let source = PathBuf::from(logo);
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("png");
        if !matches!(
            extension.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "svg" | "gif"
        ) {
            bail!("logo must be a PNG, JPEG, SVG, or GIF image")
        }
        let name = format!("project-logo.{extension}");
        fs::copy(&source, root.join(&name))
            .with_context(|| format!("could not capture logo {}", source.display()))?;
        data["project"]["logoPath"] = json!(format!("/{name}"));
    }
    fs::write(root.join("data.json"), serde_json::to_vec_pretty(&data)?)?;
    fs::write(
        root.join("report.typ"),
        format!(
            "#import {}: render\n#render(json(\"data.json\"))\n",
            typst_string(&typst_root_path(&root.join("template/template.typ"), root))
        ),
    )?;
    let mut hash = Sha256::new();
    hash_tree(root, root, &mut hash)?;
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "templateId": id, "sha256": format!("{:x}", hash.finalize()), "version": 1
        }))?,
    )?;
    if destination.exists() {
        bail!("report bundle already exists: {}", destination.display())
    }
    fs::rename(root, destination)?;
    Ok(())
}

pub(crate) fn compile(bundle: &Path, pdf: &Path) -> Result<()> {
    let output = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(bundle)
        .arg(bundle.join("report.typ"))
        .arg(pdf)
        .output()
        .context("could not start Typst; install typst to generate PDFs")?;
    if !output.status.success() {
        bail!(
            "Typst compilation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    }
    if !pdf.is_file() {
        bail!("Typst completed without creating {}", pdf.display())
    }
    Ok(())
}

pub fn validate(id: &str) -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let bundle = temporary.path().join("bundle");
    let mut data: Value = serde_json::from_str(include_str!("../tests/report-snapshot.json"))?;
    let invoice: Value = serde_json::from_str(include_str!("../tests/invoice-snapshot.json"))?;
    for (key, value) in invoice.as_object().unwrap() {
        data[key] = value.clone();
    }
    capture(id, &data, &bundle)?;
    compile(&bundle, &temporary.path().join("preview.pdf"))
}

pub fn preview(ledger: &Path, id: &str, project_id: Option<&str>) -> Result<PathBuf> {
    let state = crate::read_state(ledger)?;
    let project = state
        .projects
        .iter()
        .find(|project| project.id == project_id.unwrap_or(&state.active_project_id))
        .context("project does not exist")?;
    let bounds = crate::last_completed_period("weekly", crate::now_ms())?;
    let mut data = serde_json::to_value(crate::build_snapshot(
        &state,
        project,
        "weekly",
        bounds.start_at,
        bounds.end_at,
    ))?;
    let config = crate::billing::settings(&state, &project.id)?;
    let (from, to) = crate::billing::previous_period("monthly", &config.timezone)?;
    let currency = project
        .rate
        .as_ref()
        .map(crate::HourlyRate::currency)
        .unwrap_or("USD");
    let invoice = crate::billing::draft(&state, &project.id, &from, &to, currency)?;
    let invoice_data = crate::billing::template_data(&invoice)?;
    for key in ["invoice", "issuer", "client", "lines"] {
        data[key] = invoice_data[key].clone();
    }
    let root = cache_path()?.join("previews");
    fs::create_dir_all(&root)?;
    let temporary = tempfile::tempdir_in(root)?;
    let bundle = temporary.path().join("bundle");
    capture(id, &data, &bundle)?;
    let pdf = temporary.path().join("preview.pdf");
    compile(&bundle, &pdf)?;
    let _ = temporary.keep();
    Ok(pdf)
}
