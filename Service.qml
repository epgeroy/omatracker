import QtQuick
import Quickshell

// The Rust CLI owns all durable state, report generation, and Drive uploads.
// This service only serializes UI requests and keeps a presentation snapshot
// for every bar instance.
Item {
  id: root

  property var manifest: null
  property var shell: null

  readonly property string home: Quickshell.env("HOME") || ""
  readonly property string sourcePath: localFilePath(Qt.resolvedUrl("."))
  readonly property string backendPath: sourcePath + "/bin/omatracker"
  property string dataPath: home + "/.config/omarchy/omatracker.json"
  property var backendCommand: [backendPath, "--data-path", dataPath]
  property bool configured: false

  property var state: emptyState()
  property var activeProject: null
  property var activeTasks: []
  property var runningTasks: []
  property var preferences: ({ hourlyClick: true, volume: 25, reducedMotion: false })
  property bool feedbackEnabled: true
  property string feedbackError: ""
  signal actionFinished(string action, bool success)
  signal hourReached(int hours)
  property bool loaded: false
  property real nowMs: Date.now()
  property real statusSnapshotMs: nowMs
  property int totalTrackedSeconds: 0
  property int activeProjectSeconds: 0
  property int runningTimers: 0
  property int activeProjectRunningTimers: 0
  property string setupStatus: "Checking OmaTracker backend"
  property string reportStatus: "No PDF reports queued"
  property string syncStatus: "Not synced yet"
  property string syncError: ""
  property bool backgroundChecksEnabled: false
  property bool backgroundChecksActive: false
  property string backendError: ""
  property bool startupHandled: false
  property bool diagnosticsReady: false

  readonly property bool busy: foregroundQueue.busy || backgroundQueue.busy
  readonly property bool anyRunning: runningTimers > 0
  readonly property int elapsedSinceStatus: Math.max(0, Math.floor((nowMs - statusSnapshotMs) / 1000))
  readonly property int displayTotalSeconds: totalTrackedSeconds + runningTimers * elapsedSinceStatus
  readonly property int displayActiveProjectSeconds: activeProjectSeconds + activeProjectRunningTimers * elapsedSinceStatus
  readonly property string totalText: formatDuration(displayTotalSeconds)
  readonly property string activeProjectText: formatDuration(displayActiveProjectSeconds)

  function emptyState() {
    return {
      version: 2,
      activeProjectId: "",
      projects: [],
      tasks: [],
      entries: [],
      reports: [],
      drive: { remote: "", folder: "OmaTracker", syncOnStartup: false },
      sync: { status: "Not synced yet", error: "", lastSyncedAt: 0 }
    }
  }

  function localFilePath(url) {
    var value = String(url || "")
    if (value.indexOf("file://") !== 0) return ""
    value = decodeURIComponent(value.slice(7))
    return value.length > 1 && value.charAt(value.length - 1) === "/"
      ? value.slice(0, -1) : value
  }

  function formatDuration(seconds) {
    var total = Math.max(0, Math.floor(Number(seconds) || 0))
    var hours = Math.floor(total / 3600)
    var minutes = Math.floor((total % 3600) / 60)
    function pad(value) { return value < 10 ? "0" + value : String(value) }
    return pad(hours) + ":" + pad(minutes) + ":" + pad(total % 60)
  }

  function displayTaskSeconds(task) {
    if (!task) return 0
    return Math.max(0, Math.floor(Number(task.displaySeconds) || 0))
      + (task.running === true ? elapsedSinceStatus : 0)
  }

  function configure(path) {
    var next = String(path || "")
    if (next.indexOf("~/") === 0) next = home + next.slice(1)
    if (next === "" || (configured && next === dataPath)) return
    dataPath = next
    configured = true
    loaded = false
    startupHandled = false
    diagnosticsReady = false
    foregroundQueue.reset()
    backgroundQueue.reset()
    feedbackQueue.reset()
    refresh()
    enqueue("diagnostics", ["diagnostics"], {})
  }

  function refresh() {
    enqueue("status", ["status", "--json", "--compact"], {})
  }

  function enqueue(action, args, context) {
    var background = action === "sync" || action === "diagnostics" || action.indexOf("report-") === 0
    var queue = background ? backgroundQueue : foregroundQueue
    queue.enqueue(action, args, context)
  }

  function outputSummary(stdout, stderr) {
    var text = String(stderr || stdout || "").replace(/\s+/g, " ").trim()
    return text.length > 400 ? text.slice(0, 397) + "..." : text
  }

  function handleProcess(action, context, exitCode, stdout, stderr) {
    if (action === "status") {
      if (exitCode === 0) applyStatus(stdout)
      else applyBackendError(outputSummary(stdout, stderr))
    } else if (action === "diagnostics") {
      if (exitCode === 0) applyDiagnostics(stdout, context)
      else setupStatus = outputSummary(stdout, stderr) || "Could not check OmaTracker setup"
    } else {
      backendError = exitCode !== 0 ? outputSummary(stdout, stderr) || "OmaTracker command failed" : ""
      actionFinished(action, exitCode === 0)
      refresh()
      if (action === "report-timer") enqueue("diagnostics", ["diagnostics"], { checkReports: true })
    }
  }

  function handleStartup() {
    if (startupHandled || !loaded || !diagnosticsReady) return
    startupHandled = true
    if (!backgroundChecksActive) enqueue("report-check", ["report", "check"], {})
    if (state.drive && state.drive.syncOnStartup === true) enqueue("sync", ["sync"], {})
  }

  function applyDiagnostics(raw, context) {
    try {
      var next = JSON.parse(raw)
      setupStatus = String(next.setupStatus || "OmaTracker backend is ready")
      backgroundChecksEnabled = next.backgroundChecksEnabled === true
      backgroundChecksActive = next.backgroundChecksActive === true
      diagnosticsReady = true
      var alreadyStarted = startupHandled
      handleStartup()
      if (alreadyStarted && context.checkReports) {
        if (!backgroundChecksActive) enqueue("report-check", ["report", "check"], {})
        else refresh() // Pick up reports completed by the systemd worker.
      }
    } catch (error) {
      setupStatus = "Could not read OmaTracker setup: " + error
    }
  }

  function applyStatus(raw) {
    try {
      var next = JSON.parse(raw)
      if (!next || !next.state) throw new Error("status output has no state")
      state = next.state
      activeProject = next.activeProject || null
      activeTasks = Array.isArray(next.activeTasks) ? next.activeTasks : []
      runningTasks = Array.isArray(next.runningTasks) ? next.runningTasks : []
      preferences = next.preferences || ({ hourlyClick: true, volume: 25, reducedMotion: false })
      totalTrackedSeconds = Math.max(0, Math.floor(Number(next.totalTrackedSeconds) || 0))
      activeProjectSeconds = Math.max(0, Math.floor(Number(next.activeProjectSeconds) || 0))
      runningTimers = Math.max(0, Math.floor(Number(next.runningTimers) || 0))
      activeProjectRunningTimers = 0
      for (var i = 0; i < activeTasks.length; i++)
        if (activeTasks[i].running === true) activeProjectRunningTimers++
      nowMs = Math.max(0, Number(next.nowMs) || Date.now())
      statusSnapshotMs = nowMs
      reportStatus = String(next.reportStatus || "No PDF reports queued")
      syncStatus = String(next.syncStatus || "Not synced yet")
      syncError = String(next.syncError || "")
      if (!loaded) backendError = ""
      loaded = true
      handleStartup()
    } catch (error) {
      applyBackendError("Could not read OmaTracker status: " + error)
    }
  }

  function applyBackendError(message) {
    backendError = message === ""
      ? "OmaTracker backend is unavailable at " + backendPath
      : message
    setupStatus = "OmaTracker backend unavailable"
    syncStatus = backendError
    syncError = backendError
    loaded = false
  }

  function selectProject(id) {
    enqueue("project-select", ["project", "select", id], {})
  }

  function createProject(name) {
    enqueue("project-create", ["project", "create", String(name || "New project")], {})
  }

  function updateProject(id, changes) {
    var args = ["project", "update", id]
    if (changes.name !== undefined) args.push("--name", String(changes.name))
    if (changes.clientName !== undefined) args.push("--client-name", String(changes.clientName))
    if (changes.companyName !== undefined) args.push("--company-name", String(changes.companyName))
    if (changes.templateId !== undefined) args.push("--template-id", String(changes.templateId))
    if (changes.exportWeekly !== undefined) args.push("--export-weekly", String(changes.exportWeekly))
    if (changes.exportMonthly !== undefined) args.push("--export-monthly", String(changes.exportMonthly))
    enqueue("project-update", args, {})
  }

  function addTask(title) {
    enqueue("task-add", ["task", "add", String(title || "Empty")], {})
  }

  function updatePreferences(hourlyClick, volume, reducedMotion) {
    enqueue("preferences", ["feedback", "configure", "--hourly-click", String(hourlyClick),
      "--volume", String(Math.round(volume)), "--reduced-motion", String(reducedMotion)], {})
  }

  function previewClick(volume) {
    if (sound.item) sound.item.play(volume)
    else feedbackError = "Audio is unavailable. Check Qt Multimedia and your audio output."
  }

  function startTimer(id) {
    enqueue("task-start", ["task", "start", id], {})
  }

  function stopTimer(id) {
    enqueue("task-stop", ["task", "stop", id], {})
  }

  function resetTimer(id) {
    enqueue("task-reset", ["task", "reset", id], {})
  }

  function resetActiveProject() {
    enqueue("task-reset-project", ["task", "reset-active-project"], {})
  }

  function removeTask(id) {
    enqueue("task-remove", ["task", "remove", id], {})
  }

  function renameAndAddManualTime(id, title, duration) {
    var args = ["task", "edit", id, "--title", String(title || "")]
    if (String(duration || "").trim() !== "") args.push("--add", String(duration))
    enqueue("task-edit", args, {})
  }

  function updateDrive(remote, folder, syncOnStartup) {
    enqueue("drive-update", [
      "drive", "update", "--remote", String(remote || ""), "--folder", String(folder || "OmaTracker"),
      "--sync-on-startup", String(syncOnStartup === true)
    ], {})
  }

  function requestSync() {
    enqueue("sync", ["sync"], {})
  }

  function requestExport(period) {
    if (period === "weekly" || period === "monthly")
      enqueue("report-export", ["report", "export", period], {})
  }

  function retryReports() {
    enqueue("report-retry", ["report", "retry"], {})
  }

  function setBackgroundChecks(enabled) {
    enqueue("report-timer", ["service", enabled === true ? "install" : "remove"], {})
  }

  Component.onCompleted: root.configure(root.dataPath)

  BackendQueue {
    id: foregroundQueue
    commandPrefix: root.backendCommand
    onCompleted: function(action, context, exitCode, stdout, stderr) {
      root.handleProcess(action, context, exitCode, stdout, stderr)
    }
  }

  BackendQueue {
    id: backgroundQueue
    commandPrefix: root.backendCommand
    paused: foregroundQueue.busy || foregroundQueue.pendingActions.length > 0
    // A backlog can legitimately take longer than two minutes. It no longer
    // blocks controls, so let the backend finish instead of killing a batch.
    timeoutMs: 0
    onCompleted: function(action, context, exitCode, stdout, stderr) {
      root.handleProcess(action, context, exitCode, stdout, stderr)
    }
  }

  // Independent of the popup and report lane. The backend atomically claims a
  // milestone, so duplicate widgets cannot play the same hour twice.
  BackendQueue {
    id: feedbackQueue
    commandPrefix: root.backendCommand
    timeoutMs: 5000
    onCompleted: function(action, context, exitCode, stdout, stderr) {
      if (exitCode !== 0) { root.feedbackError = root.outputSummary(stdout, stderr); return }
      try {
        var claim = JSON.parse(stdout)
        root.feedbackError = ""
        if (claim.play) {
          root.previewClick(claim.volume)
          root.hourReached(claim.hours)
        }
      } catch (error) { root.feedbackError = "Could not read hourly feedback: " + error }
    }
  }

  Loader {
    id: sound
    active: root.feedbackEnabled
    source: "HourlySound.qml"
    onStatusChanged: if (status === Loader.Error) root.feedbackError = "Qt Multimedia audio could not be loaded"
  }
  Connections {
    target: sound.item
    function onFailed(message) { root.feedbackError = message }
  }

  Timer {
    interval: 10000
    repeat: true
    running: root.loaded && root.feedbackEnabled
    triggeredOnStart: true
    onTriggered: if (!feedbackQueue.busy) feedbackQueue.enqueue("feedback", ["feedback", "poll"], {})
  }

  // Pick up CLI changes and timers started on another output, even when idle.
  Timer {
    interval: 5000
    repeat: true
    running: root.loaded
    onTriggered: if (!foregroundQueue.busy) root.refresh()
  }

  Timer {
    interval: 1000
    repeat: true
    running: root.anyRunning
    triggeredOnStart: true
    onTriggered: root.nowMs = Date.now()
  }

  // This is merely a trigger. The Rust CLI decides what reports to queue,
  // render, upload, or leave retryable.
  Timer {
    interval: 15 * 60 * 1000
    repeat: true
    running: root.loaded
    onTriggered: root.enqueue("diagnostics", ["diagnostics"], { checkReports: true })
  }
}
