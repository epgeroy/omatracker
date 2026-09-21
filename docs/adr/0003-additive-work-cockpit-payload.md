# Add an additive work-cockpit payload

The compact status response keeps its existing `activeTasks` field and adds
`cockpitTasks` for the curated work-cockpit selection. This avoids silently
changing integrations that consume the existing open-task list while allowing
the widget to receive the product-specific curation reason with each cockpit
task.
