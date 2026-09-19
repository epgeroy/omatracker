//! Durable local coordination. Money, interval splitting and receipts belong to
//! the existing agent/billing paths, never to the workflow journal.
use crate::{HourlyRate, Mutation, State, agent, billing as b};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Pricing {
    Explicit { rate: String, currency: String },
    HistoricalInheritance {},
}

impl Pricing {
    pub(crate) fn rate(&self) -> Result<Option<HourlyRate>> {
        match self {
            Self::Explicit { rate, currency } => Ok(Some(HourlyRate::parse(rate, currency)?)),
            Self::HistoricalInheritance {} => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    project: String,
    items: Vec<Item>,
    pricing: Pricing,
    summary: Range,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Item {
    #[serde(rename = "ref")]
    reference: String,
    new_task: NewTask,
    entries: Vec<DatedEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NewTask {
    title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DatedEntry {
    start: String,
    end: String,
    #[serde(default = "default_note")]
    note: String,
}

fn default_note() -> String {
    "Manual entry".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Range {
    from: String,
    to: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    item: usize,
    action: String,
    key: String,
    input: Value,
    task_step: Option<usize>,
    ready: bool,
    result: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    version: u32,
    ledger: PathBuf,
    ledger_id: String,
    request: Request,
    steps: Vec<Step>,
    completed: Option<Value>,
    last_error: Option<Value>,
}

/// Content-free binding survives clear-all so old keys cannot start new work on
/// a replacement ledger. Actual work/input/results stay in the ledger journal.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    version: u32,
    ledger_id: String,
}

fn binding_path(path: &Path, key: &str) -> PathBuf {
    PathBuf::from(format!("{}.workflows", path.display()))
        .join(format!("{:x}.json", Sha256::digest(key.as_bytes())))
}

fn check_binding(path: &Path, key: &str, ledger_id: &str) -> Result<bool> {
    let bytes = match std::fs::read(binding_path(path, key)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    let binding: Binding =
        serde_json::from_slice(&bytes).context("WORKFLOW_STATE_INVALID: invalid ledger binding")?;
    if binding.version != 1 {
        bail!("WORKFLOW_STATE_INVALID: unsupported binding version")
    }
    if ledger_id.is_empty() || binding.ledger_id != ledger_id {
        bail!(
            "WORKFLOW_LEDGER_MISMATCH: original ledger was cleared or replaced; old workflow keys cannot be resumed"
        )
    }
    Ok(true)
}

fn bind_ledger(path: &Path, key: &str, ledger_id: &str) -> Result<()> {
    if !check_binding(path, key, ledger_id)? {
        let target = binding_path(path, key);
        crate::atomic_write(
            &target,
            &serde_json::to_vec(&Binding {
                version: 1,
                ledger_id: ledger_id.into(),
            })?,
        )?;
        sync_directory(&target)?;
        // Include the directory's own creation in the durability barrier.
        sync_directory(path)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct PartialFailure(pub Value);
impl std::fmt::Display for PartialFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0["error"]["message"]
                .as_str()
                .unwrap_or("workflow interrupted")
        )
    }
}
impl std::error::Error for PartialFailure {}

fn text(value: &str, limit: usize) -> Result<()> {
    if value.chars().count() > limit || value.chars().any(char::is_control) {
        bail!("INVALID_INPUT: text must be single-line and at most {limit} characters")
    }
    Ok(())
}

fn normalize(input: Value) -> Result<Request> {
    let mut request: Request =
        serde_json::from_value(input).context("INVALID_INPUT: malformed batch")?;
    if request.project.trim().is_empty() || request.items.is_empty() || request.items.len() > 50 {
        bail!("INVALID_INPUT: explicit project and 1–50 items required")
    }
    request.pricing.rate()?;
    let mut refs = BTreeSet::new();
    let mut count = 0;
    for item in &mut request.items {
        if item.reference.is_empty()
            || item.reference.len() > 64
            || !item
                .reference
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
            || !refs.insert(item.reference.clone())
        {
            bail!("INVALID_INPUT: refs must be unique, 1–64 ASCII letters/digits/-/_")
        }
        text(&item.new_task.title, 160)?;
        item.new_task.title = item.new_task.title.trim().to_owned();
        if item.new_task.title.is_empty() || item.entries.is_empty() || item.entries.len() > 200 {
            bail!("INVALID_INPUT: nonblank title and 1–200 entries required per item")
        }
        count += item.entries.len();
        for entry in &mut item.entries {
            text(&entry.note, 240)?;
            let parse = |value: &str| -> Result<chrono::DateTime<chrono::FixedOffset>> {
                let at = chrono::DateTime::parse_from_rfc3339(value)
                    .context("INVALID_INPUT: timestamp must be RFC3339")?;
                if at.timestamp() <= 0 || at.timestamp_subsec_nanos() != 0 {
                    bail!("INVALID_INPUT: timestamps must be after epoch at whole-second precision")
                }
                Ok(at)
            };
            let start = parse(&entry.start)?;
            let end = parse(&entry.end)?;
            let seconds = end.signed_duration_since(start).num_seconds();
            if !(1..=31 * 86400).contains(&seconds) || end.timestamp_millis() > crate::now_ms() {
                bail!(
                    "INVALID_INPUT: entry duration must be 1 second–31 days and end no later than now"
                )
            }
            entry.start = start
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            entry.end = end
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        }
    }
    if count > 200 {
        bail!("INVALID_INPUT: at most 200 entries per batch")
    }
    Ok(request)
}

fn summary_input(request: &Request) -> agent::Input {
    agent::Input {
        project: Some(request.project.clone()),
        from: Some(request.summary.from.clone()),
        to: Some(request.summary.to.clone()),
        ..Default::default()
    }
}

fn steps(request: &Request) -> Vec<Step> {
    let mut steps = Vec::new();
    for (item, value) in request.items.iter().enumerate() {
        let task_step = steps.len();
        steps.push(Step {
            item,
            action: "task.create".into(),
            key: crate::make_id("request"),
            input: json!({"project":request.project,"title":value.new_task.title}),
            task_step: None,
            ready: false,
            result: None,
        });
        for entry in &value.entries {
            steps.push(Step { item, action: "entry.add".into(), key: crate::make_id("request"),
                input: json!({"start":entry.start,"end":entry.end,"note":entry.note,"pricing":request.pricing}),
                task_step: Some(task_step), ready: false, result: None });
        }
    }
    steps
}

fn resolve(steps: &[Step], index: usize) -> Result<Value> {
    let mut input = steps[index].input.clone();
    if let Some(task) = steps[index].task_step {
        let id = steps[task]
            .result
            .as_ref()
            .and_then(|r| r["data"]["id"].as_str())
            .context("WORKFLOW_RESULT_INVALID: missing task ID")?;
        input["id"] = json!(id);
    }
    Ok(input)
}

fn preflight(state: &State, request: &Request, planned: &[Step]) -> Result<Value> {
    crate::entities::active_project(state, &request.project)?;
    b::bounds(
        &request.summary.from,
        &request.summary.to,
        &b::settings(state, &request.project)?.timezone,
    )?;
    let mut simulation = state.clone();
    let mut steps = planned.to_vec();
    let mut preview = Vec::new();
    for index in 0..steps.len() {
        let args = serde_json::from_value(resolve(&steps, index)?)?;
        let result = agent::mutate(&steps[index].action, &mut simulation, Path::new(""), &args)?;
        let segments = result["entries"].as_array().map(|entries| {
            entries
                .iter()
                .map(|e| {
                    json!({"start":e["entry"]["startedAt"],"end":e["entry"]["endedAt"],
                "seconds":e["entry"]["seconds"],"billing":e["billing"]})
                })
                .collect::<Vec<_>>()
        });
        preview.push(json!({"ref":request.items[steps[index].item].reference,
            "action":steps[index].action,"billingSegments":segments}));
        steps[index].result = Some(json!({"data":result}));
    }
    Ok(
        json!({"status":"validated","steps":preview,"futureRatePolicy":"unchanged",
        "summary":agent::read("summary", &simulation, &summary_input(request))?,
        "historicalPricing":"Snapshot estimate only; each actual write uses the then-current history."}),
    )
}

fn fingerprint(request: &Request) -> Result<String> {
    // serde_json's default object map sorts keys recursively; array order remains significant.
    let canonical = json!({"action":"work.record-batch","normalizationVersion":1,"input":request});
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

fn load(state: &State, key: &str, fingerprint: &str, ledger: &Path) -> Result<Option<Journal>> {
    let bound = check_binding(ledger, key, &state.billing.ledger_id)?;
    let Some(receipt) = state.billing.requests.get(key) else {
        if bound {
            bail!("WORKFLOW_STATE_LOST: workflow receipt was removed; do not start it again")
        }
        return Ok(None);
    };
    if receipt.fingerprint != fingerprint {
        bail!("IDEMPOTENCY_CONFLICT: workflow key was used with different arguments")
    }
    let journal: Journal = serde_json::from_value(receipt.result.clone())
        .context("WORKFLOW_STATE_INVALID: invalid journal")?;
    if journal.version != 1 {
        bail!("WORKFLOW_STATE_INVALID: unsupported journal version")
    }
    if journal.ledger != ledger || journal.ledger_id != state.billing.ledger_id {
        bail!(
            "WORKFLOW_LEDGER_MISMATCH: resume against {}",
            journal.ledger.display()
        )
    }
    Ok(Some(journal))
}

fn store(state: &mut State, key: &str, journal: &Journal) -> Result<()> {
    state.billing.requests.insert(
        key.into(),
        b::Receipt {
            fingerprint: fingerprint(&journal.request)?,
            result: serde_json::to_value(journal)?,
        },
    );
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path.parent().context("ledger has no parent")?)?.sync_all()?;
    Ok(())
}

fn checkpoint(path: &Path, key: &str, journal: &Journal) -> Result<()> {
    crate::mutate_state(path, |state| {
        load(state, key, &fingerprint(&journal.request)?, path)?
            .context("WORKFLOW_STATE_LOST: journal was removed")?;
        store(state, key, journal)?;
        Ok(Mutation::Changed(()))
    })?;
    sync_directory(path)
}

fn step_result<'a>(step: &'a Step, state: &'a State) -> Option<&'a Value> {
    step.result
        .as_ref()
        .or_else(|| state.billing.requests.get(&step.key).map(|r| &r.result))
}

fn progress(journal: &Journal, state: &State, key: &str, status: &str) -> Value {
    let items: Vec<_> = journal.request.items.iter().enumerate().map(|(index, item)| {
        let steps: Vec<_> = journal.steps.iter().filter(|s| s.item == index).collect();
        let task = step_result(steps[0], state).map(|r| r["data"]["id"].clone());
        let entries: Vec<_> = steps.iter().skip(1).filter_map(|s| step_result(s, state))
            .filter_map(|r| r["data"]["entries"].as_array()).flatten().map(|e|
                json!({"id":e["entry"]["id"],"seconds":e["entry"]["seconds"],"billing":e["billing"]})).collect();
        let completed = steps.iter().filter(|s| s.result.is_some()).count();
        let recorded = steps.iter().filter(|s| s.result.is_none() && step_result(s, state).is_some()).count();
        json!({"ref":item.reference,"taskId":task,"entries":entries,
            "status":if completed == steps.len() {"completed"} else if task.is_some() {"partial"} else {"pending"},
            "completedSteps":completed,"recordedSteps":recorded,"pendingSteps":steps.len()-completed-recorded,
            "skippedAdjustments":[]})
    }).collect();
    json!({"status":status,"resume":{"key":key,"ledger":journal.ledger},"items":items,
        "futureRatePolicy":"unchanged"})
}

fn verify_targets(journal: &Journal, state: &State) -> Result<()> {
    for step in &journal.steps {
        let Some(result) = step_result(step, state) else {
            continue;
        };
        if step.action == "task.create" {
            let id = result["data"]["id"]
                .as_str()
                .context("WORKFLOW_RESULT_INVALID: missing task ID")?;
            if !state
                .tasks
                .iter()
                .any(|t| t.id == id && t.project_id == journal.request.project)
                || state
                    .billing
                    .archived_projects
                    .contains(&journal.request.project)
            {
                bail!("REQUEST_TARGET_REMOVED: workflow task {id} was removed")
            }
        } else {
            let entries = result["data"]["entries"]
                .as_array()
                .filter(|v| !v.is_empty())
                .context("WORKFLOW_RESULT_INVALID: missing entries")?;
            for entry in entries {
                if !state
                    .entries
                    .iter()
                    .any(|e| Some(e.id.as_str()) == entry["entry"]["id"].as_str())
                {
                    bail!("REQUEST_TARGET_REMOVED: workflow entry was removed")
                }
            }
        }
    }
    Ok(())
}

fn validate_result(step: &Step, result: &Value, pricing: &Pricing) -> Result<()> {
    if result["ok"] != true {
        bail!("WORKFLOW_RESULT_INVALID: mutation did not succeed")
    }
    if step.action == "task.create" {
        result["data"]["id"]
            .as_str()
            .context("WORKFLOW_RESULT_INVALID: missing task ID")?;
    } else {
        let entries = result["data"]["entries"]
            .as_array()
            .filter(|e| !e.is_empty())
            .context("WORKFLOW_RESULT_INVALID: missing entries")?;
        let rate = pricing.rate()?;
        for entry in entries {
            let meta: b::EntryBilling = serde_json::from_value(entry["billing"].clone())?;
            if rate.is_some() && (!meta.resolved || meta.rate != rate || meta.externally_billed) {
                bail!("WORKFLOW_RESULT_INVALID: explicit billing did not match requested pricing")
            }
        }
    }
    Ok(())
}

pub(crate) fn record_batch(path: &Path, input: Value, key: Option<&str>) -> Result<Value> {
    record_with_hook(path, input, key, |_, _| Ok(()))
}

// Hook is dependency-injected for crash-window tests, never controlled by environment/input.
fn record_with_hook(
    path: &Path,
    input: Value,
    key: Option<&str>,
    mut hook: impl FnMut(&str, usize) -> Result<()>,
) -> Result<Value> {
    let mut request = normalize(input)?;
    if key.is_some_and(|k| k.trim().is_empty() || k.len() > 200 || k == "auto") {
        bail!("INVALID_INPUT: use a resolved retry key of 1–200 bytes")
    }
    if request.dry_run {
        let mut state = crate::locked_state(path)?;
        b::initialize(&mut state);
        return Ok(
            json!({"schemaVersion":1,"ok":true,"changed":false,"revision":state.billing.revision,
            "data":preflight(&state, &request, &steps(&request))?}),
        );
    }
    let key = key.context("INVALID_INPUT: work.record-batch requires a retained --key")?;
    request.dry_run = false;
    let path = std::fs::canonicalize(path).or_else(|_| {
        let absolute = std::path::absolute(path)?;
        Ok::<_, std::io::Error>(
            std::fs::canonicalize(absolute.parent().unwrap())?.join(absolute.file_name().unwrap()),
        )
    })?;
    let _worker = crate::lock_file(&PathBuf::from(format!(
        "{}.workflows-worker",
        path.display()
    )))?;
    let fingerprint = fingerprint(&request)?;
    let mut journal = crate::mutate_state(&path, |state| {
        if let Some(journal) = load(state, key, &fingerprint, &path)? {
            return Ok(Mutation::Unchanged(journal));
        }
        let steps = steps(&request);
        preflight(state, &request, &steps)?;
        if state.billing.ledger_id.is_empty() {
            state.billing.ledger_id = crate::make_id("ledger");
        }
        let journal = Journal {
            version: 1,
            ledger: path.clone(),
            ledger_id: state.billing.ledger_id.clone(),
            request,
            steps,
            completed: None,
            last_error: None,
        };
        store(state, key, &journal)?;
        Ok(Mutation::Changed(journal))
    })?;
    sync_directory(&path)?;
    let mut failed_step = None;
    let execution = (|| -> Result<Value> {
        bind_ledger(&path, key, &journal.ledger_id)?;
        verify_targets(&journal, &crate::locked_state(&path)?)?;
        if let Some(mut response) = journal.completed.clone() {
            response["replayed"] = json!(true);
            response["changed"] = json!(false);
            return Ok(response);
        }
        hook("planned", 0)?;
        for index in 0..journal.steps.len() {
            failed_step = Some(index);
            if journal.steps[index].result.is_some() {
                continue;
            }
            journal.steps[index].input = resolve(&journal.steps, index)?;
            journal.steps[index].ready = true;
            journal.last_error = None;
            checkpoint(&path, key, &journal)?;
            hook("prepared", index)?;
            let step = &journal.steps[index];
            let result = agent::execute(&path, &step.action, step.input.clone(), Some(&step.key))?;
            sync_directory(&path)?;
            hook("mutated", index)?;
            validate_result(step, &result, &journal.request.pricing)?;
            journal.steps[index].result = Some(result);
            checkpoint(&path, key, &journal)?;
            hook("checkpointed", index)?;
        }
        failed_step = None;
        hook("summarizing", journal.steps.len())?;
        // Verification and final summary/checkpoint share a snapshot. No nested lock.
        let response = crate::mutate_state(&path, |state| {
            load(state, key, &fingerprint, &path)?
                .context("WORKFLOW_STATE_LOST: journal was removed")?;
            verify_targets(&journal, state)?;
            let mut data = progress(&journal, state, key, "completed");
            data["summary"] = agent::read("summary", state, &summary_input(&journal.request))?;
            data["summaryRevision"] = json!(state.billing.revision);
            let response = json!({"schemaVersion":1,"ok":true,"changed":true,
                "revision":state.billing.revision+1,"requestKey":key,"data":data});
            journal.completed = Some(response.clone());
            store(state, key, &journal)?;
            Ok(Mutation::Changed(response))
        })?;
        sync_directory(&path)?;
        hook("completed", journal.steps.len())?;
        Ok(response)
    })();
    execution.map_err(|error| {
        let mut response = agent::error(&error);
        journal.last_error = Some(response["error"].clone());
        if let Err(persistence) = checkpoint(&path, key, &journal) {
            response["journalError"] = json!(format!("{persistence:#}"));
        }
        let state = crate::locked_state(&path).unwrap_or_default();
        response["requestKey"] = json!(key);
        response["data"] = progress(&journal, &state, key, "partial");
        response["data"]["failedStep"] = json!(failed_step);
        anyhow::Error::new(PartialFailure(response))
    })
}
