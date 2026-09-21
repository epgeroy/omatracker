# Best-effort task catalogue continuation

Task-catalogue listings use opaque continuation cursors based on descending
Recent activity and a deterministic ID tie-breaker. We deliberately accept
best-effort continuation when tasks change during enumeration rather than
rejecting a continuation or retaining server-side snapshots, because task
catalogues are a current-work view and clients can requery cheaply.
