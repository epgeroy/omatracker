//! Versioned, noninteractive CLI surface. JSON requests are validated before mutation;
//! receipts and their ledger changes are committed together for retry safety.
use crate::{Mutation, State, billing as b};
use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const ACTIONS: &[&str] = &[
    "help",
    "request.key",
    "request.keys",
    "data.clear",
    "context",
    "issuer.get",
    "issuer.set",
    "client.list",
    "client.get",
    "client.set",
    "client.update",
    "client.remove",
    "client.delete",
    "project.list",
    "project.get",
    "project.create",
    "project.configure",
    "project.update",
    "project.remove",
    "project.delete",
    "project.rate",
    "task.list",
    "task.get",
    "task.create",
    "task.update",
    "task.rate",
    "task.remove",
    "task.delete",
    "task.start",
    "task.stop",
    "entry.list",
    "entry.add",
    "entry.correct",
    "entry.undo",
    "summary",
    "migration.preview",
    "migration.apply",
    "migration.resolve",
    "invoice.list",
    "invoice.get",
    "invoice.create",
    "invoice.period",
    "invoice.refresh",
    "invoice.issue",
    "invoice.paid",
    "invoice.void",
    "invoice.reissue",
    "invoice.check",
    "invoice.preview",
    "invoice.render",
    "invoice.upload",
    "template.list",
    "template.create",
    "template.asset",
    "template.validate",
    "artifact.open",
    "template.path",
    "doctor",
    "drive.configure",
    "drive.check",
    "drive.test",
    "repository.bind",
    "repository.resolve",
];

#[derive(Args)]
pub struct Cli {
    /// Operation name. Run `agent help` for the complete request contract.
    pub action: String,
    /// JSON request object. All date ranges have an exclusive `to` boundary.
    #[arg(long, default_value = "{}", conflicts_with = "input_file")]
    pub input: String,
    /// Read the request from a file, or `-` for stdin.
    #[arg(long)]
    pub input_file: Option<PathBuf>,
    /// Durable retry key, or "auto" for a new operation (printed before execution).
    #[arg(long)]
    pub key: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub path: Option<String>,
    pub id: Option<String>,
    pub project: Option<String>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub note: Option<String>,
    pub client: Option<String>,
    pub template: Option<String>,
    pub logo: Option<String>,
    pub accent_color: Option<String>,
    pub paper: Option<String>,
    pub rate: Option<String>,
    pub currency: Option<String>,
    pub no_rate: bool,
    pub effective_at: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub seconds: Option<i64>,
    pub delta: Option<i64>,
    pub reason: Option<String>,
    pub revision: Option<u64>,
    pub entity_revision: Option<String>,
    pub date: Option<String>,
    pub cadence: Option<String>,
    pub timezone: Option<String>,
    pub due_days: Option<u32>,
    pub drive_folder: Option<String>,
    pub copy_from: Option<String>,
    pub repository: Option<String>,
    pub externally_billed: bool,
    pub source: Option<String>,
    pub remote: Option<String>,
    pub sync_on_startup: Option<bool>,
    pub details: Option<b::Party>,
    pub offset: usize,
    pub limit: Option<usize>,
    pub running: bool,
    pub include_archived: bool,
    pub inherit_rate: bool,
    pub apply_existing: bool,
    pub add: Option<String>,
    pub dry_run: bool,
    pub include_drive: bool,
}

const MAX_REQUEST_KEYS: usize = 64;
const MAX_KEY_LABEL_BYTES: usize = 80;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyBatch {
    labels: Vec<String>,
}

fn request_keys(input: Value, key: Option<&str>) -> Result<Value> {
    if key.is_some() {
        bail!(
            "INVALID_INPUT: request.keys does not accept a retry key; it generates fresh keys without receipts"
        )
    }
    let batch: KeyBatch =
        serde_json::from_value(input).context("INVALID_INPUT: expected an object with labels")?;
    if batch.labels.is_empty() || batch.labels.len() > MAX_REQUEST_KEYS {
        bail!("INVALID_INPUT: labels must contain 1–{MAX_REQUEST_KEYS} items")
    }
    let mut seen = std::collections::HashSet::new();
    for label in &batch.labels {
        if label.is_empty()
            || label.len() > MAX_KEY_LABEL_BYTES
            || label.trim() != label
            || label.chars().any(char::is_control)
        {
            bail!(
                "INVALID_INPUT: labels must be 1–{MAX_KEY_LABEL_BYTES} UTF-8 bytes without surrounding whitespace or control characters"
            )
        }
        if !seen.insert(label) {
            bail!("INVALID_INPUT: labels must be unique")
        }
    }
    // Validate the whole batch before generation. This path never touches a ledger.
    let keys: serde_json::Map<String, Value> = batch
        .labels
        .into_iter()
        .map(|label| (label, json!(crate::make_id("request"))))
        .collect();
    Ok(json!({"schemaVersion":1,"ok":true,"changed":false,"data":{"keys":keys}}))
}

fn required(value: &Option<String>, name: &str) -> Result<String> {
    value
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .cloned()
        .with_context(|| format!("INVALID_INPUT: {name} is required"))
}

fn entity_name(value: &Option<String>, field: &str, limit: usize) -> Result<String> {
    let name = crate::sanitize_text(required(value, field)?, limit);
    if name.is_empty() {
        bail!("INVALID_INPUT: {field} cannot be blank")
    }
    Ok(name)
}

