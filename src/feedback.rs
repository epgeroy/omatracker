//! Local presentation preferences and an atomic hourly-notification claim.
//! Kept beside (not inside) the uploaded ledger; each machine owns its audio.
use crate::{State, atomic_write, lock_file, now_ms, read_state};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub hourly_click: bool,
    pub volume: u8,
    pub reduced_motion: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            hourly_click: true,
            volume: 25,
            reduced_motion: false,
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct Checkpoint {
    preferences: Preferences,
    last_checked_at: Option<i64>,
    claimed_hours: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub play: bool,
    pub hours: i64,
    pub volume: u8,
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".feedback.json");
    PathBuf::from(name)
}

fn read_checkpoint(path: &Path) -> Result<Checkpoint> {
    match fs::read(sidecar(path)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Checkpoint::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn preferences(path: &Path) -> Result<Preferences> {
    Ok(read_checkpoint(path)?.preferences)
}

/// Reset work milestones while retaining preferences. Caller holds the ledger lock.
pub(crate) fn clear_history(path: &Path) -> Result<()> {
    let checkpoint = Checkpoint {
        preferences: read_checkpoint(path)?.preferences,
        ..Default::default()
    };
    atomic_write(&sidecar(path), &serde_json::to_vec(&checkpoint)?)
}

pub fn configure(path: &Path, hourly_click: bool, volume: u8, reduced_motion: bool) -> Result<()> {
    if volume > 100 {
        bail!("volume must be between 0 and 100")
    }
    let _lock = lock_file(path)?;
    let mut checkpoint = read_checkpoint(path)?;
    if checkpoint.preferences.hourly_click != hourly_click {
        checkpoint.last_checked_at = None;
    }
    checkpoint.preferences = Preferences {
        hourly_click,
        volume,
        reduced_motion,
    };
    atomic_write(&sidecar(path), &serde_json::to_vec(&checkpoint)?)
}

/// Union of real timer intervals, retaining fractional seconds across pauses.
/// Visible resets, deleted tasks and project switches do not erase ledger time.
pub fn tracked_millis(state: &State, now: i64) -> i64 {
    let mut intervals: Vec<_> = state
        .entries
        .iter()
        .filter(|entry| entry.note != "Manual entry")
        .map(|entry| (entry.started_at, entry.ended_at))
        .chain(
            state
                .tasks
                .iter()
                .filter(|task| task.is_tracking())
                .map(|task| (task.started_at, now)),
        )
        .map(|(start, end)| (start.max(0), end.min(now)))
        .filter(|(start, end)| end > start)
        .collect();
    intervals.sort_unstable();
    let mut end = 0;
    let mut total = 0_i64;
    for (start, next_end) in intervals {
        if next_end > end {
            total = total.saturating_add(next_end - start.max(end));
            end = next_end;
        }
    }
    total
}

fn advance(checkpoint: &mut Checkpoint, work_ms: i64, now: i64) -> Claim {
    let hours = work_ms / 3_600_000;
    // A stale worker or resume must not replay old milestones. A persisted high
    // water mark also suppresses duplicates after a clock rollback or data import.
    let live = checkpoint
        .last_checked_at
        .is_some_and(|last| (0..=30_000).contains(&(now - last)));
    let play = live && hours > checkpoint.claimed_hours && checkpoint.preferences.hourly_click;
    checkpoint.claimed_hours = checkpoint.claimed_hours.max(hours);
    checkpoint.last_checked_at = Some(now);
    Claim {
        play,
        hours,
        volume: checkpoint.preferences.volume,
    }
}

pub fn poll(path: &Path) -> Result<Claim> {
    let _lock = lock_file(path)?;
    let state = read_state(path)?;
    let now = now_ms();
    let mut checkpoint = read_checkpoint(path)?;
    let claim = advance(&mut checkpoint, tracked_millis(&state, now), now);
    atomic_write(&sidecar(path), &serde_json::to_vec(&checkpoint)?)?;
    Ok(claim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Task};

    #[test]
    fn union_counts_overlap_once_and_excludes_manual_and_legacy_time() {
        let mut state = State {
            entries: vec![
                Entry {
                    started_at: 1_000,
                    ended_at: 601_000,
                    ..Entry::default()
                },
                Entry {
                    started_at: 301_000,
                    ended_at: 901_000,
                    ..Entry::default()
                },
                Entry {
                    started_at: 901_000,
                    ended_at: 3_601_000,
                    note: "Manual entry".into(),
                    ..Entry::default()
                },
            ],
            tasks: vec![Task {
                status: crate::TaskStatus::Tracking,
                started_at: 1_201_000,
                display_since: 1_501_000,
                legacy_seconds: 9999,
                ..Task::default()
            }],
            ..State::default()
        };
        assert_eq!(tracked_millis(&state, 1_801_000), 1_500_000);
        // Closing a running interval (or deleting/resetting its task) preserves work.
        state.entries.push(Entry {
            started_at: 1_201_000,
            ended_at: 1_801_000,
            ..Entry::default()
        });
        state.tasks.clear();
        assert_eq!(tracked_millis(&state, 2_000_000), 1_500_000);
    }

    #[test]
    fn milestone_claim_survives_restart_and_is_not_replayed() {
        let mut c = Checkpoint::default();
        assert!(!advance(&mut c, 3_599_000, 10_000).play);
        assert!(advance(&mut c, 3_600_000, 11_000).play);
        // Another panel reads the same committed checkpoint.
        let mut c: Checkpoint = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert!(!advance(&mut c, 3_600_000, 11_001).play);
        assert!(!advance(&mut c, 7_200_000, 90_000).play);
        assert!(!advance(&mut c, 3_600_000, 80_000).play);
        assert!(!advance(&mut c, 7_200_000, 90_001).play);
        assert!(advance(&mut c, 10_800_000, 100_000).play);
    }

    #[test]
    fn muted_milestones_do_not_backlog_and_pauses_retain_progress() {
        let mut c = Checkpoint::default();
        advance(&mut c, 3_599_500, 1000);
        advance(&mut c, 3_599_500, 20_000);
        assert!(advance(&mut c, 3_600_000, 21_000).play);
        c.preferences.hourly_click = false;
        assert!(!advance(&mut c, 7_200_000, 22_000).play);
        c.preferences.hourly_click = true;
        assert!(!advance(&mut c, 7_200_000, 23_000).play);
    }
}
