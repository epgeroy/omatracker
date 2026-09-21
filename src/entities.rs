//! Entity lifecycle operations shared by CLI adapters. Call with the ledger lock held.
use crate::{State, Task, TaskStatus};
use anyhow::{Context, Result, bail};
use serde_json::json;
use sha2::{Digest, Sha256};

/// Entity-scoped concurrency tokens exclude unrelated ledger writes and receipts.
pub(crate) fn revision(state: &State, kind: &str, id: &str) -> Result<String> {
    let value = match kind {
        "task" => {
            json!({"task": state.tasks.iter().find(|t| t.id == id).context("TASK_NOT_FOUND")?,
            "rates": state.billing.task_rates.get(id)})
        }
        "project" => {
            json!({"project": state.projects.iter().find(|p| p.id == id).context("PROJECT_NOT_FOUND")?,
            "billing": state.billing.projects.get(id), "archived": state.billing.archived_projects.contains(id)})
        }
        "client" => {
            json!({"id":id,"details":state.billing.clients.get(id).context("CLIENT_NOT_FOUND")?,
            "archived":state.billing.archived_clients.contains(id)})
        }
        _ => bail!("INVALID_INPUT: unknown entity kind"),
    };
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

pub(crate) fn active_project(state: &State, id: &str) -> Result<()> {
    if !state.projects.iter().any(|p| p.id == id) {
        bail!("PROJECT_NOT_FOUND: {id}")
    }
    if state.billing.archived_projects.contains(id) {
        bail!("PROJECT_ARCHIVED: {id}")
    }
    Ok(())
}

pub(crate) fn start_task(state: &mut State, index: usize, now: i64) -> Result<bool> {
    let task = &mut state.tasks[index];
    if task.is_done() {
        bail!("TASK_DONE: reopen the task before tracking time")
    }
    if task.is_tracking() {
        return Ok(false);
    }
    task.status = TaskStatus::Tracking;
    task.started_at = now;
    Ok(true)
}

pub(crate) fn stop_task(state: &mut State, index: usize, now: i64) -> bool {
    if !state.tasks[index].is_tracking() {
        return false;
    }
    let task = state.tasks[index].clone();
    crate::append_entry(
        state,
        &task,
        task.started_at,
        now,
        (now - task.started_at) / 1000,
        "",
    );
    state.tasks[index].status = TaskStatus::Stopped;
    state.tasks[index].started_at = 0;
    true
}

pub(crate) fn complete_task(state: &mut State, index: usize, now: i64) -> bool {
    if state.tasks[index].is_done() {
        return false;
    }
    stop_task(state, index, now);
    state.tasks[index].status = TaskStatus::Done;
    state.tasks[index].completed_at = now;
    true
}

pub(crate) fn reopen_task(state: &mut State, index: usize) -> bool {
    if !state.tasks[index].is_done() {
        return false;
    }
    state.tasks[index].status = TaskStatus::Stopped;
    state.tasks[index].completed_at = 0;
    true
}

pub(crate) fn remove_task(state: &mut State, id: &str) -> Result<Task> {
    let index = state
        .tasks
        .iter()
        .position(|t| t.id == id)
        .context("TASK_NOT_FOUND")?;
    stop_task(state, index, crate::now_ms());
    Ok(state.tasks.remove(index))
}

pub(crate) fn archive_project(state: &mut State, id: &str) -> Result<bool> {
    if !state.projects.iter().any(|p| p.id == id) {
        bail!("PROJECT_NOT_FOUND: {id}")
    }
    if id == crate::DEFAULT_PROJECT_ID {
        bail!("PROTECTED_PROJECT: Unassigned is the fallback project and cannot be removed")
    }
    if state.billing.archived_projects.contains(id) {
        return Ok(false);
    }
    let now = crate::now_ms();
    let indices: Vec<_> = state
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(index, t)| (t.project_id == id).then_some(index))
        .collect();
    for index in indices {
        stop_task(state, index, now);
    }
    state.billing.archived_projects.insert(id.into());
    state
        .billing
        .projects
        .get_mut(id)
        .context("PROJECT_NOT_FOUND: billing settings missing")?
        .cadence = "manual".into();
    state.billing.bindings.retain(|_, project| project != id);
    if state.active_project_id == id {
        state.active_project_id = crate::DEFAULT_PROJECT_ID.into();
    }
    Ok(true)
}

pub(crate) fn sync_client_name(state: &mut State, id: &str) {
    let name = state.billing.clients[id].name.clone();
    for project in &mut state.projects {
        if state
            .billing
            .projects
            .get(&project.id)
            .is_some_and(|p| p.client_id == id)
        {
            project.client_name = name.clone();
        }
    }
}

pub(crate) fn archive_client(state: &mut State, id: &str) -> Result<bool> {
    if !state.billing.clients.contains_key(id) {
        bail!("CLIENT_NOT_FOUND: {id}")
    }
    if state.billing.archived_clients.contains(id) {
        return Ok(false);
    }
    let projects: Vec<_> = state
        .projects
        .iter()
        .filter(|p| {
            !state.billing.archived_projects.contains(&p.id)
                && state
                    .billing
                    .projects
                    .get(&p.id)
                    .is_some_and(|b| b.client_id == id)
        })
        .map(|p| p.id.as_str())
        .collect();
    if !projects.is_empty() {
        bail!(
            "CLIENT_IN_USE: reassign or unlink the client from active projects first (project.configure with client set to an empty string): {}",
            projects.join(", ")
        )
    }
    Ok(state.billing.archived_clients.insert(id.into()))
}