fn task_name(i: &Input) -> Result<String> {
    if let (Some(name), Some(title)) = (&i.name, &i.title)
        && crate::sanitize_text(name, 160) != crate::sanitize_text(title, 160)
    {
        bail!("INVALID_INPUT: name and title must agree when both are supplied")
    }
    entity_name(
        &i.title.clone().or_else(|| i.name.clone()),
        "name or title",
        160,
    )
}
fn revision(i: &Input) -> Result<u64> {
    i.revision.context("INVALID_INPUT: revision is required")
}
fn page<T: Serialize>(items: &[T], i: &Input) -> Value {
    let limit = i.limit.unwrap_or(50).clamp(1, 200);
    json!({"items": items.iter().skip(i.offset).take(limit).collect::<Vec<_>>(), "total": items.len(),
        "offset": i.offset, "nextOffset": (i.offset + limit < items.len()).then_some(i.offset + limit)})
}
fn project_id(state: &State, i: &Input) -> Result<String> {
    if let Some(id) = &i.project {
        b::settings(state, id)?;
        return Ok(id.clone());
    }
    if let Some(repo) = &i.repository {
        let path =
            std::fs::canonicalize(repo).context("INVALID_INPUT: repository path does not exist")?;
        let id = state
            .billing
            .bindings
            .get(&path.display().to_string())
            .context("PROJECT_REQUIRED: repository is not bound")?;
        b::settings(state, id)?;
        return Ok(id.clone());
    }
    bail!("PROJECT_REQUIRED: supply project or repository; UI selection is not used")
}

fn range(state: &State, i: &Input, project: &str) -> Result<(i64, i64)> {
    b::bounds(
        &required(&i.from, "from")?,
        &required(&i.to, "to")?,
        &b::settings(state, project)?.timezone,
    )
}

fn read(action: &str, state: &State, i: &Input) -> Result<Value> {
    Ok(match action {
        "context" => {
            json!({"projects": state.projects.iter().filter(|p| !state.billing.archived_projects.contains(&p.id)).collect::<Vec<_>>(), "runningTasks": state.tasks.iter().filter(|t| t.running).collect::<Vec<_>>(),
            "drafts": state.billing.invoices.iter().filter(|v| v.state == "draft").map(|v| json!({"id":v.id,"project":v.project_id,"total":v.total_text})).collect::<Vec<_>>(),
            "revision": state.billing.revision})
        }
        "issuer.get" => json!(state.billing.issuer),
        "client.list" => page(
            &state
                .billing
                .clients
                .iter()
                .filter(|(id, _)| i.include_archived || !state.billing.archived_clients.contains(*id))
                .map(|(id, details)| json!({"id":id,"details":details,"archived":state.billing.archived_clients.contains(id),"entityRevision":crate::entities::revision(state,"client",id).expect("listed client exists")}))
                .collect::<Vec<_>>(),
            i,
        ),
        "client.get" => {
            let id = required(&i.id, "id")?;
            let client = state.billing.clients.get(&id).context("CLIENT_NOT_FOUND")?;
            json!({"id":id,"details":client,"archived":state.billing.archived_clients.contains(&id),"entityRevision":crate::entities::revision(state,"client",&id)?})
        }
        "project.list" => page(&state.projects.iter()
            .filter(|p| i.include_archived || !state.billing.archived_projects.contains(&p.id))
            .map(|p| { let mut item = json!(p); item["archived"] = json!(state.billing.archived_projects.contains(&p.id)); item["protected"] = json!(p.id == crate::DEFAULT_PROJECT_ID); item["entityRevision"] = json!(crate::entities::revision(state,"project",&p.id).expect("listed project exists")); item })
            .collect::<Vec<_>>(), i),
        "project.get" => {
            let id = project_id(state, i)?;
            json!({"project":state.projects.iter().find(|p| p.id == id), "billing":b::settings(state,&id)?, "revision":state.billing.revision,"archived":state.billing.archived_projects.contains(&id),"protected":id == crate::DEFAULT_PROJECT_ID,"entityRevision":crate::entities::revision(state,"project",&id)?})
        }
        "task.get" => crate::task_rates::task_json(state,state.tasks.iter().find(|t| Some(&t.id) == i.id.as_ref()).context("TASK_NOT_FOUND")?),
        "task.list" => {
            let project = project_id(state, i)?;
            page(
                &state
                    .tasks
                    .iter()
                    .filter(|t| t.project_id == project && (!i.running || t.running))
                    .map(|t| crate::task_rates::task_json(state,t))
                    .collect::<Vec<_>>(),
                i,
            )
        }
        "entry.list" => {
            let project = project_id(state, i)?;
            let (start, end) = if i.from.is_some() || i.to.is_some() {
                range(state, i, &project)?
            } else {
                (1, i64::MAX)
            };
            page(&state.entries.iter().filter(|e| e.project_id == project && e.ended_at > start && e.started_at < end
                && i.id.as_ref().is_none_or(|id| &e.id == id)).map(|e| json!({"entry":e,
                    "billing":state.billing.entries.get(&e.id).cloned().unwrap_or_default(),
                    "corrections":state.billing.corrections.iter().filter(|c| c.entry_id == e.id).collect::<Vec<_>>(),
                    "rateAdjustments":state.billing.task_rate_adjustments.iter().filter(|a| a.previous_billing.contains_key(&e.id))
                        .map(|a| json!({"id":a.id,"taskId":a.task_id,"createdAt":a.created_at,"rate":a.rate,"reason":a.reason,"previousBilling":a.previous_billing[&e.id]})).collect::<Vec<_>>()
                })).collect::<Vec<_>>(),i)
        }
        "summary" => {
            let project = project_id(state, i)?;
            let (start, end) = range(state, i, &project)?;
            let (allocations, excluded) = b::allocations(state, &project, start, end, None);
            let currencies: std::collections::BTreeSet<_> = allocations
                .iter()
                .map(|a| a.rate.currency().to_owned())
                .collect();
            let mut amounts = Vec::new();
            for currency in currencies {
                let draft = b::draft(
                    state,
                    &project,
                    i.from.as_ref().unwrap(),
                    i.to.as_ref().unwrap(),
                    &currency,
                )?;
                amounts.push(json!({"currency":currency,"seconds":draft.total_seconds,"amountMinor":draft.total_minor,"amountText":draft.total_text}));
            }
            let mut tasks = std::collections::BTreeMap::<String, i64>::new();
            for e in state.entries.iter().filter(|e| e.project_id == project) {
                let total = tasks.entry(e.task_title.clone()).or_default();
                *total = total
                    .checked_add(b::entry_seconds(e, start, end))
                    .context("INVALID_INPUT: duration overflow")?;
            }
            let recorded = tasks
                .values()
                .try_fold(0_i64, |sum, seconds| sum.checked_add(*seconds))
                .context("INVALID_INPUT: duration overflow")?;
            json!({"project":project,"from":i.from,"to":i.to,"recordedSeconds":recorded,
                "tasks":tasks,"uninvoiced":amounts,"excluded":excluded})
        }
        "invoice.list" => page(
            &state
                .billing
                .invoices
                .iter()
                .filter(|v| i.project.as_ref().is_none_or(|p| p == &v.project_id))
                .collect::<Vec<_>>(),
            i,
        ),
        "invoice.get" => json!(b::invoice(state, &required(&i.id, "id")?)?),
        "migration.preview" => json!({"targetVersion":crate::STATE_VERSION,
            "unresolvedEntries":state.entries.iter().filter(|e| !state.billing.entries.get(&e.id).is_some_and(|m| m.resolved)).count(),
            "undatedSeconds":state.tasks.iter().map(|t| t.legacy_seconds).sum::<i64>(),
            "archivedReports":state.reports.len(),"projects":state.billing.projects,"log":state.billing.migration_log,
            "policy":"Assign historical rates/noRate and externallyBilled explicitly. Archived reports are never invoices."}),
        "repository.resolve" => json!({"project":project_id(state,i)?}),
        _ => bail!("UNKNOWN_ACTION: {action}"),
    })
}

