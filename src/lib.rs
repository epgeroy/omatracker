use anyhow::{Context, Result, bail};
use chrono::{Datelike, Days, Local, NaiveDate, TimeZone, Timelike};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use uuid::Uuid;

pub mod agent;
pub mod billing;
pub mod clear_data;
mod entities;
pub mod feedback;
mod rates;
pub mod skills;
mod task_rates;
pub mod templates;
pub use rates::{Estimate, HourlyRate};

pub const STATE_VERSION: u32 = 4;
pub const DEFAULT_PROJECT_ID: &str = "project-unassigned";

fn default_drive_folder() -> String {
    "OmaTracker".to_owned()
}

fn default_sync_status() -> String {
    "Not synced yet".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
    pub rate: Option<HourlyRate>,
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
    // A successful render survives upload failures. Older ledgers default to false.
    pub rendered: bool,
    // Empty for old ledgers; captured lazily on their next render.
    pub template_bundle: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
    pub billing: billing::Billing,
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
            billing: billing::Billing::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    #[serde(flatten)]
    pub task: Task,
    pub display_seconds: i64,
    #[serde(flatten)]
    pub billing: task_rates::View,
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
    pub active_project_estimate: Option<Estimate>,
    pub running_timers: usize,
    pub setup_status: String,
    pub report_status: String,
    pub sync_status: String,
    pub sync_error: String,
    pub background_checks_enabled: bool,
    pub dependencies: DependencyStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationState {
    pub version: u32,
    pub active_project_id: String,
    pub projects: Vec<Project>,
    pub drive: Drive,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationStatus {
    pub invoice_settings: billing::ProjectBilling,
    pub invoice_status: String,
    pub state: PresentationState,
    pub now_ms: i64,
    pub active_project: Option<Project>,
    pub active_tasks: Vec<TaskView>,
    pub running_tasks: Vec<TaskView>,
    pub preferences: feedback::Preferences,
    pub total_tracked_seconds: i64,
    pub active_project_seconds: i64,
    pub active_project_estimate: Option<Estimate>,
    pub running_timers: usize,
    pub report_status: String,
    pub sync_status: String,
    pub sync_error: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub setup_status: String,
    pub dependencies: DependencyStatus,
    pub background_checks_enabled: bool,
    pub background_checks_active: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    pub client_name: Option<String>,
    pub company_name: Option<String>,
    pub template_id: Option<String>,
    pub accent_color: Option<String>,
    pub paper: Option<String>,
    pub logo_path: Option<String>,
    pub export_weekly: Option<bool>,
    pub export_monthly: Option<bool>,
    pub hourly_rate: Option<String>,
    pub currency: Option<String>,
    pub clear_rate: bool,
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
    estimate: Option<Estimate>,
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
    rate: Option<HourlyRate>,
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
        rate: None,
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
    if !templates::valid_id(&project.template_id) {
        project.template_id = "detailed".to_owned();
    }
    project.client_name = sanitize_text(&project.client_name, 120);
    project.company_name = sanitize_text(&project.company_name, 120);
    if !is_color(&project.accent_color) {
        project.accent_color = "#476a89".to_owned();
    }
    if project.paper != "letter" {
        project.paper = "a4".to_owned();
    }
}

fn normalize_state(state: &mut State) {
    state.version = STATE_VERSION;
    state.billing.archived_projects.remove(DEFAULT_PROJECT_ID);
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
        if entry.seconds == 0
            && entry.ended_at > entry.started_at
            && !state.billing.entries.contains_key(&entry.id)
        {
            entry.seconds = (entry.ended_at - entry.started_at) / 1000;
        }
        entry.ended_at > entry.started_at
            && (entry.seconds > 0 || state.billing.entries.contains_key(&entry.id))
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
        if !templates::valid_id(&report.template_id) {
            report.template_id = "detailed".to_owned();
        }
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

    if !project_ids.contains(&state.active_project_id)
        || state
            .billing
            .archived_projects
            .contains(&state.active_project_id)
    {
        state.active_project_id = DEFAULT_PROJECT_ID.into();
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
    if value.is_array() || version < 2 {
        return Ok(migrate_legacy(value));
    }
    if version > STATE_VERSION as i64 {
        bail!("ledger version {version} is newer than this application supports")
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
    billing::backup_before_upgrade(path)?;
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
    let lock = open_lock_file(path)?;
    lock.lock_exclusive()
        .with_context(|| format!("could not lock {}", path.display()))?;
    Ok(lock)
}

fn open_lock_file(path: &Path) -> Result<File> {
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
    Ok(lock)
}

enum Mutation<T> {
    Unchanged(T),
    Changed(T),
}

fn mutate_state<T>(
    path: &Path,
    mutate: impl FnOnce(&mut State) -> Result<Mutation<T>>,
) -> Result<T> {
    let lock = lock_file(path)?;
    let mut state = read_state(path)?;
    billing::initialize(&mut state);
    let previous_revision = state.billing.revision;
    let result = match mutate(&mut state)? {
        Mutation::Unchanged(result) => result,
        Mutation::Changed(result) => {
            if state.billing.revision == previous_revision {
                state.billing.revision += 1;
            }
            normalize_state(&mut state);
            billing::initialize(&mut state);
            write_state(path, &state)?;
            result
        }
    };
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
    task_seconds_from_entries(
        task,
        state
            .entries
            .iter()
            .filter(|entry| entry.task_id == task.id),
        now,
    )
}

fn task_seconds_from_entries<'a>(
    task: &Task,
    entries: impl IntoIterator<Item = &'a Entry>,
    now: i64,
) -> i64 {
    let since = task.display_since.max(0);
    let mut total = if since > 0 {
        0
    } else {
        task.legacy_seconds.max(0)
    };
    for entry in entries {
        total += entry_display_seconds(entry, since);
    }
    if task.running {
        total += ((now - task.started_at.max(since)) / 1000).max(0);
    }
    total
}

fn entry_display_seconds(entry: &Entry, since: i64) -> i64 {
    if since > 0 {
        billing::entry_seconds(entry, since, entry.ended_at)
    } else {
        entry.seconds.max(0)
    }
}

pub fn total_seconds(state: &State, now: i64, project_id: Option<&str>) -> i64 {
    let totals = task_totals(state, now);
    state
        .tasks
        .iter()
        .zip(totals)
        .filter(|(task, _)| project_id.is_none_or(|project_id| task.project_id == project_id))
        .map(|(_, seconds)| seconds)
        .sum()
}

fn task_totals(state: &State, now: i64) -> Vec<i64> {
    // Index tasks rather than entries: auxiliary memory stays proportional to
    // task count, even when most of the ledger is historical time entries.
    let mut task_indices: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut totals = Vec::with_capacity(state.tasks.len());
    for (index, task) in state.tasks.iter().enumerate() {
        if state.billing.archived_projects.contains(&task.project_id) {
            totals.push(0);
            continue;
        }
        task_indices.entry(&task.id).or_default().push(index);
        totals.push(task_seconds_from_entries(task, std::iter::empty(), now));
    }
    for entry in &state.entries {
        if let Some(indices) = task_indices.get(entry.task_id.as_str()) {
            for &index in indices {
                totals[index] += entry_display_seconds(entry, state.tasks[index].display_since);
            }
        }
    }
    totals
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
        let seconds = billing::entry_seconds(entry, start_at, end_at);
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
    let entry = Entry {
        id: make_id("entry"),
        project_id: task.project_id.clone(),
        task_id: task.id.clone(),
        task_title: task.title.clone(),
        note: sanitize_text(note, 240),
        started_at: started_at.max(0),
        ended_at: ended_at.max(0),
        seconds,
    };
    billing::record_entry(state, entry);
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
            rate: None,
        };
        state.active_project_id = project.id.clone();
        let id = project.id.clone();
        state.projects.push(project);
        Ok(Mutation::Changed(id))
    })
}

