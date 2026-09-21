# Preserve legacy task-list pagination

The new cursor-based task-catalogue contract coexists with the existing
`agent task.list` `running: true` offset mode. Existing automation keeps its
bounded, familiar response shape while new consumers opt into lifecycle state
filters and opaque continuation cursors.