const READS: &[&str] = &[
    "context",
    "issuer.get",
    "client.list",
    "client.get",
    "project.list",
    "project.get",
    "task.list",
    "task.get",
    "entry.list",
    "summary",
    "invoice.list",
    "invoice.get",
    "migration.preview",
    "repository.resolve",
];

fn mutate(action: &str, state: &mut State, path: &Path, i: &Input) -> Result<Value> {
    if let Some(expected) = &i.entity_revision {
        if i.revision.is_some() {
            bail!("INVALID_INPUT: use entityRevision or revision, not both")
        }
        let (kind, id) = match action {
            "project.configure" | "project.update" | "project.rate" | "project.remove"
            | "project.delete" => ("project", project_id(state, i)?),
            "client.set" | "client.update" | "client.remove" | "client.delete" => {
                ("client", required(&i.id, "id")?)
            }
            "task.update" | "task.rate" | "task.remove" | "task.delete" | "task.start"
            | "task.stop" => ("task", required(&i.id, "id")?),
            _ => bail!(
                "INVALID_INPUT: entityRevision is only supported for task/project/client edits"
            ),
        };
        if expected != &crate::entities::revision(state, kind, &id)? {
            bail!(
                "REVISION_CONFLICT: {kind} {id} changed; reload {kind}.get and use its current entityRevision"
            )
        }
    }
    if let Some(expected) = i.revision
        && matches!(
            action,
            "project.configure"
                | "project.update"
                | "project.remove"
                | "project.delete"
                | "project.rate"
                | "issuer.set"
                | "client.set"
                | "client.update"
                | "client.remove"
                | "client.delete"
                | "task.update"
                | "task.rate"
                | "task.remove"
                | "task.delete"
                | "migration.resolve"
        )
        && expected != state.billing.revision
    {
        bail!(
            "REVISION_CONFLICT: ledger revision {expected} is now {}; fetch the entity again and prefer entityRevision for task/project/client edits so unrelated changes do not conflict",
            state.billing.revision
        )
    }
    Ok(match action {
        "migration.apply" => {
            if !state
                .billing
                .migration_log
                .iter()
                .any(|v| v["action"] == "upgrade" && v["targetVersion"] == crate::STATE_VERSION)
            {
                state
                    .billing
                    .migration_log
                    .push(json!({"at":crate::now_ms(),"action":"upgrade","targetVersion":crate::STATE_VERSION}));
            }
            json!({"version":crate::STATE_VERSION,"backup":b::upgrade_backup_path(path)?})
        }
        "issuer.set" => {
            state.billing.issuer = i
                .details
                .clone()
                .context("INVALID_INPUT: details required")?;
            json!(state.billing.issuer)
        }
        "client.set" => {
            let id = i.id.clone().unwrap_or_else(|| crate::make_id("client"));
            if state.billing.archived_clients.contains(&id) {
                bail!("CLIENT_ARCHIVED: {id}")
            }
            let mut details = i
                .details
                .clone()
                .context("INVALID_INPUT: details required")?;
            details.name = entity_name(&Some(details.name.clone()), "client name", 120)?;
            state.billing.clients.insert(id.clone(), details.clone());
            crate::entities::sync_client_name(state, &id);
            json!({"id":id,"details":details,"entityRevision":crate::entities::revision(state,"client",&id)?})
        }
        "client.update" => {
            let id = required(&i.id, "id")?;
            if state.billing.archived_clients.contains(&id) {
                bail!("CLIENT_ARCHIVED: {id}")
            }
            let name = entity_name(&i.name, "name", 120)?;
            state
                .billing
                .clients
                .get_mut(&id)
                .context("CLIENT_NOT_FOUND")?
                .name = name;
            crate::entities::sync_client_name(state, &id);
            read("client.get", state, i)?
        }
        "client.remove" | "client.delete" => {
            let id = required(&i.id, "id")?;
            let removed = crate::entities::archive_client(state, &id)?;
            json!({"id":id,"removed":removed,"archived":true})
        }
        "project.create" => {
            let mut project = if let Some(id) = &i.copy_from {
                state
                    .projects
                    .iter()
                    .find(|p| &p.id == id)
                    .cloned()
                    .context("PROJECT_NOT_FOUND")?
            } else {
                crate::default_project()
            };
            project.id = crate::make_id("project");
            project.name = required(&i.name, "name")?;
            let id = project.id.clone();
            let mut config = if let Some(id) = &i.copy_from {
                b::settings(state, id)?
            } else {
                b::ProjectBilling::default()
            };
            if state.billing.archived_clients.contains(&config.client_id) {
                config.client_id.clear();
                project.client_name.clear();
            }
            config.rates = vec![b::RatePoint {
                effective_at: 1,
                rate: project.rate.clone(),
            }];
            state.projects.push(project);
            state.billing.projects.insert(id.clone(), config);
            let mut input = i.clone();
            input.project = Some(id.clone());
            configure(state, &input)?;
            json!({"id":id,"project":state.projects.iter().find(|p| p.id == id),"billing":b::settings(state,&id)?,"entityRevision":crate::entities::revision(state,"project",&id)?})
        }
        "project.configure" | "project.update" => {
            configure(state, i)?;
            read("project.get", state, i)?
        }
        "project.remove" | "project.delete" => {
            let id = project_id(state, i)?;
            let removed = crate::entities::archive_project(state, &id)?;
            json!({"id":id,"removed":removed,"archived":true})
        }
        "project.rate" => {
            let id = project_id(state, i)?;
            crate::entities::active_project(state, &id)?;
            if i.no_rate == i.rate.is_some() {
                bail!("INVALID_INPUT: supply exactly one of rate or noRate")
            }
            let rate = if i.no_rate {
                None
            } else {
                Some(crate::HourlyRate::parse(
                    &required(&i.rate, "rate")?,
                    &required(&i.currency, "currency")?,
                )?)
            };
            let at = i
                .effective_at
                .as_deref()
                .map(b::timestamp)
                .transpose()?
                .unwrap_or_else(crate::now_ms);
            b::record_rate(state, &id, at, rate.clone())?;
            // Recorded entries retain their snapshots; only new/backdated entries use this history.
            state.projects.iter_mut().find(|p| p.id == id).unwrap().rate = state.billing.projects
                [&id]
                .rates
                .last()
                .unwrap()
                .rate
                .clone();
            read("project.get", state, i)?
        }
        "task.create" => {
            let project = project_id(state, i)?;
            crate::entities::active_project(state, &project)?;
            let task = crate::Task {
                id: crate::make_id("task"),
                project_id: project,
                title: task_name(i)?,
                ..Default::default()
            };
            state.tasks.push(task);
            crate::task_rates::task_json(state, state.tasks.last().unwrap())
        }
        "task.update" => {
            let id = required(&i.id, "id")?;
            let index = state
                .tasks
                .iter()
                .position(|t| t.id == id)
                .context("TASK_NOT_FOUND")?;
            crate::entities::active_project(state, &state.tasks[index].project_id)?;
            let rate_change = i.rate.is_some() || i.no_rate || i.inherit_rate;
            if i.name.is_none() && i.title.is_none() && i.add.is_none() && !rate_change {
                bail!("INVALID_INPUT: supply name/title, add, or a task rate setting")
            }
            if i.name.is_some() || i.title.is_some() {
                state.tasks[index].title = task_name(i)?;
            }
            if let Some(duration) = &i.add {
                let seconds = crate::parse_duration(duration)?;
                let now = crate::now_ms();
                let start = seconds
                    .checked_mul(1000)
                    .and_then(|ms| now.checked_sub(ms))
                    .filter(|t| *t > 0)
                    .context("INVALID_INPUT: added duration is too large")?;
                let task = state.tasks[index].clone();
                crate::append_entry(state, &task, start, now, seconds, "Manual entry");
            }
            if rate_change {
                crate::task_rates::assign(state, &id, i)?
            } else {
                if i.apply_existing || i.currency.is_some() || i.effective_at.is_some() {
                    bail!("INVALID_INPUT: rate options require rate, noRate, or inheritRate")
                }
                crate::task_rates::task_json(state, &state.tasks[index])
            }
        }
        "task.rate" => crate::task_rates::assign(state, &required(&i.id, "id")?, i)?,
        "task.remove" | "task.delete" => {
            let id = required(&i.id, "id")?;
            crate::entities::remove_task(state, &id)?;
            json!({"id":id,"removed":true})
        }
        "task.start" | "task.stop" => {
            let id = required(&i.id, "id")?;
            let index = state
                .tasks
                .iter()
                .position(|t| t.id == id)
                .context("TASK_NOT_FOUND")?;
            if action == "task.start" {
                crate::entities::active_project(state, &state.tasks[index].project_id)?;
            }
            if action == "task.start" && !state.tasks[index].running {
                state.tasks[index].running = true;
                state.tasks[index].started_at = crate::now_ms();
            } else if action == "task.stop" && state.tasks[index].running {
                let task = state.tasks[index].clone();
                let now = crate::now_ms();
                crate::append_entry(
                    state,
                    &task,
                    task.started_at,
                    now,
                    (now - task.started_at) / 1000,
                    "",
                );
                state.tasks[index].running = false;
                state.tasks[index].started_at = 0;
            }
            crate::task_rates::task_json(state, &state.tasks[index])
        }
        "entry.add" => {
            let task = state
                .tasks
                .iter()
                .find(|t| Some(&t.id) == i.id.as_ref())
                .cloned()
                .context("TASK_NOT_FOUND: id must identify a task")?;
            crate::entities::active_project(state, &task.project_id)?;
            let start = b::timestamp(&required(&i.start, "start")?)?;
            let end = if let Some(end) = &i.end {
                b::timestamp(end)?
            } else {
                start
                    .checked_add(
                        i.seconds
                            .context("INVALID_INPUT: seconds or end required")?
                            .checked_mul(1000)
                            .context("INVALID_INPUT: duration overflow")?,
                    )
                    .context("INVALID_INPUT: duration overflow")?
            };
            if end <= start || end > crate::now_ms() {
                bail!(
                    "INVALID_INPUT: entry must have a positive duration and end no later than now"
                )
            }
            let offset = state.entries.len();
            crate::append_entry(
                state,
                &task,
                start,
                end,
                (end - start) / 1000,
                i.note.as_deref().unwrap_or("Manual entry"),
            );
            json!({"entries":state.entries[offset..].iter().map(|e| json!({"entry":e,"billing":state.billing.entries[&e.id]})).collect::<Vec<_>>()})
        }
        "entry.correct" => json!(b::correct(
            state,
            &required(&i.id, "id")?,
            revision(i)?,
            i.delta
                .context("INVALID_INPUT: delta in seconds required")?,
            &required(&i.reason, "reason")?,
            None
        )?),
        "entry.undo" => {
            let id = required(&i.id, "id")?;
            let correction = state
                .billing
                .corrections
                .iter()
                .find(|c| c.id == id)
                .cloned()
                .context("CORRECTION_NOT_FOUND")?;
            if state
                .billing
                .corrections
                .iter()
                .any(|c| c.reverses.as_ref() == Some(&id))
            {
                bail!("INVALID_STATE: correction already reversed")
            }
            json!(b::correct(
                state,
                &correction.entry_id,
                revision(i)?,
                correction.before - correction.after,
                &required(&i.reason, "reason")?,
                Some(id)
            )?)
        }
        "migration.resolve" => {
            let project = project_id(state, i)?;
            let (start, end) = range(state, i, &project)?;
            if i.no_rate == i.rate.is_some() {
                bail!("INVALID_INPUT: supply exactly one of rate or noRate")
            }
            let rate = if i.no_rate {
                None
            } else {
                Some(crate::HourlyRate::parse(
                    &required(&i.rate, "rate")?,
                    &required(&i.currency, "currency")?,
                )?)
            };
            let mut ids = Vec::new();
            for e in state
                .entries
                .iter()
                .filter(|e| e.project_id == project && e.started_at >= start && e.ended_at <= end)
            {
                let meta = state.billing.entries.entry(e.id.clone()).or_default();
                if meta.resolved {
                    continue;
                }
                meta.resolved = true;
                meta.rate = rate.clone();
                meta.externally_billed = i.externally_billed;
                meta.revision += 1;
                ids.push(e.id.clone());
            }
            let log = json!({"at":crate::now_ms(),"project":project,"from":i.from,"to":i.to,"rate":rate,"externallyBilled":i.externally_billed,"entries":ids});
            if !ids.is_empty() {
                state.billing.migration_log.push(log.clone());
            }
            log
        }
        "invoice.create" => {
            let project = project_id(state, i)?;
            let inv = b::draft(
                state,
                &project,
                &required(&i.from, "from")?,
                &required(&i.to, "to")?,
                &required(&i.currency, "currency")?.to_ascii_uppercase(),
            )?;
            let out = json!(inv);
            state.billing.invoices.push(inv);
            out
        }
        "invoice.period" => {
            let project = project_id(state, i)?;
            let (from, to) = b::previous_period(
                &required(&i.cadence, "cadence")?,
                &b::settings(state, &project)?.timezone,
            )?;
            let (start, end) = b::bounds(&from, &to, &b::settings(state, &project)?.timezone)?;
            let (available, _) = b::allocations(state, &project, start, end, None);
            let currencies: std::collections::BTreeSet<_> = available
                .iter()
                .map(|a| a.rate.currency().to_owned())
                .collect();
            let mut invoices = Vec::new();
            for currency in currencies {
                if let Some(existing) = state.billing.invoices.iter().find(|v| {
                    v.project_id == project
                        && v.from == from
                        && v.to == to
                        && v.currency == currency
                        && v.state == "draft"
                }) {
                    invoices.push(existing.clone());
                    continue;
                }
                let inv = b::draft(state, &project, &from, &to, &currency)?;
                invoices.push(inv.clone());
                state.billing.invoices.push(inv);
            }
            json!({"invoices":invoices,"from":from,"to":to})
        }
        "invoice.refresh" => {
            let old = b::invoice(state, &required(&i.id, "id")?)?;
            b::check_revision(&old, revision(i)?)?;
            if old.state != "draft" {
                bail!("INVALID_STATE: only a draft can be refreshed")
            }
            let mut new = b::draft(state, &old.project_id, &old.from, &old.to, &old.currency)?;
            new.id = old.id.clone();
            new.created_at = old.created_at;
            new.revision = old.revision + 1;
            new.replaces = old.replaces;
            let out = json!(new);
            *state
                .billing
                .invoices
                .iter_mut()
                .find(|v| v.id == old.id)
                .unwrap() = new;
            out
        }
        "invoice.issue" => json!(b::issue(
            state,
            path,
            &required(&i.id, "id")?,
            revision(i)?,
            &required(&i.date, "date")?
        )?),
        "invoice.paid" | "invoice.void" => {
            let id = required(&i.id, "id")?;
            let inv = state
                .billing
                .invoices
                .iter_mut()
                .find(|v| v.id == id)
                .context("INVOICE_NOT_FOUND")?;
            b::check_revision(inv, revision(i)?)?;
            if action == "invoice.paid" {
                if inv.state != "issued" {
                    bail!("INVALID_STATE: only an issued invoice can be marked paid")
                }
                let date = required(&i.date, "date")?;
                chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .context("INVALID_INPUT: invalid payment date")?;
                inv.state = "paid".into();
                inv.paid_at = Some(date);
            } else {
                if inv.state == "void" {
                    bail!("INVALID_STATE: invoice is already void")
                }
                inv.void_reason = required(&i.reason, "reason")?;
                inv.state = "void".into();
            }
            inv.revision += 1;
            json!(inv)
        }
        "invoice.reissue" => {
            let old = b::invoice(state, &required(&i.id, "id")?)?;
            b::check_revision(&old, revision(i)?)?;
            if old.state != "void" {
                bail!("INVALID_STATE: void the original before preparing a replacement")
            }
            let mut new = b::draft(state, &old.project_id, &old.from, &old.to, &old.currency)?;
            new.replaces = Some(old.id);
            let out = json!(new);
            state.billing.invoices.push(new);
            out
        }
        "invoice.check" => json!({"created":b::schedule(state,crate::now_ms())?}),
        "repository.bind" => {
            let project = required(&i.project, "project")?;
            b::settings(state, &project)?;
            let repo = std::fs::canonicalize(required(&i.repository, "repository")?)
                .context("INVALID_INPUT: repository path does not exist")?;
            if !repo.is_dir() {
                bail!("INVALID_INPUT: repository must be a directory")
            }
            state
                .billing
                .bindings
                .insert(repo.display().to_string(), project.clone());
            json!({"repository":repo,"project":project})
        }
        "drive.configure" => {
            let remote = required(&i.remote, "remote")?;
            if !crate::valid_remote(&remote) {
                bail!("INVALID_INPUT: invalid rclone remote name")
            }
            state.drive.remote = remote;
            if let Some(folder) = &i.drive_folder {
                state.drive.folder = folder.clone();
            }
            if let Some(sync) = i.sync_on_startup {
                state.drive.sync_on_startup = sync;
            }
            crate::remote_path(&state.drive, "validation")?;
            json!(state.drive)
        }
        _ => bail!("UNKNOWN_ACTION: {action}"),
    })
}