pub fn select_project(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        entities::active_project(state, id)?;
        if state.active_project_id == id {
            return Ok(Mutation::Unchanged(()));
        }
        state.active_project_id = id.to_owned();
        Ok(Mutation::Changed(()))
    })
}

pub fn update_project(path: &Path, id: &str, changes: ProjectChanges) -> Result<()> {
    let requested_template = changes.template_id.clone();
    if let Some(template_id) = &changes.template_id {
        template_path(template_id)?;
    }
    if let Some(color) = &changes.accent_color
        && !is_color(color)
    {
        bail!("accent color must be a six-digit hex color such as #476a89")
    }
    if let Some(paper) = &changes.paper
        && !matches!(paper.as_str(), "a4" | "letter")
    {
        bail!("paper must be a4 or letter")
    }
    let logo_path = changes
        .logo_path
        .map(|logo| -> Result<String> {
            if logo.is_empty() {
                return Ok(logo);
            }
            let logo = if let Some(relative) = logo.strip_prefix("~/") {
                home_dir()?.join(relative)
            } else {
                PathBuf::from(logo)
            };
            let logo = logo.canonicalize().context("logo file does not exist")?;
            if !logo.is_file() {
                bail!("logo must be a file")
            }
            let extension = logo
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "svg" | "gif") {
                bail!("logo must be a PNG, JPEG, SVG, or GIF image")
            }
            Ok(logo.display().to_string())
        })
        .transpose()?;
    if changes.clear_rate && (changes.hourly_rate.is_some() || changes.currency.is_some()) {
        bail!("--clear-rate conflicts with --hourly-rate and --currency")
    }
    if changes.currency.is_some() && changes.hourly_rate.is_none() {
        bail!("--currency requires --hourly-rate")
    }
    mutate_state(path, |state| {
        entities::active_project(state, id)?;
        let project = state
            .projects
            .iter_mut()
            .find(|project| project.id == id)
            .with_context(|| format!("project {id} does not exist"))?;
        let previous = project.clone();
        if changes.clear_rate {
            project.rate = None;
        } else if let Some(amount) = changes.hourly_rate {
            let currency = changes
                .currency
                .as_deref()
                .or_else(|| project.rate.as_ref().map(HourlyRate::currency))
                .context("--currency is required when first setting an hourly rate")?;
            project.rate = Some(HourlyRate::parse(&amount, currency)?);
        }
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
        if let Some(color) = changes.accent_color {
            project.accent_color = color;
        }
        if let Some(paper) = changes.paper {
            project.paper = paper;
        }
        if let Some(logo) = logo_path {
            project.logo_path = logo;
        }
        if let Some(export_weekly) = changes.export_weekly {
            project.export_weekly = export_weekly;
        }
        if let Some(export_monthly) = changes.export_monthly {
            project.export_monthly = export_monthly;
        }
        normalize_project(project);
        let mut changed = *project != previous;
        let rate = project.rate.clone();
        if let Some(template) = requested_template {
            let settings = state
                .billing
                .projects
                .get_mut(id)
                .context("billing settings missing")?;
            changed |= settings.template_id != template;
            settings.template_id = template;
        }
        if rate != previous.rate {
            billing::record_rate(state, id, now_ms(), rate)?;
        }
        Ok(if !changed {
            Mutation::Unchanged(())
        } else {
            Mutation::Changed(())
        })
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
        Ok(Mutation::Changed(id))
    })
}

