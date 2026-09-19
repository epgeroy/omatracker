//! Desktop dispatch for explicit local PDFs; launching is not visual inspection.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// Match Service.qml::openTemplateFile: encodeURIComponent on each path segment.
fn file_url(path: &str) -> String {
    let mut url = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.!~*'()".contains(&byte) {
            url.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(url, "%{byte:02X}").unwrap();
        }
    }
    url
}

pub fn open(source: &str) -> Result<Value> {
    let path = Path::new(source).canonicalize().context(
        "INVALID_INPUT: PDF path does not exist; supply an existing local path, not a URL",
    )?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        bail!("INVALID_INPUT: artifact.open accepts only a local PDF file")
    }
    let mut header = [0; 5];
    File::open(&path)?
        .read_exact(&mut header)
        .context("INVALID_INPUT: could not read PDF header")?;
    if &header != b"%PDF-" {
        bail!("INVALID_INPUT: file does not have a PDF header")
    }
    let path_text = path
        .to_str()
        .context("INVALID_INPUT: PDF path must be UTF-8")?;
    let mut result = json!({"path":path_text,"status":"launch_failed","launchRequested":false,
        "visiblyOpened":null});
    #[cfg(target_os = "linux")]
    if ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .all(|key| std::env::var_os(key).is_none_or(|value| value.is_empty()))
    {
        result["message"] =
            json!("No desktop session (DISPLAY/WAYLAND_DISPLAY); open the PDF path on a desktop.");
        return Ok(result);
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // A file, not a pipe: neither a viewer nor its descendants can hold CLI output
    // pipes open. Keep immediate diagnostics, with no shell interpolation.
    let mut diagnostics = tempfile::tempfile()?;
    let mut command = Command::new(opener);
    command
        .arg(file_url(path_text))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(diagnostics.try_clone()?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            result["message"] = json!(format!(
                "Could not start {opener}: {error}; open the PDF path manually."
            ));
            return Ok(result);
        }
    };
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                diagnostics.rewind()?;
                let mut detail = String::new();
                diagnostics.take(4096).read_to_string(&mut detail).ok();
                result["message"] = json!(format!(
                    "{opener} exited with {status}: {}. Check the PDF file association and desktop session.",
                    detail.trim()
                ));
                result["exitCode"] = json!(status.code());
                return Ok(result);
            }
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                // Reap in long-lived library hosts too; this thread never delays CLI exit.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                break;
            }
            Err(error) => {
                result["message"] = json!(format!("Could not observe {opener} launch: {error}"));
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(result);
            }
        }
    }
    result["status"] = json!("launch_requested");
    result["launchRequested"] = json!(true);
    result["message"] = json!(
        "Launch requested; document visibility and later viewer failures are not observable."
    );
    Ok(result)
}
