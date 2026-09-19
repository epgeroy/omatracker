import QtQuick
import Quickshell.Io

// One serialized command lane. Separate instances let durable background work
// proceed without holding up time-sensitive task commands.
Item {
  id: root

  property var commandPrefix: []
  property int timeoutMs: 120000
  property bool paused: false
  readonly property bool busy: currentAction !== ""
  property string currentAction: ""
  property var currentContext: ({})
  property var pendingActions: []
  property int generation: 0
  property int currentGeneration: 0
  property string processOutput: ""
  property string processError: ""
  property bool stdoutFinished: false
  property bool stderrFinished: false
  property bool exited: false
  property bool timedOut: false
  property int exitCode: -1

  signal completed(string action, var context, int exitCode, string stdout, string stderr)

  onPausedChanged: if (!paused) startNextAction()

  function reset() {
    generation++
    pendingActions = []
    // Let an in-flight mutation finish against its original ledger, but discard
    // its result. New requests capture the new command prefix when enqueued.
  }

  function enqueue(action, args, context) {
    if (["status", "diagnostics", "report-check", "sync"].indexOf(action) >= 0) {
      // A status already in flight may predate a background mutation. Retain
      // one follow-up refresh, rather than silently losing that invalidation.
      if (action !== "status" && currentAction === action && currentGeneration === generation) return
      for (var i = 0; i < pendingActions.length; i++)
        if (pendingActions[i].action === action) return
    }
    pendingActions = pendingActions.concat([{
      action: action,
      command: commandPrefix.concat(args),
      context: context || ({}),
      generation: generation
    }])
    startNextAction()
  }

  function startNextAction() {
    if (paused || busy || pendingActions.length === 0) return
    var next = pendingActions[0]
    currentAction = next.action
    pendingActions = pendingActions.slice(1)
    currentContext = next.context
    currentGeneration = next.generation
    processOutput = ""
    processError = ""
    stdoutFinished = false
    stderrFinished = false
    exited = false
    timedOut = false
    exitCode = -1
    commandProcess.command = ["sh", "-c", "exec \"$@\"", "omatracker"].concat(next.command)
    commandProcess.running = true
    if (timeoutMs > 0) processTimeout.restart()
  }

  function finishProcess() {
    if (!exited || !stdoutFinished || !stderrFinished || !busy) return
    var action = currentAction
    var context = currentContext
    var code = timedOut ? -1 : exitCode
    var stdout = processOutput
    var stderr = timedOut ? "OmaTracker command timed out" : processError
    var deliver = currentGeneration === generation
    currentAction = ""
    currentContext = ({})
    processTimeout.stop()
    if (deliver) completed(action, context, code, stdout, stderr)
    startNextAction()
  }

  Process {
    id: commandProcess
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.processOutput = String(text || "")
        root.stdoutFinished = true
        root.finishProcess()
      }
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.processError = String(text || "")
        root.stderrFinished = true
        root.finishProcess()
      }
    }
    onExited: function(exitCode) {
      root.exitCode = exitCode
      root.exited = true
      root.finishProcess()
    }
  }

  Timer {
    id: processTimeout
    interval: Math.max(1, root.timeoutMs)
    onTriggered: {
      root.timedOut = true
      commandProcess.running = false
      // Wait for exit and both collectors before reusing the Process object.
    }
  }
}