pub fn start_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        let project = &state
            .tasks
            .iter()
            .find(|t| t.id == id)
            .context("TASK_NOT_FOUND")?
            .project_id;
        entities::active_project(state, project)?;
        let task = state
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        if task.running {
            return Ok(Mutation::Unchanged(()));
        }
        task.running = true;
        task.started_at = now_ms();
        Ok(Mutation::Changed(()))
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
            return Ok(Mutation::Unchanged(()));
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
        Ok(Mutation::Changed(()))
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
        Ok(Mutation::Changed(()))
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
        if indices.is_empty() {
            return Ok(Mutation::Unchanged(()));
        }
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
        Ok(Mutation::Changed(()))
    })
}

pub fn remove_task(path: &Path, id: &str) -> Result<()> {
    mutate_state(path, |state| {
        entities::remove_task(state, id)?;
        Ok(Mutation::Changed(()))
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
        let mut changed = false;
        let index = state
            .tasks
            .iter()
            .position(|task| task.id == id)
            .with_context(|| format!("task {id} does not exist"))?;
        entities::active_project(state, &state.tasks[index].project_id)?;
        if let Some(title) = title {
            let title = sanitize_text(title, 160);
            let title = if title.is_empty() {
                "Empty".to_owned()
            } else {
                title
            };
            changed = state.tasks[index].title != title;
            state.tasks[index].title = title;
        }
        if let Some(seconds) = parsed_duration
            && seconds > 0
        {
            changed = true;
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
        Ok(if changed {
            Mutation::Changed(())
        } else {
            Mutation::Unchanged(())
        })
    })
}

pub fn update_drive(path: &Path, remote: &str, folder: &str, sync_on_startup: bool) -> Result<()> {
    mutate_state(path, |state| {
        let previous = state.drive.clone();
        state.drive.remote = sanitize_text(remote, 80);
        state.drive.folder = sanitize_text(folder, 160);
        if state.drive.folder.is_empty() {
            state.drive.folder = default_drive_folder();
        }
        state.drive.sync_on_startup = sync_on_startup;
        Ok(if state.drive == previous {
            Mutation::Unchanged(())
        } else {
            Mutation::Changed(())
        })
    })
}

pub fn status(path: &Path) -> Result<Status> {
    let state = locked_state(path)?;
    let presentation = build_presentation_status(&state, now_ms());
    let diagnostics = diagnostics(path);
    Ok(Status {
        total_tracked_seconds: presentation.total_tracked_seconds,
        active_project_seconds: presentation.active_project_seconds,
        active_project_estimate: presentation.active_project_estimate,
        running_timers: presentation.running_timers,
        report_status: presentation.report_status,
        sync_status: presentation.sync_status,
        sync_error: presentation.sync_error,
        background_checks_enabled: diagnostics.background_checks_enabled,
        state,
        now_ms: presentation.now_ms,
        active_project: presentation.active_project,
        active_tasks: presentation.active_tasks,
        setup_status: diagnostics.setup_status,
        dependencies: diagnostics.dependencies,
    })
}

pub fn presentation_status(path: &Path) -> Result<PresentationStatus> {
    let mut status = build_presentation_status(&locked_state(path)?, now_ms());
    // A damaged optional audio sidecar must not make the ledger unavailable.
    // The independent feedback poll surfaces its error to the UI.
    status.preferences = feedback::preferences(path).unwrap_or_default();
    Ok(status)
}

fn build_presentation_status(state: &State, now: i64) -> PresentationStatus {
    let totals = task_totals(state, now);
    let total_tracked_seconds = totals.iter().sum();
    let active_project_seconds = state
        .tasks
        .iter()
        .zip(&totals)
        .filter(|(task, _)| task.project_id == state.active_project_id)
        .map(|(_, seconds)| seconds)
        .sum();
    let active_project = state
        .projects
        .iter()
        .find(|project| project.id == state.active_project_id)
        .cloned();
    let active_tasks = state
        .tasks
        .iter()
        .zip(totals.iter().copied())
        .filter(|(task, _)| task.project_id == state.active_project_id)
        .map(|(task, display_seconds)| TaskView {
            billing: task_rates::view(state, task, now),
            display_seconds,
            task: task.clone(),
        })
        .collect();
    PresentationStatus {
        invoice_settings: billing::settings(state, &state.active_project_id).unwrap_or_default(),
        invoice_status: format!(
            "{} draft · {} issued · {} paid · {} archived reports",
            state
                .billing
                .invoices
                .iter()
                .filter(|i| i.state == "draft")
                .count(),
            state
                .billing
                .invoices
                .iter()
                .filter(|i| i.state == "issued")
                .count(),
            state
                .billing
                .invoices
                .iter()
                .filter(|i| i.state == "paid")
                .count(),
            state.reports.len()
        ),
        state: PresentationState {
            version: state.version,
            active_project_id: state.active_project_id.clone(),
            projects: state
                .projects
                .iter()
                .filter(|p| !state.billing.archived_projects.contains(&p.id))
                .cloned()
                .collect(),
            drive: state.drive.clone(),
        },
        now_ms: now,
        active_project_estimate: active_project
            .as_ref()
            .and_then(|project| project.rate.as_ref())
            .map(|rate| rate.estimate(active_project_seconds)),
        active_project,
        active_tasks,
        running_tasks: state
            .tasks
            .iter()
            .zip(totals)
            .filter(|(task, _)| task.running)
            .map(|(task, display_seconds)| TaskView {
                billing: task_rates::view(state, task, now),
                task: task.clone(),
                display_seconds,
            })
            .collect(),
        preferences: feedback::Preferences::default(),
        total_tracked_seconds,
        active_project_seconds,
        running_timers: state.tasks.iter().filter(|task| task.running).count(),
        report_status: report_status_text(state),
        sync_status: state.sync.status.clone(),
        sync_error: state.sync.error.clone(),
    }
}

pub fn diagnostics(path: &Path) -> Diagnostics {
    let typst_available = command_available("typst");
    let rclone_available = command_available("rclone");
    let setup_status = dependency_status_text(typst_available, rclone_available);
    let dependencies = DependencyStatus {
        typst_available,
        rclone_available,
        setup_status: setup_status.clone(),
    };
    let (background_checks_enabled, background_checks_active) = report_timer_state(path);
    Diagnostics {
        background_checks_enabled,
        background_checks_active,
        setup_status,
        dependencies,
    }
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
            rate: project.rate.clone(),
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
        estimate: project
            .rate
            .as_ref()
            .map(|rate| rate.estimate(total_seconds)),
        entries,
    }
}

