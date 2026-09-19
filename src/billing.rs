//! Durable billing domain. All ledger mutations run under the existing ledger lock.
//! Rates and entry provenance are separate from the legacy presentation counters.
use crate::{Entry, HourlyRate, State, now_ms};
use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Billing {
    /// Assigned on first durable workflow; clearing the ledger invalidates it.
    pub ledger_id: String,
    pub initialized: bool,
    pub revision: u64,
    pub issuer: Party,
    pub clients: BTreeMap<String, Party>,
    pub projects: BTreeMap<String, ProjectBilling>,
    pub entries: BTreeMap<String, EntryBilling>,
    pub corrections: Vec<Correction>,
    pub invoices: Vec<Invoice>,
    pub sequences: BTreeMap<String, u64>,
    pub requests: BTreeMap<String, Receipt>,
    pub bindings: BTreeMap<String, String>,
    pub migration_log: Vec<Value>,
    pub archived_projects: BTreeSet<String>,
    pub archived_clients: BTreeSet<String>,
    pub task_rates: BTreeMap<String, Vec<crate::task_rates::RatePoint>>,
    pub task_rate_adjustments: Vec<crate::task_rates::Adjustment>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Party {
    pub name: String,
    pub address: String,
    pub email: String,
    pub registration_id: String,
    pub payment_instructions: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectBilling {
    pub client_id: String,
    pub template_id: String,
    pub cadence: String,
    pub timezone: String,
    pub due_days: u32,
    pub drive_folder: String,
    pub rates: Vec<RatePoint>,
}

impl Default for ProjectBilling {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            template_id: "invoice".into(),
            cadence: "monthly".into(),
            timezone: "UTC".into(),
            due_days: 30,
            drive_folder: String::new(),
            rates: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatePoint {
    pub effective_at: i64,
    pub rate: Option<HourlyRate>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EntryBilling {
    pub resolved: bool,
    pub rate: Option<HourlyRate>,
    pub externally_billed: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Correction {
    pub id: String,
    pub entry_id: String,
    pub before: i64,
    pub after: i64,
    pub reason: String,
    pub created_at: i64,
    pub reverses: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub fingerprint: String,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub entry_id: String,
    pub revision: u64,
    pub start_at: i64,
    pub end_at: i64,
    pub seconds: i64,
    pub task: String,
    pub note: String,
    pub rate: HourlyRate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    pub task: String,
    pub seconds: i64,
    pub duration: String,
    pub rate: HourlyRate,
    pub hourly_rate: String,
    pub amount_minor: String,
    pub amount_text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Excluded {
    pub non_billable_seconds: i64,
    pub unresolved_seconds: i64,
    pub already_billed_seconds: i64,
    pub running_timers: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: String,
    pub project_id: String,
    pub project: Value,
    pub revision: u64,
    pub state: String,
    pub number: String,
    pub currency: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub start_at: i64,
    pub end_at: i64,
    pub created_at: i64,
    pub issue_date: String,
    pub due_date: String,
    pub paid_at: Option<String>,
    pub due_days: u32,
    pub issuer: Party,
    pub client: Party,
    pub payment_instructions: String,
    pub lines: Vec<InvoiceLine>,
    pub allocations: Vec<Allocation>,
    pub excluded: Excluded,
    pub total_seconds: i64,
    pub total_minor: String,
    pub total_text: String,
    pub template_id: String,
    pub bundle: String,
    pub pdf_path: String,
    pub render_status: String,
    pub upload_status: String,
    pub remote_path: String,
    pub last_error: String,
    pub replaces: Option<String>,
    pub void_reason: String,
}

fn version_backup_path(path: &Path, version: u64) -> PathBuf {
    let suffix = if version >= 3 {
        "pre-task-rates.bak"
    } else {
        "pre-invoices.bak"
    };
    PathBuf::from(format!("{}.{suffix}", path.display()))
}

pub(crate) fn upgrade_backup_path(path: &Path) -> Result<Option<PathBuf>> {
    let version = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?["version"]
            .as_u64()
            .unwrap_or(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if version < u64::from(crate::STATE_VERSION) {
        Ok(Some(version_backup_path(path, version)))
    } else {
        Ok([version_backup_path(path, 3), version_backup_path(path, 2)]
            .into_iter()
            .find(|p| p.is_file()))
    }
}

pub(crate) fn backup_before_upgrade(path: &Path) -> Result<()> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let value: Value = serde_json::from_slice(&bytes)?;
    let version = value["version"].as_u64().unwrap_or(0);
    if version >= u64::from(crate::STATE_VERSION) {
        return Ok(());
    }
    let backup = version_backup_path(path, version);
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("ledger has no parent")?)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&backup) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            // Never overwrite the original backup, including on an interrupted migration.
            serde_json::from_slice::<Value>(&fs::read(&backup)?)
                .context("migration backup is damaged")?;
        }
        Err(e) => return Err(e.error.into()),
    }
    Ok(())
}

pub(crate) fn initialize(state: &mut State) {
    let migrating = !state.billing.initialized
        && (!state.entries.is_empty()
            || !state.reports.is_empty()
            || state
                .tasks
                .iter()
                .any(|t| t.running || t.legacy_seconds > 0));
    for project in &state.projects {
        state
            .billing
            .projects
            .entry(project.id.clone())
            .or_insert_with(|| {
                let cadence = if !project.export_monthly && project.export_weekly {
                    "weekly"
                } else if !project.export_monthly && !project.export_weekly {
                    "manual"
                } else {
                    "monthly"
                };
                ProjectBilling {
                    cadence: cadence.into(),
                    rates: vec![RatePoint {
                        effective_at: if migrating { now_ms() } else { 1 },
                        rate: project.rate.clone(),
                    }],
                    ..Default::default()
                }
            });
    }
    for entry in &state.entries {
        state.billing.entries.entry(entry.id.clone()).or_default();
    }
    state.billing.initialized = true;
}

pub(crate) fn record_rate(
    state: &mut State,
    project: &str,
    at: i64,
    rate: Option<HourlyRate>,
) -> Result<()> {
    if at <= 0 || at > now_ms() {
        bail!("INVALID_INPUT: rate effective time must be in the past or present")
    }
    let settings = state
        .billing
        .projects
        .get_mut(project)
        .context("PROJECT_NOT_FOUND")?;
    settings.rates.retain(|p| p.effective_at != at);
    settings.rates.push(RatePoint {
        effective_at: at,
        rate,
    });
    settings.rates.sort_by_key(|p| p.effective_at);
    Ok(())
}

pub(crate) fn at_rate(state: &State, project: &str, at: i64) -> EntryBilling {
    state
        .billing
        .projects
        .get(project)
        .and_then(|p| p.rates.iter().rev().find(|p| p.effective_at <= at))
        .map(|p| EntryBilling {
            resolved: true,
            rate: p.rate.clone(),
            ..Default::default()
        })
        .unwrap_or_default()
}

/// Cumulative allocation prevents rounding from charging a boundary second twice.
pub fn entry_seconds(entry: &Entry, from: i64, to: i64) -> i64 {
    let span = entry.ended_at - entry.started_at;
    if span <= 0 {
        return 0;
    }
    let cumulative = |at: i64| {
        i128::from((at - entry.started_at).clamp(0, span)) * i128::from(entry.seconds)
            / i128::from(span)
    };
    (cumulative(to) - cumulative(from)).max(0) as i64
}

pub(crate) fn record_entry(state: &mut State, entry: Entry) {
    let mut boundaries = vec![entry.started_at, entry.ended_at];
    if let Some(p) = state.billing.projects.get(&entry.project_id) {
        boundaries.extend(
            p.rates
                .iter()
                .map(|r| r.effective_at)
                .filter(|at| *at > entry.started_at && *at < entry.ended_at),
        );
    }
    if let Some(points) = state.billing.task_rates.get(&entry.task_id) {
        boundaries.extend(
            points
                .iter()
                .map(|p| p.effective_at)
                .filter(|at| *at > entry.started_at && *at < entry.ended_at),
        );
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    for pair in boundaries.windows(2) {
        let mut segment = entry.clone();
        segment.id = crate::make_id("entry");
        segment.started_at = pair[0];
        segment.ended_at = pair[1];
        segment.seconds = entry_seconds(&entry, pair[0], pair[1]);
        if segment.seconds == 0 {
            continue;
        }
        state.billing.entries.insert(
            segment.id.clone(),
            crate::task_rates::at(state, &segment.task_id, &segment.project_id, pair[0]),
        );
        state.entries.push(segment);
    }
}

pub fn bounds(from: &str, to: &str, timezone: &str) -> Result<(i64, i64)> {
    let tz: Tz = timezone
        .parse()
        .context("INVALID_INPUT: use an IANA timezone, such as Europe/London")?;
    let parse = |text: &str| -> Result<i64> {
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .context("INVALID_INPUT: date must be YYYY-MM-DD")?;
        tz.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .map(|d| d.timestamp_millis())
            .context("INVALID_INPUT: ambiguous or nonexistent local midnight")
    };
    let start = parse(from)?;
    let end = parse(to)?;
    if start <= 0 || end <= start {
        bail!("INVALID_INPUT: date range must be positive and --to is exclusive")
    }
    Ok((start, end))
}

pub fn previous_period(cadence: &str, timezone: &str) -> Result<(String, String)> {
    let tz: Tz = timezone.parse().context("INVALID_INPUT: timezone")?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let (start, end) = match cadence {
        "weekly" => {
            let end = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
            (end - Duration::days(7), end)
        }
        "monthly" => {
            let end = today.with_day(1).unwrap();
            ((end - Duration::days(1)).with_day(1).unwrap(), end)
        }
        _ => bail!("INVALID_INPUT: period must be weekly or monthly"),
    };
    Ok((start.to_string(), end.to_string()))
}

pub fn timestamp(text: &str) -> Result<i64> {
    let at = chrono::DateTime::parse_from_rfc3339(text)
        .context("INVALID_INPUT: use RFC3339 with timezone offset")?
        .timestamp_millis();
    if at <= 0 || at > now_ms() {
        bail!("INVALID_INPUT: timestamp must be in the past or present")
    }
    Ok(at)
}

pub fn settings(state: &State, project: &str) -> Result<ProjectBilling> {
    if !state.projects.iter().any(|p| p.id == project) {
        bail!("PROJECT_NOT_FOUND: {project}")
    }
    Ok(state
        .billing
        .projects
        .get(project)
        .cloned()
        .unwrap_or_default())
}

fn occupied(state: &State, entry: &str) -> Vec<(i64, i64)> {
    let mut ranges: Vec<_> = state
        .billing
        .invoices
        .iter()
        .filter(|i| i.state == "issued" || i.state == "paid")
        .flat_map(|i| &i.allocations)
        .filter(|a| a.entry_id == entry)
        .map(|a| (a.start_at, a.end_at))
        .collect();
    ranges.sort_unstable();
    ranges
}

fn remaining(mut ranges: Vec<(i64, i64)>, occupied: &[(i64, i64)]) -> Vec<(i64, i64)> {
    for &(a, b) in occupied {
        ranges = ranges
            .into_iter()
            .flat_map(|(s, e)| {
                let mut out = Vec::new();
                if b <= s || a >= e {
                    out.push((s, e));
                } else {
                    if s < a {
                        out.push((s, a));
                    }
                    if e > b {
                        out.push((b, e));
                    }
                }
                out
            })
            .collect();
    }
    ranges
}

pub fn allocations(
    state: &State,
    project: &str,
    from: i64,
    to: i64,
    currency: Option<&str>,
) -> (Vec<Allocation>, Excluded) {
    let mut out = Vec::new();
    let mut excluded = Excluded::default();
    for e in state.entries.iter().filter(|e| e.project_id == project) {
        let seconds = entry_seconds(e, from, to);
        if seconds == 0 {
            continue;
        }
        let meta = state
            .billing
            .entries
            .get(&e.id)
            .cloned()
            .unwrap_or_default();
        if !meta.resolved {
            excluded.unresolved_seconds += seconds;
            continue;
        }
        let Some(rate) = meta.rate else {
            excluded.non_billable_seconds += seconds;
            continue;
        };
        if currency.is_some_and(|c| c != rate.currency()) {
            continue;
        }
        if meta.externally_billed {
            excluded.already_billed_seconds += seconds;
            continue;
        }
        let ranges = remaining(
            vec![(e.started_at.max(from), e.ended_at.min(to))],
            &occupied(state, &e.id),
        );
        let mut available = 0;
        for (start, end) in ranges {
            let seconds = entry_seconds(e, start, end);
            if seconds == 0 {
                continue;
            }
            available += seconds;
            out.push(Allocation {
                entry_id: e.id.clone(),
                revision: meta.revision,
                start_at: start,
                end_at: end,
                seconds,
                task: e.task_title.clone(),
                note: e.note.clone(),
                rate: rate.clone(),
            });
        }
        excluded.already_billed_seconds += seconds - available;
    }
    excluded.running_timers = state
        .tasks
        .iter()
        .filter(|t| t.project_id == project && t.running && t.started_at < to && now_ms() > from)
        .count();
    out.sort_by(|a, b| (a.start_at, &a.entry_id).cmp(&(b.start_at, &b.entry_id)));
    (out, excluded)
}

fn lines(allocations: &[Allocation]) -> Result<Vec<InvoiceLine>> {
    let mut grouped: BTreeMap<String, (String, HourlyRate, i64)> = BTreeMap::new();
    for a in allocations {
        let key = serde_json::to_string(&(&a.task, &a.rate))?;
        let line = grouped
            .entry(key)
            .or_insert((a.task.clone(), a.rate.clone(), 0));
        line.2 = line
            .2
            .checked_add(a.seconds)
            .context("INVALID_INPUT: duration overflow")?;
    }
    Ok(grouped
        .into_values()
        .map(|(task, rate, seconds)| {
            let estimate = rate.estimate(seconds);
            InvoiceLine {
                task,
                seconds,
                duration: crate::format_duration(seconds),
                rate,
                hourly_rate: estimate.hourly_rate,
                amount_minor: estimate.amount_minor,
                amount_text: estimate.amount_text,
            }
        })
        .collect())
}

pub fn money(minor: i128, currency: &str) -> Result<String> {
    let digits = HourlyRate::parse("0", currency)?
        .estimate(0)
        .fraction_digits;
    let scale = 10_i128.pow(digits as u32);
    Ok(if digits == 0 {
        format!("{currency} {minor}")
    } else {
        format!("{currency} {}.{:0digits$}", minor / scale, minor % scale)
    })
}

pub fn draft(
    state: &State,
    project_id: &str,
    from: &str,
    to: &str,
    currency: &str,
) -> Result<Invoice> {
    let config = settings(state, project_id)?;
    let project = state
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .context("PROJECT_NOT_FOUND")?;
    let (start_at, end_at) = bounds(from, to, &config.timezone)?;
    HourlyRate::parse("0", currency)?;
    let (allocations, excluded) = allocations(state, project_id, start_at, end_at, Some(currency));
    let lines = lines(&allocations)?;
    let total: i128 = lines
        .iter()
        .map(|l| l.amount_minor.parse::<i128>().unwrap())
        .sum();
    let client = state
        .billing
        .clients
        .get(&config.client_id)
        .cloned()
        .unwrap_or_else(|| Party {
            name: project.client_name.clone(),
            ..Default::default()
        });
    let mut issuer = state.billing.issuer.clone();
    if issuer.name.is_empty() {
        issuer.name = project.company_name.clone();
    }
    Ok(Invoice {
        id: crate::make_id("invoice"),
        project_id: project_id.into(),
        project: serde_json::to_value(project)?,
        revision: 1,
        state: "draft".into(),
        number: String::new(),
        currency: currency.into(),
        from: from.into(),
        to: to.into(),
        timezone: config.timezone,
        start_at,
        end_at,
        created_at: now_ms(),
        issue_date: String::new(),
        due_date: String::new(),
        paid_at: None,
        due_days: config.due_days,
        payment_instructions: issuer.payment_instructions.clone(),
        issuer,
        client,
        total_seconds: allocations
            .iter()
            .try_fold(0_i64, |sum, a| sum.checked_add(a.seconds))
            .context("INVALID_INPUT: duration overflow")?,
        allocations,
        excluded,
        lines,
        total_minor: total.to_string(),
        total_text: money(total, currency)?,
        template_id: config.template_id,
        bundle: String::new(),
        pdf_path: String::new(),
        render_status: "pending".into(),
        upload_status: "pending".into(),
        remote_path: String::new(),
        last_error: String::new(),
        replaces: None,
        void_reason: String::new(),
    })
}

pub fn invoice(state: &State, id: &str) -> Result<Invoice> {
    state
        .billing
        .invoices
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .context("INVOICE_NOT_FOUND")
}

pub fn check_revision(invoice: &Invoice, revision: u64) -> Result<()> {
    if invoice.revision != revision {
        bail!("REVISION_CONFLICT: inspect invoice and retry with its current revision")
    }
    Ok(())
}

pub fn artifact_root(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Per-ledger sibling storage also makes alternate ledgers and backups self-contained.
    Ok(PathBuf::from(format!("{}.invoices", absolute.display())))
}

pub fn template_data(invoice: &Invoice) -> Result<Value> {
    Ok(
        json!({ "schemaVersion": 1, "invoice": invoice, "project": invoice.project,
        "issuer": invoice.issuer, "client": invoice.client, "lines": invoice.lines,
        "totalSeconds": invoice.total_seconds, "totalDuration": crate::format_duration(invoice.total_seconds),
        "period": { "start": invoice.from, "end": invoice.to, "label": format!("{} — {} (exclusive)", invoice.from, invoice.to) },
        "generatedAt": invoice.created_at }),
    )
}

pub fn issue(
    state: &mut State,
    path: &Path,
    id: &str,
    revision: u64,
    date: &str,
) -> Result<Invoice> {
    let mut inv = invoice(state, id)?;
    check_revision(&inv, revision)?;
    if inv.state != "draft" {
        bail!("INVALID_STATE: only drafts can be issued")
    }
    let fresh = draft(state, &inv.project_id, &inv.from, &inv.to, &inv.currency)?;
    if fresh.allocations != inv.allocations
        || fresh.project != inv.project
        || fresh.client != inv.client
        || fresh.issuer != inv.issuer
        || fresh.template_id != inv.template_id
        || fresh.due_days != inv.due_days
        || fresh.start_at != inv.start_at
        || fresh.end_at != inv.end_at
    {
        bail!("STALE_DRAFT: source data changed; refresh and preview the draft")
    }
    if inv.allocations.is_empty() {
        bail!("EMPTY_INVOICE: no uninvoiced billable time")
    }
    if inv.client.name.trim().is_empty() || inv.issuer.name.trim().is_empty() {
        bail!("MISSING_BILLING_DETAILS: set issuer and client names before issuing")
    }
    let issued =
        NaiveDate::parse_from_str(date, "%Y-%m-%d").context("INVALID_INPUT: invalid issue date")?;
    let due = issued
        .checked_add_signed(Duration::days(i64::from(inv.due_days)))
        .context("INVALID_INPUT: due date overflow")?;
    let sequence = state
        .billing
        .sequences
        .entry(issued.year().to_string())
        .or_default();
    *sequence += 1;
    inv.number = format!("INV-{}-{:05}", issued.year(), sequence);
    inv.issue_date = date.into();
    inv.due_date = due.to_string();
    inv.state = "issued".into();
    inv.revision += 1;
    let root = artifact_root(path)?.join(&inv.id);
    fs::create_dir_all(&root)?;
    // A unique generation avoids an orphaned capture blocking recovery after a crash.
    let bundle = root.join(crate::make_id("bundle"));
    inv.bundle = bundle.display().to_string();
    inv.pdf_path = root.join("invoice.pdf").display().to_string();
    crate::templates::capture(&inv.template_id, &template_data(&inv)?, &bundle)?;
    *state
        .billing
        .invoices
        .iter_mut()
        .find(|i| i.id == id)
        .unwrap() = inv.clone();
    Ok(inv)
}

pub fn correct(
    state: &mut State,
    id: &str,
    revision: u64,
    delta: i64,
    reason: &str,
    reverses: Option<String>,
) -> Result<Correction> {
    if reason.trim().is_empty() || delta == 0 {
        bail!("INVALID_INPUT: supply a nonzero adjustment and a reason")
    }
    if !occupied(state, id).is_empty() {
        bail!("ENTRY_INVOICED: void the invoice before correcting its time")
    }
    let meta = state
        .billing
        .entries
        .get_mut(id)
        .context("ENTRY_NOT_FOUND")?;
    if meta.externally_billed {
        bail!("ENTRY_INVOICED: entry is marked externally billed")
    }
    if meta.revision != revision {
        bail!("REVISION_CONFLICT: inspect entry before correcting")
    }
    let e = state
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .context("ENTRY_NOT_FOUND")?;
    let after = e
        .seconds
        .checked_add(delta)
        .filter(|s| *s >= 0)
        .context("INVALID_INPUT: adjustment would make duration negative or overflow")?;
    let correction = Correction {
        id: crate::make_id("correction"),
        entry_id: id.into(),
        before: e.seconds,
        after,
        reason: reason.into(),
        created_at: now_ms(),
        reverses,
    };
    e.seconds = after;
    meta.revision += 1;
    state.billing.corrections.push(correction.clone());
    Ok(correction)
}

pub fn schedule(state: &mut State, now: i64) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for project in state.projects.clone() {
        if state.billing.archived_projects.contains(&project.id) {
            continue;
        }
        let config = settings(state, &project.id)?;
        if config.cadence == "manual" {
            continue;
        }
        let tz: Tz = config
            .timezone
            .parse()
            .context("INVALID_INPUT: project timezone")?;
        let local_now = Utc
            .timestamp_millis_opt(now)
            .single()
            .context("INVALID_INPUT: timestamp")?
            .with_timezone(&tz)
            .date_naive();
        let mut periods = BTreeMap::new();
        for entry in state.entries.iter().filter(|e| e.project_id == project.id) {
            let Some(meta) = state.billing.entries.get(&entry.id) else {
                continue;
            };
            let Some(rate) = &meta.rate else {
                continue;
            };
            if !meta.resolved || meta.externally_billed {
                continue;
            }
            let local = Utc
                .timestamp_millis_opt(entry.started_at)
                .single()
                .context("invalid entry date")?
                .with_timezone(&tz)
                .date_naive();
            let mut start = if config.cadence == "weekly" {
                local - Duration::days(i64::from(local.weekday().num_days_from_monday()))
            } else {
                local.with_day(1).unwrap()
            };
            loop {
                let end = if config.cadence == "weekly" {
                    start + Duration::days(7)
                } else {
                    (start + Duration::days(32)).with_day(1).unwrap()
                };
                let (s, e) = bounds(&start.to_string(), &end.to_string(), &config.timezone)?;
                if s >= entry.ended_at || end > local_now {
                    break;
                }
                if entry_seconds(entry, s, e) > 0 {
                    periods.insert(
                        (
                            start.to_string(),
                            end.to_string(),
                            rate.currency().to_owned(),
                        ),
                        (s, e),
                    );
                }
                start = end;
            }
        }
        for ((from, to, currency), _) in periods {
            let new = draft(state, &project.id, &from, &to, &currency)?;
            if new.allocations.is_empty() {
                continue;
            }
            if let Some(old) = state.billing.invoices.iter().find(|i| {
                i.project_id == project.id
                    && i.from == from
                    && i.to == to
                    && i.currency == currency
                    && i.state == "draft"
            }) {
                // Do not overwrite a reviewed draft; refresh is explicit when late entries arrive.
                let _ = old;
                continue;
            }
            created.push(new.id.clone());
            state.billing.invoices.push(new);
        }
    }
    Ok(created)
}

pub fn check_scheduled(path: &Path) -> Result<Vec<String>> {
    crate::mutate_state(path, |state| {
        let ids = schedule(state, now_ms())?;
        Ok(if ids.is_empty() {
            crate::Mutation::Unchanged(ids)
        } else {
            crate::Mutation::Changed(ids)
        })
    })
}

/// External work runs outside the ledger lock. A separate worker lock serializes artifacts.
pub fn render(path: &Path, id: &str, preview: bool) -> Result<Value> {
    let _worker = crate::lock_file(&PathBuf::from(format!(
        "{}.invoices-worker",
        path.display()
    )))?;
    let inv = invoice(&crate::locked_state(path)?, id)?;
    if preview {
        if inv.state != "draft" {
            bail!("INVALID_STATE: use invoice.render for the captured issued document")
        }
        let root = artifact_root(path)?.join("previews");
        fs::create_dir_all(&root)?;
        let temp = tempfile::tempdir_in(root)?;
        crate::templates::capture(
            &inv.template_id,
            &template_data(&inv)?,
            &temp.path().join("bundle"),
        )?;
        let pdf = temp.path().join("preview.pdf");
        crate::templates::compile(&temp.path().join("bundle"), &pdf)?;
        let _ = temp.keep();
        return Ok(json!({"id": id, "path": pdf, "draft": inv.state == "draft"}));
    }
    if inv.state == "draft" {
        bail!("INVALID_STATE: issue invoice or use preview")
    }
    let pdf = PathBuf::from(&inv.pdf_path);
    let result = if inv.render_status == "complete" && pdf.is_file() {
        Ok(())
    } else {
        let temporary = pdf.with_extension("pending.pdf");
        crate::templates::compile(Path::new(&inv.bundle), &temporary)
            .and_then(|()| Ok(fs::rename(temporary, &pdf)?))
    };
    crate::mutate_state(path, |state| {
        let invoice = state
            .billing
            .invoices
            .iter_mut()
            .find(|i| i.id == id)
            .context("INVOICE_NOT_FOUND")?;
        invoice.render_status = if result.is_ok() { "complete" } else { "failed" }.into();
        invoice.last_error = result
            .as_ref()
            .err()
            .map(|e| format!("{e:#}"))
            .unwrap_or_default();
        Ok(crate::Mutation::Changed(()))
    })?;
    result?;
    Ok(json!({"id": id, "path": pdf, "renderStatus": "complete"}))
}

pub fn upload(path: &Path, id: &str) -> Result<Value> {
    render(path, id, false)?;
    let _worker = crate::lock_file(&PathBuf::from(format!(
        "{}.invoices-worker",
        path.display()
    )))?;
    let state = crate::locked_state(path)?;
    let inv = invoice(&state, id)?;
    if inv.state == "void" {
        bail!("INVALID_STATE: cannot upload a void invoice")
    }
    let mut drive = state.drive.clone();
    let config = settings(&state, &inv.project_id)?;
    if !config.drive_folder.is_empty() {
        drive.folder = config.drive_folder;
    }
    if !crate::valid_remote(&drive.remote) {
        bail!("DRIVE_NOT_CONFIGURED: configure an rclone remote")
    }
    let destination = if inv.remote_path.is_empty() {
        crate::remote_path(
            &drive,
            &format!("invoices/{}/{}.pdf", inv.project_id, inv.number),
        )?
    } else {
        inv.remote_path.clone()
    };
    crate::mutate_state(path, |state| {
        let i = state
            .billing
            .invoices
            .iter_mut()
            .find(|i| i.id == id)
            .context("INVOICE_NOT_FOUND")?;
        i.remote_path = destination.clone();
        i.upload_status = "uploading".into();
        Ok(crate::Mutation::Changed(()))
    })?;
    let result = crate::run_command(
        "rclone",
        [
            "copyto".into(),
            "--checksum".into(),
            "--retries".into(),
            "3".into(),
            inv.pdf_path,
            destination.clone(),
        ],
    );
    crate::mutate_state(path, |state| {
        let i = state
            .billing
            .invoices
            .iter_mut()
            .find(|i| i.id == id)
            .context("INVOICE_NOT_FOUND")?;
        i.upload_status = if result.is_ok() { "complete" } else { "failed" }.into();
        i.last_error = result
            .as_ref()
            .err()
            .map(|e| format!("{e:#}"))
            .unwrap_or_default();
        Ok(crate::Mutation::Changed(()))
    })?;
    result?;
    Ok(json!({"id": id, "remotePath": destination, "uploadStatus": "complete"}))
}
