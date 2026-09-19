use anyhow::{Context, Result, bail};
use chrono::{Datelike, Days, Local, NaiveDate, TimeZone, Timelike};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use uuid::Uuid;

pub const STATE_VERSION: u32 = 2;
pub const DEFAULT_PROJECT_ID: &str = "project-unassigned";

fn default_drive_folder() -> String {
    "OmaTracker".to_owned()
}

fn default_sync_status() -> String {
    "Not synced yet".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub template_id: String,
    pub client_name: String,
    pub company_name: String,
    pub logo_path: String,
    pub accent_color: String,
    pub paper: String,
    pub export_weekly: bool,
    pub export_monthly: bool,
}

impl Default for Project {
    fn default() -> Self {
        default_project()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub legacy_seconds: i64,
    pub running: bool,
    pub started_at: i64,
    pub display_since: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    pub id: String,
    pub project_id: String,
    pub task_id: String,
    pub task_title: String,
    pub note: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub seconds: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub key: String,
    pub project_id: String,
    pub project_name: String,
    pub period: String,
    pub start_at: i64,
    pub end_at: i64,
    pub template_id: String,
    pub status: String,
    pub data_path: String,
    pub typ_path: String,
    pub pdf_path: String,
    pub remote_path: String,
    pub created_at: i64,
    pub completed_at: i64,
    pub attempts: i64,
    pub last_error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Drive {
    pub remote: String,
    pub folder: String,
    pub sync_on_startup: bool,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            remote: String::new(),
            folder: default_drive_folder(),
            sync_on_startup: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncState {
    pub status: String,
    pub error: String,
    pub last_synced_at: i64,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            status: default_sync_status(),
            error: String::new(),
            last_synced_at: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub version: u32,
    pub active_project_id: String,
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub entries: Vec<Entry>,
    pub reports: Vec<Report>,
    pub drive: Drive,
    pub sync: SyncState,
}

impl Default for State {
    fn default() -> Self {
        let project = default_project();
        Self {
            version: STATE_VERSION,
            active_project_id: project.id.clone(),
            projects: vec![project],
            tasks: Vec::new(),
            entries: Vec::new(),
            reports: Vec::new(),
            drive: Drive::default(),
            sync: SyncState::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    #[serde(flatten)]
    pub task: Task,
    pub display_seconds: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub typst_available: bool,
    pub rclone_available: bool,
    pub setup_status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    pub now_ms: i64,
    pub active_project: Option<Project>,
    pub active_tasks: Vec<TaskView>,
    pub total_tracked_seconds: i64,
    pub active_project_seconds: i64,
    pub running_timers: usize,
    pub setup_status: String,
    pub report_status: String,
    pub sync_status: String,
    pub sync_error: String,
    pub background_checks_enabled: bool,
    pub dependencies: DependencyStatus,
}

#[derive(Clone, Debug)]
pub struct ProjectChanges {
    pub name: Option<String>,
    pub client_name: Option<String>,
    pub company_name: Option<String>,
    pub template_id: Option<String>,
    pub export_weekly: Option<bool>,
    pub export_monthly: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct Period {
    pub start_at: i64,
    pub end_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportEntry {
    task: String,
    note: String,
    date: String,
    started: String,
    ended: String,
    duration: String,
    seconds: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportSnapshot {
    generated_at: i64,
    project: SnapshotProject,
    period: SnapshotPeriod,
    total_seconds: i64,
    total_duration: String,
    entries: Vec<ReportEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProject {
    id: String,
    name: String,
    client_name: String,
    company_name: String,
    logo_path: String,
    accent_color: String,
    paper: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotPeriod {
    kind: String,
    label: String,
    start: String,
    end: String,
    start_at: i64,
    end_at: i64,
}

fn default_project() -> Project {
    Project {
        id: DEFAULT_PROJECT_ID.to_owned(),
        name: "Unassigned".to_owned(),
        template_id: "detailed".to_owned(),
        client_name: String::new(),
        company_name: String::new(),
        logo_path: String::new(),
        accent_color: "#476a89".to_owned(),
        paper: "a4".to_owned(),
        export_weekly: true,
        export_monthly: true,
    }
}

pub fn default_data_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config/omarchy/omatracker.json"))
}

pub fn cache_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".cache/omarchy/omatracker"))
}

pub fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn format_duration(seconds: i64) -> String {
    let total = seconds.max(0);
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

pub fn parse_duration(value: &str) -> Result<i64> {
    let raw = value.trim();
    if raw.is_empty() {
        bail!("duration cannot be empty")
    }

    if raw.contains(':') {
        let parts: Vec<&str> = raw.split(':').collect();
        if parts.len() > 3 || parts.iter().any(|part| part.trim().is_empty()) {
            bail!("duration must be SS, MM:SS, or HH:MM:SS")
        }
        let mut seconds = 0_i64;
        for part in parts {
            let value = part
                .trim()
                .parse::<i64>()
                .context("duration contains a non-numeric segment")?;
            if value < 0 {
                bail!("duration cannot be negative")
            }
            seconds = seconds
                .checked_mul(60)
                .and_then(|total| total.checked_add(value))
                .context("duration is too large")?;
        }
        return Ok(seconds);
    }

    if raw.chars().all(|character| character.is_ascii_digit()) {
        return raw.parse::<i64>().context("duration is too large");
    }

    let mut rest = raw;
    let mut total = 0_i64;
    for suffix in ['h', 'm', 's'] {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(suffix) else {
            continue;
        };
        let number = rest[..index].trim();
        if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {
            bail!("duration must use forms such as 1h30m or 45m")
        }
        let amount = number.parse::<i64>().context("duration is too large")?;
        let multiplier = match suffix {
            'h' => 3600,
            'm' => 60,
            _ => 1,
        };
        total = total
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .context("duration is too large")?,
            )
            .context("duration is too large")?;
        rest = &rest[index + suffix.len_utf8()..];
    }
    if rest.trim().is_empty() && total > 0 {
        Ok(total)
    } else {
        bail!("duration must be SS, MM:SS, HH:MM:SS, or use h/m/s suffixes")
    }
}

pub fn sanitize_text(value: impl AsRef<str>, max_length: usize) -> String {
    let collapsed = value
        .as_ref()
        .chars()
        .map(|character| {
            if matches!(character, '\r' | '\n' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    collapsed
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_length)
        .collect()
}

fn make_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

fn normalize_project(project: &mut Project) {
    if project.id.trim().is_empty() {
        project.id = make_id("project");
    }
    project.name = sanitize_text(&project.name, 80);
    if project.name.is_empty() {
        project.name = "Untitled project".to_owned();
    }
    if project.template_id != "summary" {
        project.template_id = "detailed".to_owned();
    }
    project.client_name = sanitize_text(&project.client_name, 120);
    project.company_name = sanitize_text(&project.company_name, 120);
    project.logo_path = sanitize_text(&project.logo_path, 400);
    if !is_color(&project.accent_color) {
        project.accent_color = "#476a89".to_owned();
    }
    if project.paper != "letter" {
        project.paper = "a4".to_owned();
    }
}

fn normalize_state(state: &mut State) {
    state.version = STATE_VERSION;
    if state.projects.is_empty() {
        state.projects.push(default_project());
    }
    for project in &mut state.projects {
        normalize_project(project);
    }

    let mut project_ids: HashSet<String> = state
        .projects
        .iter()
        .map(|project| project.id.clone())
        .collect();
    if !project_ids.contains(DEFAULT_PROJECT_ID) {
        state.projects.push(default_project());
        project_ids.insert(DEFAULT_PROJECT_ID.to_owned());
    }

    for task in &mut state.tasks {
        if task.id.trim().is_empty() {
            task.id = make_id("task");
        }
        if !project_ids.contains(&task.project_id) {
            task.project_id = DEFAULT_PROJECT_ID.to_owned();
        }
        task.title = sanitize_text(&task.title, 160);
        if task.title.is_empty() {
            task.title = "Empty".to_owned();
        }
        task.legacy_seconds = task.legacy_seconds.max(0);
        task.started_at = task.started_at.max(0);
        task.display_since = task.display_since.max(0);
        if !task.running || task.started_at == 0 {
            task.running = false;
            task.started_at = 0;
        }
    }

    state.entries.retain_mut(|entry| {
        if !project_ids.contains(&entry.project_id) {
            entry.project_id = DEFAULT_PROJECT_ID.to_owned();
        }
        entry.id = if entry.id.trim().is_empty() {
            make_id("entry")
        } else {
            entry.id.clone()
        };
        entry.task_title = sanitize_text(&entry.task_title, 160);
        if entry.task_title.is_empty() {
            entry.task_title = "Manual entry".to_owned();
        }
        entry.note = sanitize_text(&entry.note, 240);
        entry.started_at = entry.started_at.max(0);
        entry.ended_at = entry.ended_at.max(0);
        entry.seconds = entry.seconds.max(0);
        if entry.started_at == 0 && entry.ended_at > 0 && entry.seconds > 0 {
            entry.started_at = entry.ended_at - entry.seconds * 1000;
        }
        if entry.seconds == 0 && entry.ended_at > entry.started_at {
            entry.seconds = (entry.ended_at - entry.started_at) / 1000;
        }
        entry.ended_at > entry.started_at && entry.seconds > 0
    });

    state.reports.retain_mut(|report| {
        if !project_ids.contains(&report.project_id) {
            report.project_id = DEFAULT_PROJECT_ID.to_owned();
        }
        report.period = if report.period == "monthly" {
            "monthly".to_owned()
        } else {
            "weekly".to_owned()
        };
        report.template_id = if report.template_id == "summary" {
            "summary".to_owned()
        } else {
            "detailed".to_owned()
        };
        if !["pending", "rendering", "uploading", "complete", "failed"]
            .contains(&report.status.as_str())
        {
            report.status = "pending".to_owned();
        }
        report.start_at = report.start_at.max(0);
        report.end_at = report.end_at.max(0);
        report.attempts = report.attempts.max(0);
        report.last_error = sanitize_text(&report.last_error, 500);
        report.project_name = sanitize_text(&report.project_name, 80);
        if report.key.trim().is_empty() {
            report.key = format!(
                "{}:{}:{}",
                report.project_id, report.period, report.start_at
            );
        }
        report.end_at > report.start_at
    });
    for report in &mut state.reports {
        if report.project_name.is_empty() {
            report.project_name = state
                .projects
                .iter()
                .find(|project| project.id == report.project_id)
                .map(|project| project.name.clone())
                .unwrap_or_else(|| "Unassigned".to_owned());
        }
    }

    if !project_ids.contains(&state.active_project_id) {
        state.active_project_id = state.projects[0].id.clone();
    }
    state.drive.remote = sanitize_text(&state.drive.remote, 80);
    state.drive.folder = sanitize_text(&state.drive.folder, 160);
    if state.drive.folder.is_empty() {
        state.drive.folder = default_drive_folder();
    }
    state.sync.status = sanitize_text(&state.sync.status, 160);
    if state.sync.status.is_empty() {
        state.sync.status = default_sync_status();
    }
    state.sync.error = sanitize_text(&state.sync.error, 500);
    state.sync.last_synced_at = state.sync.last_synced_at.max(0);
}

fn migrate_legacy(value: Value) -> State {
    let mut state = State::default();
    let tasks = match value {
        Value::Array(tasks) => tasks,
        Value::Object(object) => object
            .get("tasks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for value in tasks {
        let object = value.as_object();
        let title = object
            .and_then(|object| object.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Empty");
        let started_at = object
            .and_then(|object| object.get("startedAt"))
            .map(number_value)
            .unwrap_or(0);
        let running = object
            .and_then(|object| object.get("running"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && started_at > 0;
        state.tasks.push(Task {
            id: object
                .and_then(|object| object.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| make_id("task")),
            project_id: DEFAULT_PROJECT_ID.to_owned(),
            title: sanitize_text(title, 160),
            legacy_seconds: object
                .and_then(|object| object.get("seconds"))
                .map(number_value)
                .unwrap_or(0)
                .max(0),
            running,
            started_at: if running { started_at } else { 0 },
            display_since: 0,
        });
    }
    normalize_state(&mut state);
    state
}

fn number_value(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|value| value.floor() as i64))
        .unwrap_or(0)
}

pub fn parse_state(text: &str) -> Result<State> {
    if text.trim().is_empty() {
        return Ok(State::default());
    }
    let value: Value = serde_json::from_str(text).context("state file is not valid JSON")?;
    let version = value
        .as_object()
        .and_then(|object| object.get("version"))
        .map(number_value)
        .unwrap_or(0);
    if value.is_array() || version < STATE_VERSION as i64 {
        return Ok(migrate_legacy(value));
    }
    let mut state: State =
        serde_json::from_value(value).context("state file has an invalid schema")?;
    normalize_state(&mut state);
    Ok(state)
}

fn read_state(path: &Path) -> Result<State> {
    match fs::read_to_string(path) {
        Ok(text) => parse_state(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn write_state(path: &Path, state: &State) -> Result<()> {
    let serialized =
        serde_json::to_string_pretty(state).context("could not serialize state")? + "\n";
    atomic_write(path, serialized.as_bytes())
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().context("path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("could not create temporary file in {}", parent.display()))?;
    temporary
        .write_all(content)
        .context("could not write temporary file")?;
    temporary
        .as_file()
        .sync_all()
        .context("could not flush temporary file")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not replace {}", path.display()))?;
    Ok(())
}

fn lock_file(path: &Path) -> Result<File> {
    let parent = path.parent().context("data path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("could not open {}", lock_path.display()))?;
    lock.lock_exclusive()
        .with_context(|| format!("could not lock {}", lock_path.display()))?;
    Ok(lock)
}

fn mutate_state<T>(path: &Path, mutate: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
    let lock = lock_file(path)?;
    let mut state = read_state(path)?;
    let result = mutate(&mut state)?;
    normalize_state(&mut state);
    write_state(path, &state)?;
    lock.unlock().context("could not unlock state")?;
    Ok(result)
}

fn locked_state(path: &Path) -> Result<State> {
    let lock = lock_file(path)?;
    let state = read_state(path)?;
    lock.unlock().context("could not unlock state")?;
    Ok(state)
}

pub fn task_seconds(state: &State, task: &Task, now: i64) -> i64 {
    let since = task.display_since.max(0);
    let mut total = if since > 0 {
        0
    } else {
        task.legacy_seconds.max(0)
    };
    for entry in &state.entries {
        if entry.task_id != task.id {
            continue;
        }
        total += if since > 0 {
            overlap_seconds(entry.started_at, entry.ended_at, since, entry.ended_at)
        } else {
            entry.seconds.max(0)
        };
    }
    if task.running {
        total += ((now - task.started_at.max(since)) / 1000).max(0);
    }
    total
}

pub fn total_seconds(state: &State, now: i64, project_id: Option<&str>) -> i64 {
    state
        .tasks
        .iter()
        .filter(|task| project_id.is_none_or(|project_id| task.project_id == project_id))
        .map(|task| task_seconds(state, task, now))
        .sum()
}

pub fn overlap_seconds(start_at: i64, end_at: i64, range_start_at: i64, range_end_at: i64) -> i64 {
    let start = start_at.max(range_start_at).max(0);
    let end = end_at.min(range_end_at).max(0);
    ((end - start) / 1000).max(0)
}

fn entries_for_period(
    state: &State,
    project_id: &str,
    start_at: i64,
    end_at: i64,
    now: i64,
) -> Vec<Entry> {
    let mut result = Vec::new();
    for entry in &state.entries {
        if entry.project_id != project_id {
            continue;
        }
        let seconds = overlap_seconds(entry.started_at, entry.ended_at, start_at, end_at);
        if seconds == 0 {
            continue;
        }
        result.push(Entry {
            id: entry.id.clone(),
            task_id: entry.task_id.clone(),
            task_title: entry.task_title.clone(),
            note: entry.note.clone(),
            project_id: entry.project_id.clone(),
            started_at: entry.started_at.max(start_at),
            ended_at: entry.ended_at.min(end_at),
            seconds,
        });
    }
    let cap = now.min(end_at);
    for task in &state.tasks {
        if task.project_id != project_id || !task.running {
            continue;
        }
        let seconds = overlap_seconds(task.started_at, cap, start_at, end_at);
        if seconds == 0 {
            continue;
        }
        result.push(Entry {
            id: format!("active-{}", task.id),
            task_id: task.id.clone(),
            task_title: task.title.clone(),
            note: String::new(),
            project_id: task.project_id.clone(),
            started_at: task.started_at.max(start_at),
            ended_at: cap,
            seconds,
        });
    }
    result.sort_by_key(|entry| entry.started_at);
    result
}

fn append_entry(
    state: &mut State,
    task: &Task,
    started_at: i64,
    ended_at: i64,
    seconds: i64,
    note: &str,
) {
    let seconds = seconds.max(0);
    if seconds == 0 {
        return;
    }
    state.entries.push(Entry {
        id: make_id("entry"),
        project_id: task.project_id.clone(),
        task_id: task.id.clone(),
        task_title: task.title.clone(),
        note: sanitize_text(note, 240),
        started_at: started_at.max(0),
        ended_at: ended_at.max(0),
        seconds,
    });
}

pub fn create_project(path: &Path, name: &str) -> Result<String> {
    mutate_state(path, |state| {
        let clean_name = sanitize_text(name, 80);
        let project = Project {
            id: make_id("project"),
            name: if clean_name.is_empty() {
                "New project".to_owned()
            } else {
                clean_name
            },
            template_id: "detailed".to_owned(),
            client_name: String::new(),
            company_name: String::new(),
            logo_path: String::new(),
            accent_color: "#476a89".to_owned(),
            paper: "a4".to_owned(),
            export_weekly: true,
            export_monthly: true,
        };
        state.active_project_id = project.id.clone();
        let id = project.id.clone();
        state.projects.push(project);
        Ok(id)
    })
}

pub fn select_project(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        if !state.projects.iter().any(|project| project.id == id) {
            bail!("project {id} does not exist")
        }
        state.active_project_id = id.to_owned();
        Ok(())
    })
}

pub fn update_project(path: &Path, id: &str, changes: ProjectChanges) -> Result<()> {
    mutate_state(path, |state| {
        let project = state
            .projects
            .iter_mut()
            .find(|project| project.id == id)
            .with_context(|| format!("project {id} does not exist"))?;
        if let Some(name) = changes.name {
            project.name = name;
        }
        if let Some(client_name) = changes.client_name {
            project.client_name = client_name;
        }
        if let Some(company_name) = changes.company_name {
            project.company_name = company_name;
        }
        if let Some(template_id) = changes.template_id {
            project.template_id = template_id;
        }
        if let Some(export_weekly) = changes.export_weekly {
            project.export_weekly = export_weekly;
        }
        if let Some(export_monthly) = changes.export_monthly {
            project.export_monthly = export_monthly;
        }
        Ok(())
    })
}

pub fn add_task(path: &Path, title: Option<&str>) -> Result<String> {
    mutate_state(path, |state| {
        let clean_title = title
            .map(|title| sanitize_text(title, 160))
            .unwrap_or_default();
        let task = Task {
            id: make_id("task"),
            project_id: state.active_project_id.clone(),
            title: if clean_title.is_empty() {
                "Empty".to_owned()
            } else {
                clean_title
            },
            legacy_seconds: 0,
            running: false,
            started_at: 0,
            display_since: 0,
        };
        let id = task.id.clone();
        state.tasks.push(task);
        Ok(id)
    })
}

pub fn start_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        let task = state
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        if !task.running {
            task.running = true;
            task.started_at = now_ms();
        }
        Ok(())
    })
}

pub fn stop_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        let index = state
            .tasks
            .iter()
            .position(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        if !state.tasks[index].running {
            return Ok(());
        }
        let now = now_ms();
        let task = state.tasks[index].clone();
        append_entry(
            state,
            &task,
            task.started_at,
            now,
            (now - task.started_at) / 1000,
            "",
        );
        state.tasks[index].running = false;
        state.tasks[index].started_at = 0;
        Ok(())
    })
}

pub fn reset_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        let index = state
            .tasks
            .iter()
            .position(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        let now = now_ms();
        if state.tasks[index].running {
            let task = state.tasks[index].clone();
            append_entry(
                state,
                &task,
                task.started_at,
                now,
                (now - task.started_at) / 1000,
                "",
            );
            state.tasks[index].started_at = now;
        }
        state.tasks[index].display_since = now;
        Ok(())
    })
}

pub fn reset_active_project(path: &Path) -> Result<()> {
    mutate_state(path, |state| {
        let now = now_ms();
        let active_project_id = state.active_project_id.clone();
        let indices: Vec<usize> = state
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| (task.project_id == active_project_id).then_some(index))
            .collect();
        for index in indices {
            if state.tasks[index].running {
                let task = state.tasks[index].clone();
                append_entry(
                    state,
                    &task,
                    task.started_at,
                    now,
                    (now - task.started_at) / 1000,
                    "",
                );
                state.tasks[index].started_at = now;
            }
            state.tasks[index].display_since = now;
        }
        Ok(())
    })
}

pub fn remove_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        let index = state
            .tasks
            .iter()
            .position(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        if state.tasks[index].running {
            let now = now_ms();
            let task = state.tasks[index].clone();
            append_entry(
                state,
                &task,
                task.started_at,
                now,
                (now - task.started_at) / 1000,
                "",
            );
        }
        state.tasks.remove(index);
        Ok(())
    })
}

pub fn edit_task(
    path: &Path,
    id: &str,
    title: Option<&str>,
    add_duration: Option<&str>,
) -> Result<()> {
    let parsed_duration = add_duration.map(parse_duration).transpose()?;
    mutate_state(path, |state| {
        let index = state
            .tasks
            .iter()
            .position(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        if let Some(title) = title {
            let title = sanitize_text(title, 160);
            state.tasks[index].title = if title.is_empty() {
                "Empty".to_owned()
            } else {
                title
            };
        }
        if let Some(seconds) = parsed_duration
            && seconds > 0
        {
            let now = now_ms();
            let task = state.tasks[index].clone();
            append_entry(
                state,
                &task,
                now - seconds * 1000,
                now,
                seconds,
                "Manual entry",
            );
        }
        Ok(())
    })
}

pub fn update_drive(path: &Path, remote: &str, folder: &str, sync_on_startup: bool) -> Result<()> {
    mutate_state(path, |state| {
        state.drive.remote = sanitize_text(remote, 80);
        state.drive.folder = sanitize_text(folder, 160);
        if state.drive.folder.is_empty() {
            state.drive.folder = default_drive_folder();
        }
        state.drive.sync_on_startup = sync_on_startup;
        Ok(())
    })
}

pub fn status(path: &Path) -> Result<Status> {
    let state = locked_state(path)?;
    let now = now_ms();
    let active_project = state
        .projects
        .iter()
        .find(|project| project.id == state.active_project_id)
        .cloned();
    let active_tasks = state
        .tasks
        .iter()
        .filter(|task| task.project_id == state.active_project_id)
        .cloned()
        .map(|task| TaskView {
            display_seconds: task_seconds(&state, &task, now),
            task,
        })
        .collect();
    let typst_available = command_available("typst");
    let rclone_available = command_available("rclone");
    let setup_status = dependency_status_text(typst_available, rclone_available);
    let dependencies = DependencyStatus {
        typst_available,
        rclone_available,
        setup_status: setup_status.clone(),
    };
    Ok(Status {
        total_tracked_seconds: total_seconds(&state, now, None),
        active_project_seconds: total_seconds(&state, now, Some(&state.active_project_id)),
        running_timers: state.tasks.iter().filter(|task| task.running).count(),
        report_status: report_status_text(&state),
        sync_status: state.sync.status.clone(),
        sync_error: state.sync.error.clone(),
        background_checks_enabled: report_timer_enabled(),
        state,
        now_ms: now,
        active_project,
        active_tasks,
        setup_status,
        dependencies,
    })
}

pub fn last_completed_period(period: &str, now: i64) -> Result<Period> {
    let now = Local
        .timestamp_millis_opt(now)
        .single()
        .context("current time is outside the local calendar")?;
    let date = now.date_naive();
    let end = match period {
        "weekly" => date - Days::new(date.weekday().num_days_from_monday().into()),
        "monthly" => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).context("invalid month")?
        }
        _ => bail!("period must be weekly or monthly"),
    };
    let start = match period {
        "weekly" => end - Days::new(7),
        "monthly" if end.month() == 1 => {
            NaiveDate::from_ymd_opt(end.year() - 1, 12, 1).context("invalid previous month")?
        }
        "monthly" => NaiveDate::from_ymd_opt(end.year(), end.month() - 1, 1)
            .context("invalid previous month")?,
        _ => unreachable!(),
    };
    Ok(Period {
        start_at: local_midnight(start)?,
        end_at: local_midnight(end)?,
    })
}

fn next_period_start(period: &str, start_at: i64) -> Result<i64> {
    let start = Local
        .timestamp_millis_opt(start_at)
        .single()
        .context("period start is outside the local calendar")?
        .date_naive();
    let next = match period {
        "weekly" => start + Days::new(7),
        "monthly" if start.month() == 12 => {
            NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).context("invalid next month")?
        }
        "monthly" => NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
            .context("invalid next month")?,
        _ => bail!("period must be weekly or monthly"),
    };
    local_midnight(next)
}

fn local_midnight(date: NaiveDate) -> Result<i64> {
    Local
        .from_local_datetime(
            &date
                .and_hms_opt(0, 0, 0)
                .context("invalid local midnight")?,
        )
        .earliest()
        .map(|date_time| date_time.timestamp_millis())
        .context("local midnight does not exist")
}

fn local_date_text(value: i64) -> String {
    Local
        .timestamp_millis_opt(value)
        .single()
        .map(|date| format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day()))
        .unwrap_or_else(|| "1970-01-01".to_owned())
}