fn queue_report(
    state: &mut State,
    known_keys: &mut HashSet<String>,
    project: &Project,
    period: &str,
    start_at: i64,
    end_at: i64,
    include_empty: bool,
) -> Result<Option<String>> {
    let key = period_key(&project.id, period, start_at);
    if known_keys.contains(&key) {
        return Ok(Some(key));
    }
    let snapshot = build_snapshot(state, project, period, start_at, end_at);
    if !include_empty && snapshot.entries.is_empty() {
        return Ok(None);
    }
    let cache = cache_path()?;
    fs::create_dir_all(&cache).with_context(|| format!("could not create {}", cache.display()))?;
    let safe_key = safe_key(&key);
    let bundle = cache.join(format!("{safe_key}-{}", Uuid::new_v4()));
    templates::capture(
        &project.template_id,
        &serde_json::to_value(&snapshot)?,
        &bundle,
    )?;
    let data_path = bundle.join("data.json");
    let typ_path = bundle.join("report.typ");
    let pdf_path = cache.join(format!("{safe_key}.pdf"));
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
        rendered: false,
        template_bundle: bundle.display().to_string(),
    });
    known_keys.insert(key.clone());
    Ok(Some(key))
}

fn containing_period_start(period: &str, timestamp: i64) -> Result<i64> {
    let date = Local
        .timestamp_millis_opt(timestamp)
        .single()
        .context("entry time is outside the local calendar")?
        .date_naive();
    let start = match period {
        "weekly" => date - Days::new(date.weekday().num_days_from_monday().into()),
        "monthly" => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).context("invalid month")?
        }
        _ => bail!("period must be weekly or monthly"),
    };
    local_midnight(start)
}

