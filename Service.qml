import QtQuick
import Quickshell
import Quickshell.Io

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
  property bool configured: false

  property var state: emptyState()
  property var activeProject: null
  property var activeTasks: []
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
  property string backendError: ""
  property bool startupHandled: false

  property string currentAction: ""
  property var currentContext: ({})
  property var pendingActions: []
  property string processOutput: ""
  property string processError: ""
  property bool processStdoutFinished: false
  property bool processStderrFinished: false
  property bool processExited: false
  property int processExitCode: -1

  readonly property bool busy: currentAction !== ""
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
    pendingActions = []
    refresh()
  }

  function refresh() {
    enqueue("status", ["status", "--json"], {})
  }

  function enqueue(action, args, context) {
    if (action === "status") {
      if (currentAction === "status") return
      for (var i = 0; i < pendingActions.length; i++)
        if (pendingActions[i].action === "status") return
    }
    pendingActions = pendingActions.concat([{
      action: action,
      args: args,
      context: context || ({})
    }])
    startNextAction()
  }

  function startNextAction() {
    if (currentAction !== "" || pendingActions.length === 0) return
    var next = pendingActions[0]
    pendingActions = pendingActions.slice(1)
    currentAction = next.action
    currentContext = next.context
    processOutput = ""
    processError = ""
    processStdoutFinished = false
    processStderrFinished = false
    processExited = false
    processExitCode = -1
    commandProcess.command = ["sh", "-c", "exec \"$@\"", "omatracker", backendPath, "--data-path", dataPath].concat(next.args)
    commandProcess.running = true
    processTimeout.restart()
  }

  function finishProcess() {
    if (!processExited || !processStdoutFinished || !processStderrFinished || currentAction === "") return
    var action = currentAction
    var context = currentContext
    var exitCode = processExitCode
    var stdout = processOutput
    var stderr = processError
    currentAction = ""
    currentContext = ({})
    processTimeout.stop()
    handleProcess(action, context, exitCode, stdout, stderr)
  }

  function failCurrentProcess(message) {
    if (currentAction === "") return
    var action = currentAction
    var context = currentContext
    currentAction = ""
    currentContext = ({})
    commandProcess.running = false
    processTimeout.stop()
    handleProcess(action, context, -1, "", message)
  }

  function outputSummary(stdout, stderr) {
    var text = String(stderr || stdout || "").replace(/\s+/g, " ").trim()
    return text.length > 400 ? text.slice(0, 397) + "..." : text
  }

  function handleProcess(action, context, exitCode, stdout, stderr) {
    if (action === "status") {
      if (exitCode === 0) applyStatus(stdout)
      else applyBackendError(outputSummary(stdout, stderr))
    } else {
      if (exitCode !== 0) backendError = outputSummary(stdout, stderr) || "OmaTracker command failed"
      enqueue("status", ["status", "--json"], {})
    }
    startNextAction()
  }

  function applyStatus(raw) {
    try {
      var next = JSON.parse(raw)
      if (!next || !next.state) throw new Error("status output has no state")
      state = next.state
      activeProject = next.activeProject || null
      activeTasks = Array.isArray(next.activeTasks) ? next.activeTasks : []
      totalTrackedSeconds = Math.max(0, Math.floor(Number(next.totalTrackedSeconds) || 0))
      activeProjectSeconds = Math.max(0, Math.floor(Number(next.activeProjectSeconds) || 0))
      runningTimers = Math.max(0, Math.floor(Number(next.runningTimers) || 0))
      activeProjectRunningTimers = 0
      for (var i = 0; i < activeTasks.length; i++)
        if (activeTasks[i].running === true) activeProjectRunningTimers++
      nowMs = Math.max(0, Number(next.nowMs) || Date.now())
      statusSnapshotMs = nowMs
      setupStatus = String(next.setupStatus || "OmaTracker backend is ready")
      reportStatus = String(next.reportStatus || "No PDF reports queued")
      syncStatus = String(next.syncStatus || "Not synced yet")
      syncError = String(next.syncError || "")
      backgroundChecksEnabled = next.backgroundChecksEnabled === true
      backendError = ""
      loaded = true
      if (!startupHandled) {
        startupHandled = true
        enqueue("report-check", ["report", "check"], {})
        if (state.drive && state.drive.syncOnStartup === true) enqueue("sync", ["sync"], {})
      }
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

  function addTask() {
    enqueue("task-add", ["task", "add", "Empty"], {})
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

  Process {
    id: commandProcess

    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.processOutput = String(text || "")
        root.processStdoutFinished = true
        root.finishProcess()
      }
    }

    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.processError = String(text || "")
        root.processStderrFinished = true
        root.finishProcess()
      }
    }

    onExited: function(exitCode) {
      root.processExitCode = exitCode
      root.processExited = true
      root.finishProcess()
    }
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
    onTriggered: root.enqueue("report-check", ["report", "check"], {})
  }

  Timer {
    id: processTimeout
    interval: 120000
    repeat: false
    onTriggered: root.failCurrentProcess("OmaTracker command timed out after two minutes")
  }
}