fn local_time_text(value: i64) -> String {
    Local
        .timestamp_millis_opt(value)
        .single()
        .map(|date| format!("{:02}:{:02}", date.hour(), date.minute()))
        .unwrap_or_else(|| "00:00".to_owned())
}

fn period_key(project_id: &str, period: &str, start_at: i64) -> String {
    format!("{project_id}:{period}:{}", local_date_text(start_at))
}

fn safe_key(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn build_snapshot(
    state: &State,
    project: &Project,
    period: &str,
    start_at: i64,
    end_at: i64,
) -> ReportSnapshot {
    let entries = entries_for_period(state, &project.id, start_at, end_at, now_ms());
    let entries: Vec<ReportEntry> = entries
        .into_iter()
        .map(|entry| ReportEntry {
            task: entry.task_title,
            note: entry.note,
            date: local_date_text(entry.started_at),
            started: local_time_text(entry.started_at),
            ended: local_time_text(entry.ended_at),
            duration: format_duration(entry.seconds),
            seconds: entry.seconds,
        })
        .collect();
    let total_seconds = entries.iter().map(|entry| entry.seconds).sum();
    ReportSnapshot {
        generated_at: now_ms(),
        project: SnapshotProject {
            id: project.id.clone(),
            name: project.name.clone(),
            client_name: project.client_name.clone(),
            company_name: project.company_name.clone(),
            logo_path: project.logo_path.clone(),
            accent_color: project.accent_color.clone(),
            paper: project.paper.clone(),
        },
        period: SnapshotPeriod {
            kind: period.to_owned(),
            label: if period == "monthly" {
                "Monthly time report".to_owned()
            } else {
                "Weekly time report".to_owned()
            },
            start: local_date_text(start_at),
            end: local_date_text(end_at - 1),
            start_at,
            end_at,
        },
        total_seconds,
        total_duration: format_duration(total_seconds),
        entries,
    }
}

fn queue_report(
    state: &mut State,
    project: &Project,
    period: &str,
    start_at: i64,
    end_at: i64,
    include_empty: bool,
) -> Result<Option<String>> {
    let key = period_key(&project.id, period, start_at);
    if state.reports.iter().any(|report| report.key == key) {
        return Ok(Some(key));
    }
    let snapshot = build_snapshot(state, project, period, start_at, end_at);
    if !include_empty && snapshot.entries.is_empty() {
        return Ok(None);
    }
    let cache = cache_path()?;
    fs::create_dir_all(&cache).with_context(|| format!("could not create {}", cache.display()))?;
    let safe_key = safe_key(&key);
    let data_path = cache.join(format!("{safe_key}.json"));
    let typ_path = cache.join(format!("{safe_key}.typ"));
    let pdf_path = cache.join(format!("{safe_key}.pdf"));
    let snapshot = serde_json::to_string_pretty(&snapshot)
        .context("could not serialize report snapshot")?
        + "\n";
    atomic_write(&data_path, snapshot.as_bytes())?;
    state.reports.push(Report {
        key: key.clone(),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        period: period.to_owned(),
        start_at,
        end_at,
        template_id: project.template_id.clone(),
        status: "pending".to_owned(),
        data_path: data_path.display().to_string(),
        typ_path: typ_path.display().to_string(),
        pdf_path: pdf_path.display().to_string(),
        remote_path: String::new(),
        created_at: now_ms(),
        completed_at: 0,
        attempts: 0,
        last_error: String::new(),
    });
    Ok(Some(key))
}

fn earliest_project_entry(state: &State, project_id: &str) -> Option<i64> {
    state
        .entries
        .iter()
        .filter(|entry| entry.project_id == project_id)
        .map(|entry| entry.started_at)
        .chain(
            state
                .tasks
                .iter()
                .filter(|task| task.project_id == project_id && task.running)
                .map(|task| task.started_at),
        )
        .filter(|value| *value > 0)
        .min()
}

fn queue_missing_periods(state: &mut State, project: &Project, period: &str) -> Result<()> {
    let Some(earliest) = earliest_project_entry(state, &project.id) else {
        return Ok(());
    };
    let first = match period {
        "weekly" => {
            let period = last_completed_period("weekly", earliest + 7 * 24 * 60 * 60 * 1000)?;
            period.start_at
        }
        "monthly" => {
            let period = last_completed_period("monthly", earliest + 32 * 24 * 60 * 60 * 1000)?;
            period.start_at
        }
        _ => bail!("period must be weekly or monthly"),
    };
    let cutoff = match period {
        "weekly" => last_completed_period("weekly", now_ms())?.end_at,
        "monthly" => last_completed_period("monthly", now_ms())?.end_at,
        _ => unreachable!(),
    };
    let mut current = first;
    let mut limit = if period == "monthly" { 36 } else { 156 };
    while current < cutoff && limit > 0 {
        let end = next_period_start(period, current)?;
        queue_report(state, project, period, current, end, false)?;
        current = end;
        limit -= 1;
    }
    Ok(())
}

pub fn check_reports(path: &Path) -> Result<()> {
    mutate_state(path, |state| {
        let projects = state.projects.clone();
        for project in projects {
            if project.export_weekly {
                queue_missing_periods(state, &project, "weekly")?;
            }
            if project.export_monthly {
                queue_missing_periods(state, &project, "monthly")?;
            }
        }
        Ok(())
    })?;
    process_pending_reports(path)
}

pub fn export_report(path: &Path, period: &str) -> Result<()> {
    let bounds = last_completed_period(period, now_ms())?;
    mutate_state(path, |state| {
        let project = state
            .projects
            .iter()
            .find(|project| project.id == state.active_project_id)
            .cloned()
            .context("active project does not exist")?;
        queue_report(
            state,
            &project,
            period,
            bounds.start_at,
            bounds.end_at,
            true,
        )?;
        Ok(())
    })?;
    process_pending_reports(path)
}

pub fn retry_reports(path: &Path) -> Result<()> {
    mutate_state(path, |state| {
        for report in &mut state.reports {
            if ["failed", "rendering", "uploading"].contains(&report.status.as_str()) {
                report.status = "pending".to_owned();
                if report.last_error.is_empty() {
                    report.last_error = "Recovered after an interrupted export".to_owned();
                }
            }
        }
        Ok(())
    })?;
    process_pending_reports(path)
}

fn process_pending_reports(path: &Path) -> Result<()> {
    loop {
        let next = mutate_state(path, |state| {
            let Some(index) = state
                .reports
                .iter()
                .position(|report| report.status == "pending")
            else {
                return Ok(None);
            };
            let report = &mut state.reports[index];
            report.status = "rendering".to_owned();
            report.attempts += 1;
            report.last_error.clear();
            Ok(Some((report.clone(), state.drive.clone())))
        })?;
        let Some((report, drive)) = next else {
            return Ok(());
        };

        if let Err(error) = render_report(&report) {
            mark_report_failure(path, &report.key, &format!("Typst failed: {error:#}"))?;
            continue;
        }
        if let Err(error) = upload_report(path, &report, &drive) {
            mark_report_failure(
                path,
                &report.key,
                &format!("rclone upload failed: {error:#}"),
            )?;
            continue;
        }
        mutate_state(path, |state| {
            let report = state
                .reports
                .iter_mut()
                .find(|candidate| candidate.key == report.key)
                .context("queued report no longer exists")?;
            report.status = "complete".to_owned();
            report.completed_at = now_ms();
            report.last_error.clear();
            Ok(())
        })?;
    }
}

fn mark_report_failure(path: &Path, key: &str, error: &str) -> Result<()> {
    mutate_state(path, |state| {
        let report = state
            .reports
            .iter_mut()
            .find(|report| report.key == key)
            .with_context(|| format!("report {key} no longer exists"))?;
        report.status = "failed".to_owned();
        report.last_error = sanitize_text(error, 500);
        Ok(())
    })
}

fn render_report(report: &Report) -> Result<()> {
    if !command_available("typst") {
        bail!("Typst is not installed")
    }
    let template = template_path(&report.template_id)?;
    let data_path = PathBuf::from(&report.data_path);
    if !data_path.is_file() {
        bail!("report snapshot {} is missing", data_path.display())
    }
    let typ_path = PathBuf::from(&report.typ_path);
    let pdf_path = PathBuf::from(&report.pdf_path);
    let home = home_dir()?;
    let source = format!(
        "#import {}: render\n#render(json({}))\n",
        typst_string(&typst_root_path(&template, &home)),
        typst_string(&typst_root_path(&data_path, &home)),
    );
    atomic_write(&typ_path, source.as_bytes())?;
    run_command(
        "typst",
        [
            "compile".to_owned(),
            "--root".to_owned(),
            home.display().to_string(),
            typ_path.display().to_string(),
            pdf_path.display().to_string(),
        ],
    )?;
    if !pdf_path.is_file() {
        bail!("Typst completed without creating {}", pdf_path.display())
    }
    Ok(())
}

fn upload_report(path: &Path, report: &Report, drive: &Drive) -> Result<()> {
    if !command_available("rclone") {
        bail!("rclone is not installed")
    }
    if !valid_remote(&drive.remote) {
        bail!("configure a valid rclone remote first")
    }
    let destination = remote_path(
        drive,
        &format!(
            "reports/{}/{}/{}.pdf",
            slug(&report.project_name),
            report.period,
            local_date_text(report.start_at)
        ),
    )?;
    mutate_state(path, |state| {
        let queued = state
            .reports
            .iter_mut()
            .find(|candidate| candidate.key == report.key)
            .context("queued report no longer exists")?;
        queued.status = "uploading".to_owned();
        queued.remote_path = destination.clone();
        Ok(())
    })?;
    run_command(
        "rclone",
        [
            "copyto".to_owned(),
            "--checksum".to_owned(),
            "--retries".to_owned(),
            "3".to_owned(),
            "--low-level-retries".to_owned(),
            "3".to_owned(),
            report.pdf_path.clone(),
            destination,
        ],
    )
}

pub fn sync_state(path: &Path) -> Result<()> {
    let drive = mutate_state(path, |state| {
        state.sync.status = "Syncing local state to Drive".to_owned();
        state.sync.error.clear();
        Ok(state.drive.clone())
    })?;
    let result = (|| {
        if !command_available("rclone") {
            bail!("rclone is not installed")
        }
        if !valid_remote(&drive.remote) {
            bail!("configure a valid rclone remote first")
        }
        run_command(
            "rclone",
            [
                "copyto".to_owned(),
                "--checksum".to_owned(),
                "--retries".to_owned(),
                "3".to_owned(),
                "--low-level-retries".to_owned(),
                "3".to_owned(),
                path.display().to_string(),
                remote_path(&drive, "state.json")?,
            ],
        )
    })();
    mutate_state(path, |state| {
        match &result {
            Ok(()) => {
                state.sync.status = "Local state is synced to Drive".to_owned();
                state.sync.error.clear();
                state.sync.last_synced_at = now_ms();
            }
            Err(error) => {
                state.sync.status = "Sync pending".to_owned();
                state.sync.error = sanitize_text(format!("rclone sync failed: {error:#}"), 500);
            }
        }
        Ok(())
    })?;
    result
}

pub fn install_report_timer(path: &Path) -> Result<()> {
    let unit_dir = systemd_user_unit_dir()?;
    fs::create_dir_all(&unit_dir)
        .with_context(|| format!("could not create {}", unit_dir.display()))?;
    let executable = std::env::current_exe().context("could not locate omatracker binary")?;
    let service_name = "omatracker-report-check.service";
    let timer_name = "omatracker-report-check.timer";
    let service = format!(
        "[Unit]\nDescription=OmaTracker report check\n\n[Service]\nType=oneshot\nExecStart={} --data-path {} report check\n",
        systemd_argument(&executable),
        systemd_argument(path),
    );
    let timer = "[Unit]\nDescription=Run OmaTracker report checks\n\n[Timer]\nOnBootSec=2m\nOnUnitActiveSec=15m\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n";
    atomic_write(&unit_dir.join(service_name), service.as_bytes())?;
    atomic_write(&unit_dir.join(timer_name), timer.as_bytes())?;
    run_command(
        "systemctl",
        ["--user".to_owned(), "daemon-reload".to_owned()],
    )?;
    run_command(
        "systemctl",
        [
            "--user".to_owned(),
            "enable".to_owned(),
            "--now".to_owned(),
            timer_name.to_owned(),
        ],
    )
}

pub fn remove_report_timer() -> Result<()> {
    let unit_dir = systemd_user_unit_dir()?;
    let service = unit_dir.join("omatracker-report-check.service");
    let timer = unit_dir.join("omatracker-report-check.timer");
    let _ = Command::new("systemctl")
        .args([
            "--user",
            "disable",
            "--now",
            "omatracker-report-check.timer",
        ])
        .output();
    if service.exists() {
        fs::remove_file(&service)
            .with_context(|| format!("could not remove {}", service.display()))?;
    }
    if timer.exists() {
        fs::remove_file(&timer).with_context(|| format!("could not remove {}", timer.display()))?;
    }
    run_command(
        "systemctl",
        ["--user".to_owned(), "daemon-reload".to_owned()],
    )
}

fn systemd_user_unit_dir() -> Result<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".config"));
    Ok(config.join("systemd/user"))
}