// Derive occupied periods from the ledger rather than walking every calendar
// period since the first entry. This also finds late manual entries in gaps.
fn occupied_periods(
    state: &State,
    project_id: &str,
    period: &str,
    now: i64,
) -> Result<BTreeSet<i64>> {
    let cutoff = containing_period_start(period, now)?;
    let spans = state
        .entries
        .iter()
        .filter(|entry| entry.project_id == project_id)
        .map(|entry| (entry.started_at, entry.ended_at))
        .chain(
            state
                .tasks
                .iter()
                .filter(|task| task.project_id == project_id && task.running)
                .map(|task| (task.started_at, now)),
        );
    let mut periods = BTreeSet::new();
    for (start, end) in spans {
        let end = end.min(cutoff);
        if start <= 0 || end - start < 1000 {
            continue;
        }
        let mut current = containing_period_start(period, start)?;
        while current < end {
            let next = next_period_start(period, current)?;
            if overlap_seconds(start, end, current, next) > 0 {
                periods.insert(current);
            }
            current = next;
        }
    }
    Ok(periods)
}

fn queue_missing_periods(
    state: &mut State,
    known_keys: &mut HashSet<String>,
    project: &Project,
    period: &str,
    now: i64,
) -> Result<()> {
    for start in occupied_periods(state, &project.id, period, now)? {
        if known_keys.contains(&period_key(&project.id, period, start)) {
            continue;
        }
        let end = next_period_start(period, start)?;
        queue_report(state, known_keys, project, period, start, end, false)?;
    }
    Ok(())
}

pub fn check_reports(path: &Path) -> Result<()> {
    mutate_state(path, |state| {
        let previous_count = state.reports.len();
        let mut known_keys = state
            .reports
            .iter()
            .map(|report| report.key.clone())
            .collect();
        let now = now_ms();
        let projects = state.projects.clone();
        for project in projects {
            if state.billing.archived_projects.contains(&project.id) {
                continue;
            }
            if project.export_weekly {
                queue_missing_periods(state, &mut known_keys, &project, "weekly", now)?;
            }
            if project.export_monthly {
                queue_missing_periods(state, &mut known_keys, &project, "monthly", now)?;
            }
        }
        Ok(if state.reports.len() == previous_count {
            Mutation::Unchanged(())
        } else {
            Mutation::Changed(())
        })
    })?;
    process_pending_reports(path)
}

