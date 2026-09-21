//! Explicit task rate overrides and opt-in pricing of existing unrated entries.
use crate::{HourlyRate, State, Task, billing::EntryBilling};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatePoint {
    pub effective_at: i64,
    pub rate: Option<HourlyRate>,
    pub inherit_project: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    pub id: String,
    pub task_id: String,
    pub created_at: i64,
    pub rate: HourlyRate,
    pub reason: String,
    pub previous_billing: BTreeMap<String, EntryBilling>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub rate: Option<HourlyRate>,
    pub rate_source: String,
    pub hourly_rate: String,
    pub entity_revision: String,
}

pub(crate) fn policy<'a>(state: &'a State, task: &str, at: i64) -> Option<&'a RatePoint> {
    state
        .billing
        .task_rates
        .get(task)
        .and_then(|points| points.iter().rev().find(|p| p.effective_at <= at))
}

pub(crate) fn at(state: &State, task: &str, project: &str, at: i64) -> EntryBilling {
    if let Some(point) = policy(state, task, at).filter(|p| !p.inherit_project) {
        EntryBilling {
            resolved: true,
            rate: point.rate.clone(),
            ..Default::default()
        }
    } else {
        crate::billing::at_rate(state, project, at)
    }
}

pub(crate) fn view(state: &State, task: &Task, now: i64) -> View {
    let rate = at(state, &task.id, &task.project_id, now).rate;
    View {
        hourly_rate: rate
            .as_ref()
            .map(|r| r.estimate(0).hourly_rate)
            .unwrap_or_default(),
        rate,
        rate_source: if policy(state, &task.id, now).is_some_and(|p| !p.inherit_project) {
            "task"
        } else {
            "project"
        }
        .into(),
        // The task is drawn from this state; serialization of these types is infallible.
        entity_revision: crate::entities::revision(state, "task", &task.id)
            .expect("task belongs to state"),
    }
}

pub(crate) fn task_json(state: &State, task: &Task) -> Value {
    let mut result = json!(task);
    result.as_object_mut().unwrap().extend(
        serde_json::to_value(view(state, task, crate::now_ms()))
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    );
    result["rateHistory"] = json!(
        state
            .billing
            .task_rates
            .get(&task.id)
            .cloned()
            .unwrap_or_default()
    );
    result
}

fn encode_cursor(activity_at: i64, id: &str) -> String {
    serde_json::to_vec(&(activity_at, id))
        .expect("cursor serialization is infallible")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode_cursor(cursor: &str) -> Result<(i64, String)> {
    if !cursor.is_ascii() || !cursor.len().is_multiple_of(2) {
        bail!("INVALID_INPUT: cursor is invalid")
    }
    let hex = |byte: u8| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("INVALID_INPUT: cursor is invalid"),
    };
    let (pairs, _) = cursor.as_bytes().as_chunks::<2>();
    let bytes: Result<Vec<_>> = pairs
        .iter()
        .map(|[high, low]| Ok((hex(*high)? << 4) | hex(*low)?))
        .collect();
    serde_json::from_slice(&bytes?).context("INVALID_INPUT: cursor is invalid")
}