fn systemd_argument(value: &Path) -> String {
    format!(
        "\"{}\"",
        value
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    )
}

fn report_timer_enabled() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", "omatracker-report-check.timer"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn template_path(template_id: &str) -> Result<PathBuf> {
    if let Some(template_dir) = std::env::var_os("OMATRACKER_TEMPLATE_DIR") {
        let template = PathBuf::from(template_dir).join(format!("{template_id}.typ"));
        if template.is_file() {
            return Ok(template);
        }
        bail!("Typst template override {} is missing", template.display())
    }
    let executable = std::env::current_exe().context("could not locate omatracker binary")?;
    let binary_dir = executable
        .parent()
        .context("omatracker binary has no parent directory")?;
    let root = binary_dir
        .parent()
        .context("omatracker binary must be located in a bin directory")?;
    let template = root.join("templates").join(format!("{template_id}.typ"));
    if template.is_file() {
        return Ok(template);
    }
    let development_template = std::env::current_dir()
        .context("could not determine current directory")?
        .join("templates")
        .join(format!("{template_id}.typ"));
    if development_template.is_file() {
        return Ok(development_template);
    }
    bail!("bundled Typst template {} is missing", template.display())
}

fn typst_root_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map(|relative| format!("/{}", relative.display()))
        .unwrap_or_else(|_| path.display().to_string())
}