fn configure(state: &mut State, i: &Input) -> Result<()> {
    let id = project_id(state, i)?;
    crate::entities::active_project(state, &id)?;
    if let Some(client) = &i.client
        && !client.is_empty()
        && !state.billing.clients.contains_key(client)
    {
        bail!("CLIENT_NOT_FOUND")
    }
    if i.client
        .as_ref()
        .is_some_and(|client| state.billing.archived_clients.contains(client))
    {
        bail!("CLIENT_ARCHIVED: choose an active client")
    }
    if let Some(template) = &i.template {
        crate::template_path(template)?;
    }
    if let Some(tz) = &i.timezone {
        tz.parse::<chrono_tz::Tz>()
            .context("INVALID_INPUT: unknown timezone")?;
    }
    if let Some(cadence) = &i.cadence
        && !["manual", "weekly", "monthly"].contains(&cadence.as_str())
    {
        bail!("INVALID_INPUT: cadence must be manual, weekly or monthly")
    }
    if i.due_days.is_some_and(|d| d > 3650) {
        bail!("INVALID_INPUT: dueDays must not exceed 3650")
    }
    let project = state.projects.iter_mut().find(|p| p.id == id).unwrap();
    if i.name.is_some() {
        project.name = entity_name(&i.name, "name", 80)?;
    }
    if let Some(paper) = &i.paper {
        if !["a4", "letter"].contains(&paper.as_str()) {
            bail!("INVALID_INPUT: paper must be a4 or letter")
        }
        project.paper = paper.clone();
    }
    if let Some(color) = &i.accent_color {
        if !crate::is_color(color) {
            bail!("INVALID_INPUT: accentColor must be #RRGGBB")
        }
        project.accent_color = color.clone();
    }
    if let Some(logo) = &i.logo {
        project.logo_path = if logo.is_empty() {
            String::new()
        } else {
            let source =
                std::fs::canonicalize(logo).context("INVALID_INPUT: logo file does not exist")?;
            validate_image(&source)?;
            // Managed copy survives moving the original image.
            let dir = crate::templates::directory()?.join("assets").join(&id);
            std::fs::create_dir_all(&dir)?;
            let target = dir.join(format!(
                "{}.{}",
                crate::make_id("logo"),
                source.extension().unwrap().to_string_lossy()
            ));
            std::fs::copy(source, &target)?;
            target.display().to_string()
        };
    }
    let config = state.billing.projects.get_mut(&id).unwrap();
    if let Some(client) = &i.client {
        config.client_id = client.clone();
        project.client_name = state
            .billing
            .clients
            .get(client)
            .map(|c| c.name.clone())
            .unwrap_or_default();
    }
    if let Some(template) = &i.template {
        config.template_id = template.clone();
    }
    if let Some(cadence) = &i.cadence {
        config.cadence = cadence.clone();
    }
    if let Some(tz) = &i.timezone {
        config.timezone = tz.clone();
    }
    if let Some(days) = i.due_days {
        config.due_days = days;
    }
    if let Some(folder) = &i.drive_folder {
        config.drive_folder = folder.clone();
    }
    if i.rate.is_some() || i.no_rate {
        // The outer operation already checked concurrency before changing fields.
        let mut rate_input = i.clone();
        rate_input.entity_revision = None;
        rate_input.revision = None;
        mutate("project.rate", state, Path::new(""), &rate_input)?;
    }
    Ok(())
}