pub(crate) fn catalogue(state: &State, input: &crate::agent::Input) -> Result<Value> {
    let project = match (&input.project, input.all_projects) {
        (Some(_), Some(true)) => {
            bail!("INVALID_INPUT: supply exactly one of project or allProjects: true")
        }
        (Some(project), _) => {
            crate::entities::active_project(state, project)?;
            Some(project.as_str())
        }
        (None, Some(true)) => None,
        _ => bail!("INVALID_INPUT: supply exactly one of project or allProjects: true"),
    };
    let lifecycle = input.state.as_deref().unwrap_or("all");
    if !["all", "open", "stopped", "tracking", "done"].contains(&lifecycle) {
        bail!("INVALID_INPUT: state must be all, open, stopped, tracking, or done")
    }
    let query = input.query.as_deref().unwrap_or("").to_lowercase();
    let now = crate::now_ms();
    let mut tasks: Vec<_> = state
        .tasks
        .iter()
        .filter_map(|task| {
            let matches_scope = project.map_or_else(
                || !state.billing.archived_projects.contains(&task.project_id),
                |project| task.project_id == project,
            );
            let matches_lifecycle = match lifecycle {
                "all" => true,
                "open" => !task.is_done(),
                "stopped" => task.status == crate::TaskStatus::Stopped,
                "tracking" => task.is_tracking(),
                "done" => task.is_done(),
                _ => unreachable!(),
            };
            let project = state
                .projects
                .iter()
                .find(|project| project.id == task.project_id)?;
            (matches_scope && matches_lifecycle && task.title.to_lowercase().contains(&query))
                .then_some((task, project))
        })
        .collect();
    tasks.sort_by(|(left, _), (right, _)| {
        right
            .activity_at
            .cmp(&left.activity_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let total = tasks.len();
    let cursor = input.cursor.as_deref().map(decode_cursor).transpose()?;
    let start = cursor.map_or(0, |(activity_at, id)| {
        tasks
            .iter()
            .position(|(task, _)| {
                task.activity_at < activity_at || (task.activity_at == activity_at && task.id > id)
            })
            .unwrap_or(tasks.len())
    });
    let limit = input.limit.unwrap_or(50).clamp(1, 200);
    let page = &tasks[start..tasks.len().min(start.saturating_add(limit))];
    let items: Vec<_> = page
        .iter()
        .map(|(task, project)| {
            let mut item = task_json(state, task);
            item["durationSeconds"] = json!(crate::task_seconds(state, task, now));
            item["projectId"] = json!(task.project_id);
            item["projectName"] = json!(project.name);
            item["activityAt"] = json!(task.activity_at);
            item
        })
        .collect();
    let next_cursor = (start + page.len() < tasks.len())
        .then(|| {
            page.last()
                .map(|(task, _)| encode_cursor(task.activity_at, &task.id))
        })
        .flatten();
    Ok(json!({"items":items,"total":total,"nextCursor":next_cursor}))
}

/// Called inside the ledger transaction; existing invoice amounts are never changed.
pub(crate) fn assign(
    state: &mut State,
    task_id: &str,
    input: &crate::agent::Input,
) -> Result<Value> {
    let index = state
        .tasks
        .iter()
        .position(|t| t.id == task_id)
        .context("TASK_NOT_FOUND")?;
    crate::entities::active_project(state, &state.tasks[index].project_id)?;
    if usize::from(input.rate.is_some())
        + usize::from(input.no_rate)
        + usize::from(input.inherit_rate)
        != 1
    {
        bail!("INVALID_INPUT: supply exactly one of rate, noRate, or inheritRate")
    }
    if input.rate.is_none() && input.currency.is_some() {
        bail!("INVALID_INPUT: currency requires rate")
    }
    if input.apply_existing && input.rate.is_none() {
        bail!("INVALID_INPUT: applyExisting requires an explicit rate")
    }
    let now = crate::now_ms();
    let effective_at = input
        .effective_at
        .as_deref()
        .map(crate::billing::timestamp)
        .transpose()?
        .unwrap_or(now);
    let rate = input
        .rate
        .as_ref()
        .map(|amount| {
            HourlyRate::parse(
                amount,
                input
                    .currency
                    .as_deref()
                    .context("INVALID_INPUT: currency is required for a task rate")?,
            )
        })
        .transpose()?;

    // Include already-running, unrated time in an explicitly requested backfill.
    // Its old pricing is captured before installing the new policy.
    if input.apply_existing && state.tasks[index].is_tracking() {
        let task = state.tasks[index].clone();
        crate::append_entry(
            state,
            &task,
            task.started_at,
            now,
            (now - task.started_at) / 1000,
            "",
        );
        state.tasks[index].started_at = now;
    }

    let point = RatePoint {
        effective_at,
        rate: rate.clone(),
        inherit_project: input.inherit_rate,
    };
    let unchanged = input.effective_at.is_none()
        && policy(state, task_id, now).map_or(input.inherit_rate, |p| {
            p.rate == point.rate && p.inherit_project == point.inherit_project
        });
    if !unchanged {
        let history = state.billing.task_rates.entry(task_id.into()).or_default();
        history.retain(|p| p.effective_at != effective_at);
        history.push(point);
        history.sort_by_key(|p| p.effective_at);
    }

    let mut previous = BTreeMap::new();
    let mut already_rated = 0;
    let mut invoiced = 0;
    let mut externally_billed = 0;
    if input.apply_existing {
        for entry in state
            .entries
            .iter()
            .filter(|e| e.task_id == task_id && e.seconds > 0)
        {
            let old = state
                .billing
                .entries
                .get(&entry.id)
                .cloned()
                .unwrap_or_default();
            if old.externally_billed {
                externally_billed += 1;
                continue;
            }
            if state
                .billing
                .invoices
                .iter()
                .filter(|i| i.state == "issued" || i.state == "paid")
                .flat_map(|i| &i.allocations)
                .any(|a| a.entry_id == entry.id)
            {
                invoiced += 1;
                continue;
            }
            if old.rate.is_some() {
                already_rated += 1;
                continue;
            }
            let mut new = old.clone();
            new.resolved = true;
            new.rate = rate.clone();
            new.revision += 1;
            previous.insert(entry.id.clone(), old);
            state.billing.entries.insert(entry.id.clone(), new);
        }
    }
    let applied: Vec<_> = previous.keys().cloned().collect();
    let adjustment_id = if !previous.is_empty() {
        let adjustment = Adjustment {
            id: crate::make_id("rate-adjustment"),
            task_id: task_id.into(),
            created_at: now,
            rate: rate.clone().unwrap(),
            reason: input
                .reason
                .clone()
                .unwrap_or_else(|| "Explicitly price existing unrated task time".into()),
            previous_billing: previous,
        };
        let id = adjustment.id.clone();
        state.billing.task_rate_adjustments.push(adjustment);
        Some(id)
    } else {
        None
    };
    if !unchanged || adjustment_id.is_some() {
        crate::entities::record_activity(&mut state.tasks[index], now);
    }
    let mut result = task_json(state, &state.tasks[index]);
    result["rateChange"] = json!({"effectiveAt":effective_at,"appliedEntryIds":applied,"adjustmentId":adjustment_id,
        "skipped":{"alreadyRated":already_rated,"invoiced":invoiced,"externallyBilled":externally_billed}});
    Ok(result)
}