fn typst_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

fn command_available(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn run_command<I>(command: &str, arguments: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let output = Command::new(command)
        .args(arguments)
        .output()
        .with_context(|| format!("could not start {command}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = output_summary(&output.stdout, &output.stderr);
    bail!(
        "{command} exited with {}{}",
        output.status,
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    )
}

fn output_summary(stdout: &[u8], stderr: &[u8]) -> String {
    let text = if stderr.is_empty() { stdout } else { stderr };
    let text = String::from_utf8_lossy(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() > 400 {
        format!("{}...", text.chars().take(397).collect::<String>())
    } else {
        text
    }
}

fn valid_remote(remote: &str) -> bool {
    !remote.is_empty()
        && remote.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
}

fn remote_path(drive: &Drive, suffix: &str) -> Result<String> {
    if !valid_remote(&drive.remote) {
        bail!("invalid rclone remote")
    }
    let folder = drive
        .folder
        .trim_matches('/')
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '.' | '/' | '-')
            {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    Ok(format!("{}:{folder}/{suffix}", drive.remote))
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_separator = false;
    for character in sanitize_text(value, 80).to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_separator = false;
        } else if !previous_separator && !slug.is_empty() {
            slug.push('-');
            previous_separator = true;
        }
    }
    slug.trim_matches('-')
        .to_owned()
        .chars()
        .take(80)
        .collect::<String>()
        .if_empty("project")
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self
        }
    }
}