fn validate_image(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !path.is_file() || !["png", "jpg", "jpeg", "svg", "gif"].contains(&ext.as_str()) {
        bail!("INVALID_INPUT: image must be PNG, JPEG, SVG or GIF")
    }
    Ok(())
}

fn external(action: &str, path: &Path, i: &Input) -> Result<Value> {
    Ok(match action {
        "artifact.open" => crate::artifacts::open(&required(&i.path, "path")?)?,
        "invoice.preview" => b::render(path, &required(&i.id, "id")?, true)?,
        "invoice.render" => b::render(path, &required(&i.id, "id")?, false)?,
        "invoice.upload" => b::upload(path, &required(&i.id, "id")?)?,
        "template.list" => json!(crate::templates::list()?),
        "template.path" => json!({"path":crate::templates::path(&required(&i.id,"id")?)?}),
        "template.create" => json!(crate::templates::create(
            &required(&i.name, "name")?,
            i.copy_from.as_deref().unwrap_or("invoice")
        )?),
        "template.validate" => crate::templates::validate(&required(&i.id, "id")?)?,
        "template.asset" => {
            let id = required(&i.id, "id")?;
            if !id.starts_with("user:") {
                bail!("INVALID_INPUT: assets can only be imported into a user template")
            }
            let source = std::fs::canonicalize(required(&i.source, "source")?)?;
            validate_image(&source)?;
            let folder = crate::templates::path(&id)?
                .parent()
                .unwrap()
                .join("assets");
            std::fs::create_dir_all(&folder)?;
            let name = format!(
                "{}.{}",
                crate::make_id("image"),
                source.extension().unwrap().to_string_lossy()
            );
            let target = folder.join(&name);
            std::fs::copy(source, &target)?;
            json!({"path":target,"reference":format!("assets/{name}")})
        }
        "doctor" => {
            let tools = ["typst","rclone"].map(|tool| {
                let output = Command::new(tool).arg("--version").output();
                json!({"name":tool,"available":output.as_ref().is_ok_and(|o| o.status.success()),
                    "version":output.ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_owned()),
                    "install":format!("sudo pacman -S {tool}")})
            });
            json!({"dependencies":tools,"scheduler":crate::diagnostics(path),
                "nextSteps":["Install missing dependencies using your distribution's package manager.","Run rclone config; select Google Drive and complete browser authentication.","Use drive.configure, drive.check, then optionally drive.test.","Configure issuer/client and validate your invoice template."]})
        }
        "drive.check" | "drive.test" => {
            let drive = crate::locked_state(path)?.drive;
            if !crate::valid_remote(&drive.remote) {
                bail!("DRIVE_NOT_CONFIGURED: configure an rclone remote")
            }
            if action == "drive.check" {
                let output = Command::new("rclone")
                    .args(["lsjson", &format!("{}:", drive.remote), "--stat"])
                    .output()
                    .context("DEPENDENCY_MISSING: rclone")?;
                if !output.status.success() {
                    bail!(
                        "DRIVE_CHECK_FAILED: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )
                }
                json!({"remote":drive.remote,"accessible":true})
            } else {
                let temp = tempfile::NamedTempFile::new()?;
                std::fs::write(temp.path(), b"OmaTracker explicit upload test\n")?;
                let target = crate::remote_path(
                    &drive,
                    &format!("setup-tests/{}.txt", crate::make_id("test")),
                )?;
                crate::run_command(
                    "rclone",
                    [
                        "copyto".into(),
                        "--checksum".into(),
                        temp.path().display().to_string(),
                        target.clone(),
                    ],
                )?;
                json!({"remotePath":target,"uploaded":true})
            }
        }
        _ => bail!("UNKNOWN_ACTION: {action}"),
    })
}