pub fn export_report(path: &Path, period: &str) -> Result<()> {
    let bounds = last_completed_period(period, now_ms())?;
    mutate_state(path, |state| {
        let previous_count = state.reports.len();
        let mut known_keys = state
            .reports
            .iter()
            .map(|report| report.key.clone())
            .collect();
        let project = state
            .projects
            .iter()
            .find(|project| project.id == state.active_project_id)
            .cloned()
            .context("active project does not exist")?;
        queue_report(
            state,
            &mut known_keys,
            &project,
            period,
            bounds.start_at,
            bounds.end_at,
            true,
        )?;
        Ok(if state.reports.len() == previous_count {
            Mutation::Unchanged(())
        } else {
            Mutation::Changed(())
        })
    })?;
    process_pending_reports(path)
}

pub fn retry_reports(path: &Path) -> Result<()> {
    // Never reset a report being processed by another panel or systemd worker.
    let Some(_worker) = report_worker_lock(path)? else {
        return Ok(());
    };
    mutate_state(path, |state| {
        let mut changed = false;
        for report in &mut state.reports {
            if ["failed", "rendering", "uploading"].contains(&report.status.as_str()) {
                changed = true;
                report.status = "pending".to_owned();
                if report.last_error.is_empty() {
                    report.last_error = "Recovered after an interrupted export".to_owned();
                }
            }
        }
        Ok(if changed {
            Mutation::Changed(())
        } else {
            Mutation::Unchanged(())
        })
    })?;
    process_pending_reports_locked(path)
}

fn report_worker_lock(path: &Path) -> Result<Option<File>> {
    let lock = open_lock_file(&PathBuf::from(format!("{}.reports", path.display())))?;
    match lock.try_lock_exclusive() {
        Ok(()) => Ok(Some(lock)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error).context("could not lock report worker"),
    }
}

fn process_pending_reports(path: &Path) -> Result<()> {
    let Some(_worker) = report_worker_lock(path)? else {
        return Ok(());
    };
    process_pending_reports_locked(path)
}

fn process_pending_reports_locked(path: &Path) -> Result<()> {
    loop {
        let next = mutate_state(path, |state| {
            let Some(index) = state
                .reports
                .iter()
                .position(|report| report.status == "pending")
            else {
                return Ok(Mutation::Unchanged(None));
            };
            let report = &mut state.reports[index];
            report.status = "rendering".to_owned();
            report.attempts += 1;
            report.last_error.clear();
            Ok(Mutation::Changed(Some((
                report.clone(),
                state.drive.clone(),
            ))))
        })?;
        let Some((report, drive)) = next else {
            return Ok(());
        };

        if let Err(error) = ensure_rendered(&report) {
            mark_report_failure(
                path,
                &report.key,
                &format!("Typst failed: {error:#}"),
                false,
            )?;
            continue;
        }
        if let Err(error) = upload_report(path, &report, &drive) {
            mark_report_failure(
                path,
                &report.key,
                &format!("rclone upload failed: {error:#}"),
                true,
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
            Ok(Mutation::Changed(()))
        })?;
    }
}

fn mark_report_failure(path: &Path, key: &str, error: &str, rendered: bool) -> Result<()> {
    mutate_state(path, |state| {
        let report = state
            .reports
            .iter_mut()
            .find(|report| report.key == key)
            .with_context(|| format!("report {key} no longer exists"))?;
        report.status = "failed".to_owned();
        report.last_error = sanitize_text(error, 500);
        report.rendered = rendered;
        Ok(Mutation::Changed(()))
    })
}

fn ensure_rendered(report: &Report) -> Result<()> {
    if report.rendered && Path::new(&report.pdf_path).is_file() {
        return Ok(());
    }
    render_report(report)
}