fn is_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn dependency_status_text(typst_available: bool, rclone_available: bool) -> String {
    match (typst_available, rclone_available) {
        (true, true) => "Typst and rclone are ready".to_owned(),
        (false, false) => "Typst and rclone are not installed".to_owned(),
        (false, true) => "Typst is not installed".to_owned(),
        (true, false) => "rclone is not installed".to_owned(),
    }
}

fn report_status_text(state: &State) -> String {
    let mut complete = 0;
    for report in &state.reports {
        match report.status.as_str() {
            "failed" => return format!("PDF pending: {}", report.last_error),
            "rendering" => return "Generating PDF report".to_owned(),
            "uploading" => return "Uploading PDF report to Drive".to_owned(),
            "pending" => return "PDF report queued".to_owned(),
            "complete" => complete += 1,
            _ => {}
        }
    }
    if complete == 0 {
        "No PDF reports queued".to_owned()
    } else {
        format!(
            "{complete} PDF report{} exported",
            if complete == 1 { "" } else { "s" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_without_inventing_dates() {
        let state = parse_state(r#"{"version":1,"tasks":[{"id":"old","title":"Legacy","seconds":5400,"running":true,"startedAt":1000}]}"#).unwrap();
        assert_eq!(state.version, 2);
        assert_eq!(state.projects[0].name, "Unassigned");
        assert_eq!(state.tasks[0].legacy_seconds, 5400);
        assert!(state.entries.is_empty());
        assert!(state.tasks[0].running);
    }

    #[test]
    fn slices_entries_at_report_boundaries() {
        let mut state = State::default();
        let project_id = state.active_project_id.clone();
        let day = local_midnight(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()).unwrap();
        state.entries.push(Entry {
            id: "entry-a".to_owned(),
            project_id: project_id.clone(),
            task_id: "task-a".to_owned(),
            task_title: "Design".to_owned(),
            note: String::new(),
            started_at: day - 30 * 60 * 1000,
            ended_at: day + 90 * 60 * 1000,
            seconds: 7200,
        });
        let entries = entries_for_period(
            &state,
            &project_id,
            day,
            day + 60 * 60 * 1000,
            day + 2 * 60 * 60 * 1000,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].seconds, 3600);
        assert_eq!(entries[0].started_at, day);
        assert_eq!(entries[0].ended_at, day + 60 * 60 * 1000);
    }

    #[test]
    fn reset_hides_old_counter_without_removing_reportable_time() {
        let mut state = State::default();
        let project_id = state.active_project_id.clone();
        let day = local_midnight(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()).unwrap();
        let task = Task {
            id: "task-a".to_owned(),
            project_id: project_id.clone(),
            title: "Design".to_owned(),
            legacy_seconds: 120,
            running: false,
            started_at: 0,
            display_since: day + 60 * 1000,
        };
        state.tasks.push(task.clone());
        state.entries.push(Entry {
            id: "entry-a".to_owned(),
            project_id: project_id.clone(),
            task_id: "task-a".to_owned(),
            task_title: "Design".to_owned(),
            note: String::new(),
            started_at: day,
            ended_at: day + 3 * 60 * 1000,
            seconds: 180,
        });
        assert_eq!(task_seconds(&state, &task, day + 5 * 60 * 1000), 120);
        assert_eq!(
            entries_for_period(
                &state,
                &project_id,
                day,
                day + 4 * 60 * 1000,
                day + 5 * 60 * 1000
            )[0]
            .seconds,
            180
        );
    }

    #[test]
    fn parses_cli_durations() {
        assert_eq!(parse_duration("1h 30m").unwrap(), 5400);
        assert_eq!(parse_duration("01:02:03").unwrap(), 3723);
        assert!(parse_duration("after lunch").is_err());
    }

    #[test]
    fn mutations_persist_a_dated_manual_entry() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("state.json");
        let task_id = add_task(&path, Some("Design")).unwrap();
        edit_task(&path, &task_id, Some("Design review"), Some("15m")).unwrap();

        let state = locked_state(&path).unwrap();
        assert_eq!(state.tasks[0].title, "Design review");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].seconds, 900);
        assert_eq!(state.entries[0].note, "Manual entry");
    }
}