fn replay_target(state: &State, action: &str, result: &Value) -> Result<()> {
    let Some(id) = result["data"]["id"].as_str() else {
        return Ok(());
    };
    let removed = match action {
        "project.create" => {
            state.billing.archived_projects.contains(id)
                || !state.projects.iter().any(|p| p.id == id)
        }
        "client.set" => {
            state.billing.archived_clients.contains(id) || !state.billing.clients.contains_key(id)
        }
        "task.create" => !state
            .tasks
            .iter()
            .any(|t| t.id == id && !state.billing.archived_projects.contains(&t.project_id)),
        _ => false,
    };
    if removed {
        bail!(
            "REQUEST_TARGET_REMOVED: this key refers to a removed entity ({id}); to create again use a fresh key from agent request.key or --key auto, not the previous creation key"
        )
    }
    Ok(())
}

pub fn execute(path: &Path, action: &str, input: Value, key: Option<&str>) -> Result<Value> {
    if action == "help" {
        return Ok(json!({"schemaVersion":1,"ok":true,"data":{
            "actions":ACTIONS,"request":"agent ACTION --input JSON [--key RETRY_KEY]",
            "contract":"AGENT_API.md","dates":"YYYY-MM-DD; to is exclusive; timestamps RFC3339 with offset",
            "invoiceStates":["draft","issued","paid","void"],"money":"integer minor-unit totals encoded as strings",
            "mutations":"Prepare independent keys together with agent request.keys (or request.key / --key auto for one NEW operation); reuse a resolved key only for exact retries. Prefer entityRevision for task/project/client edits; entries/invoices use revision.",
            "requestKeys":{"input":{"labels":["create-task","add-entry","price-entry"]},"maxItems":MAX_REQUEST_KEYS,"maxLabelBytes":MAX_KEY_LABEL_BYTES,"labels":"unique, nonempty UTF-8; no surrounding whitespace or control characters","result":"data.keys maps each label to a fresh key; no ledger access or retry-key argument"}
        }}));
    }
    if !ACTIONS.contains(&action) {
        bail!("UNKNOWN_ACTION: run agent help")
    }
    if action == "request.keys" {
        return request_keys(input, key);
    }
    let args: Input =
        serde_json::from_value(input.clone()).context("INVALID_INPUT: malformed request")?;
    if action == "request.key" {
        return Ok(
            json!({"schemaVersion":1,"ok":true,"changed":false,"data":{"key":crate::make_id("request")}}),
        );
    }
    if action == "data.clear" {
        if input.as_object().is_none_or(|object| {
            object
                .keys()
                .any(|key| !["dryRun", "includeDrive"].contains(&key.as_str()))
        }) {
            bail!(
                "INVALID_INPUT: data.clear accepts only dryRun and includeDrive and clears the whole ledger; use entity removal commands for individual projects/clients/tasks"
            )
        }
        if key.is_some() {
            bail!(
                "INVALID_INPUT: data.clear does not accept retry keys; inspect its backup/progress after an interruption before starting another clear"
            )
        }
        let mut data = crate::clear_data::clear(path, args.dry_run, args.include_drive)?;
        data.as_object_mut().unwrap().remove("schemaVersion");
        data.as_object_mut().unwrap().remove("ok");
        return Ok(json!({"schemaVersion":1,"ok":true,"changed":!args.dry_run,"data":data}));
    }
    if args.apply_existing && !matches!(action, "task.rate" | "task.update") {
        bail!("INVALID_INPUT: applyExisting is supported by task.rate and task.update only")
    }
    if key.is_some_and(|k| k.is_empty() || k.len() > 200) {
        bail!("INVALID_INPUT: retry key must have 1–200 characters")
    }
    if READS.contains(&action) {
        let mut state = crate::locked_state(path)?;
        b::initialize(&mut state);
        return Ok(
            json!({"schemaVersion":1,"ok":true,"changed":false,"revision":state.billing.revision,"data":read(action,&state,&args)?}),
        );
    }
    if action.starts_with("template.")
        || [
            "artifact.open",
            "doctor",
            "drive.check",
            "drive.test",
            "invoice.preview",
            "invoice.render",
            "invoice.upload",
        ]
        .contains(&action)
    {
        if let Some(key) = key {
            if !matches!(action, "invoice.render" | "invoice.upload") {
                bail!(
                    "INVALID_INPUT: --key is supported for ledger mutations and invoice render/upload; omit it for other external operations"
                )
            }
            return external_receipt(path, action, &args, &input, key);
        }
        return Ok(json!({"schemaVersion":1,"ok":true,"data":external(action,path,&args)?}));
    }
    // Prevent an upload from racing an invoice void; timer writes do not use this lock.
    let _worker = if matches!(action, "invoice.void" | "invoice.paid") {
        Some(crate::lock_file(&PathBuf::from(format!(
            "{}.invoices-worker",
            path.display()
        )))?)
    } else {
        None
    };
    let fingerprint = serde_json::to_string(&(action, input))?;
    crate::mutate_state(path, |state| {
        if let Some(receipt) = key.and_then(|key| state.billing.requests.get(key)) {
            if receipt.fingerprint != fingerprint {
                bail!(
                    "IDEMPOTENCY_CONFLICT: this key already completed a different request; use agent request.key or --key auto for a new operation. Exact retries must keep their original key and arguments"
                )
            }
            if receipt.result.is_null() {
                bail!("OPERATION_IN_PROGRESS: retry the original external operation")
            }
            replay_target(state, action, &receipt.result)?;
            let mut response = receipt.result.clone();
            response["replayed"] = json!(true);
            response["requestKey"] = json!(key);
            return Ok(Mutation::Unchanged(response));
        }
        let before = serde_json::to_vec(state)?;
        let data = mutate(action, state, path, &args)?;
        let changed = before != serde_json::to_vec(state)?;
        if changed || key.is_some() {
            state.billing.revision += 1;
        }
        let mut response = json!({"schemaVersion":1,"ok":true,"changed":changed,"revision":state.billing.revision,"data":data});
        if let Some(key) = key {
            response["requestKey"] = json!(key);
        }
        if let Some(key) = key {
            state.billing.requests.insert(
                key.into(),
                b::Receipt {
                    fingerprint,
                    result: response.clone(),
                },
            );
        }
        Ok(if changed || key.is_some() {
            Mutation::Changed(response)
        } else {
            Mutation::Unchanged(response)
        })
    })
}