fn render_report(report: &Report) -> Result<()> {
    let bundle = if report.template_bundle.is_empty() {
        // Legacy reports keep their existing PDF; if it needs rebuilding, pin
        // the currently available source once, under the report worker lock.
        let bundle = PathBuf::from(&report.typ_path).with_extension("bundle");
        if !bundle.exists() {
            let data = fs::read(&report.data_path).context("report snapshot is missing")?;
            templates::capture(
                &report.template_id,
                &serde_json::from_slice(&data)?,
                &bundle,
            )?;
        }
        bundle
    } else {
        PathBuf::from(&report.template_bundle)
    };
    if !bundle.join("manifest.json").is_file() {
        bail!("captured report template is missing: {}", bundle.display())
    }
    templates::compile(&bundle, Path::new(&report.pdf_path))
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
        queued.rendered = true;
        Ok(Mutation::Changed(()))
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
    let _worker = lock_file(&PathBuf::from(format!("{}.sync-worker", path.display())))?;
    let (drive, snapshot) = mutate_state(path, |state| {
        state.sync.status = "Syncing local state to Drive".to_owned();
        state.sync.error.clear();
        let snapshot = serde_json::to_vec(state).context("could not serialize sync snapshot")?;
        Ok(Mutation::Changed((state.drive.clone(), snapshot)))
    })?;
    let result = (|| {
        if !command_available("rclone") {
            bail!("rclone is not installed")
        }
        if !valid_remote(&drive.remote) {
            bail!("configure a valid rclone remote first")
        }
        // Foreground task commands can replace the live ledger during an upload.
        // Give rclone a stable source without holding the ledger lock over I/O.
        let mut snapshot_file = NamedTempFile::new().context("could not create sync snapshot")?;
        snapshot_file
            .write_all(&snapshot)
            .context("could not write sync snapshot")?;
        run_command(
            "rclone",
            [
                "copyto".to_owned(),
                "--checksum".to_owned(),
                "--retries".to_owned(),
                "3".to_owned(),
                "--low-level-retries".to_owned(),
                "3".to_owned(),
                snapshot_file.path().display().to_string(),
                remote_path(&drive, "state.json")?,
            ],
        )
    })();
    mutate_state(path, |state| {
        match &result {
            Ok(()) => {
                state.sync.status = "Local snapshot synced to Drive".to_owned();
                state.sync.error.clear();
                state.sync.last_synced_at = now_ms();
            }
            Err(error) => {
                state.sync.status = "Sync pending".to_owned();
                state.sync.error = sanitize_text(format!("rclone sync failed: {error:#}"), 500);
            }
        }
        Ok(Mutation::Changed(()))
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

fn report_timer_state(path: &Path) -> (bool, bool) {
    // The installed user timer serves one ledger. A panel using another ledger
    // must retain its own scheduler, as must a panel whose timer was stopped.
    let service = systemd_user_unit_dir().ok().and_then(|directory| {
        fs::read_to_string(directory.join("omatracker-report-check.service")).ok()
    });
    let suffix = format!(" --data-path {} report check", systemd_argument(path));
    if !service.is_some_and(|service| {
        service
            .lines()
            .any(|line| line.starts_with("ExecStart=") && line.ends_with(&suffix))
    }) {
        return (false, false);
    }
    let enabled = Command::new("systemctl")
        .args(["--user", "is-enabled", "omatracker-report-check.timer"])
        .output()
        .is_ok_and(|output| output.status.success());
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "omatracker-report-check.timer"])
        .output()
        .is_ok_and(|output| output.status.success());
    (enabled, active)
}

fn template_path(template_id: &str) -> Result<PathBuf> {
    if !templates::valid_id(template_id) {
        bail!("invalid template ID: {template_id}")
    }
    if template_id.starts_with("user:") {
        return templates::user_path(template_id);
    }
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
        assert_eq!(state.version, STATE_VERSION);
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

    #[test]
    fn aggregated_totals_preserve_resets_running_time_and_legacy_seconds() {
        let tasks = vec![
            Task {
                id: "reset".into(),
                project_id: DEFAULT_PROJECT_ID.into(),
                legacy_seconds: 999,
                display_since: 2500,
                running: true,
                started_at: 6000,
                ..Task::default()
            },
            Task {
                id: "legacy".into(),
                project_id: "other".into(),
                legacy_seconds: 40,
                ..Task::default()
            },
        ];
        let entries = vec![
            Entry {
                task_id: "reset".into(),
                started_at: 1000,
                ended_at: 5900,
                seconds: 4,
                ..Entry::default()
            },
            Entry {
                task_id: "legacy".into(),
                started_at: 1000,
                ended_at: 3000,
                seconds: 2,
                ..Entry::default()
            },
            Entry {
                task_id: "deleted".into(),
                started_at: 1000,
                ended_at: 91000,
                seconds: 90,
                ..Entry::default()
            },
        ];
        let state = State {
            tasks,
            entries,
            ..State::default()
        };
        let presentation = build_presentation_status(&state, 9500);
        assert_eq!(presentation.active_tasks[0].display_seconds, 6);
        assert_eq!(presentation.active_project_seconds, 6);
        assert_eq!(presentation.total_tracked_seconds, 48);
        assert_eq!(presentation.running_timers, 1);
        assert_eq!(task_totals(&state, 9500), vec![6, 42]);
        for (task, total) in state.tasks.iter().zip(task_totals(&state, 9500)) {
            assert_eq!(total, task_seconds(&state, task, 9500));
        }
    }

    #[test]
    #[cfg(unix)]
    fn no_op_commands_do_not_replace_the_ledger() {
        use std::os::unix::fs::MetadataExt;
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("state.json");
        let task_id = add_task(&path, Some("Design")).unwrap();
        // Keeping the original file open also prevents inode reuse masking a write.
        let original = File::open(&path).unwrap();
        let contents = fs::read(&path).unwrap();
        stop_task(&path, &task_id).unwrap();
        select_project(&path, DEFAULT_PROJECT_ID).unwrap();
        edit_task(&path, &task_id, Some("Design"), None).unwrap();
        update_drive(&path, "", "OmaTracker", false).unwrap();
        check_reports(&path).unwrap();
        retry_reports(&path).unwrap();
        assert_eq!(
            original.metadata().unwrap().ino(),
            fs::metadata(&path).unwrap().ino()
        );
        assert_eq!(contents, fs::read(&path).unwrap());

        start_task(&path, &task_id).unwrap();
        let running = File::open(&path).unwrap();
        start_task(&path, &task_id).unwrap();
        assert_eq!(
            running.metadata().unwrap().ino(),
            fs::metadata(&path).unwrap().ino()
        );
    }

    fn date(year: i32, month: u32, day: u32) -> i64 {
        local_midnight(NaiveDate::from_ymd_opt(year, month, day).unwrap()).unwrap()
    }

    #[test]
    fn report_periods_cover_long_histories_and_late_entries_without_empty_gaps() {
        let mut state = State::default();
        for start in [date(2020, 1, 10), date(2026, 8, 10)] {
            state.entries.push(Entry {
                project_id: DEFAULT_PROJECT_ID.into(),
                started_at: start,
                ended_at: start + 60000,
                seconds: 60,
                ..Entry::default()
            });
        }
        let now = date(2026, 9, 18);
        assert_eq!(
            occupied_periods(&state, DEFAULT_PROJECT_ID, "weekly", now).unwrap(),
            BTreeSet::from([date(2020, 1, 6), date(2026, 8, 10)])
        );
        assert_eq!(
            occupied_periods(&state, DEFAULT_PROJECT_ID, "monthly", now).unwrap(),
            BTreeSet::from([date(2020, 1, 1), date(2026, 8, 1)])
        );

        let start = date(2023, 2, 2);
        state.entries.push(Entry {
            project_id: DEFAULT_PROJECT_ID.into(),
            started_at: start,
            ended_at: start + 60000,
            seconds: 60,
            ..Entry::default()
        });
        assert_eq!(
            occupied_periods(&state, DEFAULT_PROJECT_ID, "monthly", now).unwrap(),
            BTreeSet::from([date(2020, 1, 1), date(2023, 2, 1), date(2026, 8, 1)])
        );
    }

    #[test]
    fn report_periods_handle_month_end_and_running_sessions() {
        let mut state = State::default();
        state.entries.push(Entry {
            project_id: DEFAULT_PROJECT_ID.into(),
            started_at: date(2024, 1, 31),
            ended_at: date(2024, 2, 2),
            seconds: 172800,
            ..Entry::default()
        });
        state.tasks.push(Task {
            project_id: DEFAULT_PROJECT_ID.into(),
            running: true,
            started_at: date(2024, 3, 1),
            ..Task::default()
        });
        assert_eq!(
            occupied_periods(&state, DEFAULT_PROJECT_ID, "monthly", date(2024, 4, 15)).unwrap(),
            BTreeSet::from([date(2024, 1, 1), date(2024, 2, 1), date(2024, 3, 1)])
        );
    }

    #[test]
    fn retry_does_not_reset_an_active_workers_report() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("state.json");
        let mut state = State::default();
        state.reports.push(Report {
            key: "in-flight".into(),
            project_id: DEFAULT_PROJECT_ID.into(),
            start_at: 1000,
            end_at: 2000,
            status: "uploading".into(),
            ..Report::default()
        });
        write_state(&path, &state).unwrap();
        let _worker = report_worker_lock(&path).unwrap().unwrap();
        retry_reports(&path).unwrap();
        assert_eq!(read_state(&path).unwrap().reports[0].status, "uploading");
    }
}
