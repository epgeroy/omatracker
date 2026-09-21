# OmaTracker

OmaTracker records client work and its billable time. It distinguishes the
small set of tasks a person is working with now from their larger task history.

## Language

**Work cockpit**:
The selected project's focused view of Tracking tasks, recently created tasks,
and recently tracked Stopped tasks.
_Avoid_: task list, dashboard

**Task catalogue**:
The browsable project-scoped collection of Stopped, Tracking, and Done tasks.
_Avoid_: history, archive

**Completed task**:
A task in the Done lifecycle state, retained as part of the task catalogue.
_Avoid_: archived task, deleted task

**Open task**:
A task in either the Stopped or Tracking lifecycle state.
_Avoid_: active task, unfinished task

**Last tracked time**:
The end of a task's most recent live Tracking interval. It ranks Stopped tasks
in the work cockpit.
_Avoid_: last activity, last edited

**Recent activity**:
The most recent user-visible task change, including creation, lifecycle, task
detail, manual-entry, and correction changes. It orders task-catalogue results.
_Avoid_: last tracked time

**Never-tracked task**:
A task that has not had a live Tracking interval. It may still have manually
recorded time.
_Avoid_: new task, empty task

**Cockpit recency**:
The timestamp used to choose a Stopped task for the work cockpit: creation time
for a Never-tracked task, otherwise Last tracked time.
_Avoid_: recent activity