fn external_receipt(
    path: &Path,
    action: &str,
    args: &Input,
    input: &Value,
    key: &str,
) -> Result<Value> {
    let _lock = crate::lock_file(&PathBuf::from(format!("{}.agent-external", path.display())))?;
    let fingerprint = serde_json::to_string(&(action, input))?;
    let replay = crate::mutate_state(path, |state| {
        if let Some(receipt) = state.billing.requests.get(key) {
            if receipt.fingerprint != fingerprint {
                bail!(
                    "IDEMPOTENCY_CONFLICT: key was used with different arguments; generate a fresh request.key for a new operation"
                )
            }
            return Ok(Mutation::Unchanged(
                (!receipt.result.is_null()).then(|| receipt.result.clone()),
            ));
        }
        state.billing.requests.insert(
            key.into(),
            b::Receipt {
                fingerprint: fingerprint.clone(),
                result: Value::Null,
            },
        );
        Ok(Mutation::Changed(None))
    })?;
    if let Some(mut response) = replay {
        response["replayed"] = json!(true);
        response["requestKey"] = json!(key);
        return Ok(response);
    }
    // A crash after upload but before receipt persistence retries the same pinned destination.
    let response =
        json!({"schemaVersion":1,"ok":true,"requestKey":key,"data":external(action,path,args)?});
    crate::mutate_state(path, |state| {
        state
            .billing
            .requests
            .get_mut(key)
            .context("missing external receipt")?
            .result = response.clone();
        Ok(Mutation::Changed(()))
    })?;
    Ok(response)
}

pub fn run(path: &Path, cli: Cli) -> Result<Value> {
    let text = if let Some(file) = cli.input_file {
        if file == Path::new("-") {
            std::io::read_to_string(std::io::stdin())?
        } else {
            std::fs::read_to_string(file)?
        }
    } else {
        cli.input
    };
    let input = serde_json::from_str(&text).context("INVALID_INPUT: request must be JSON")?;
    let key = cli.key.map(|key| {
        if key == "auto" {
            let key = crate::make_id("request");
            eprintln!("requestKey: {key}");
            key
        } else {
            key
        }
    });
    execute(path, &cli.action, input, key.as_deref())
}

pub fn error(error: &anyhow::Error) -> Value {
    let message = format!("{error:#}");
    let code = message
        .split(':')
        .next()
        .filter(|c| c.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        .unwrap_or("OPERATION_FAILED");
    json!({"schemaVersion":1,"ok":false,"error":{"code":code,"message":message}})
}
