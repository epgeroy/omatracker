import QtQuick
import Quickshell
import Quickshell.Io

// Keep the entry point at the plugin root: Quickshell confines local type
// discovery to the configuration directory.

Item {
  id: root
  property string work: Quickshell.env("OMATRACKER_TEST_DIR")
  property var fakeBackend: ["sh", decodeURIComponent(String(Qt.resolvedUrl("tests/fake-backend.sh")).slice(7)), work]
  property int phase: 0
  property int snapshots: 0
  property bool queueDone: false

  Service {
    id: tracker
    dataPath: root.work + "/unused-ledger.json"
    backendCommand: root.fakeBackend
  }

  BackendQueue {
    id: refreshQueue
    commandPrefix: ["sh", "-c", "sleep 0.05; printf snapshot"]
    onCompleted: function(action, context, exitCode, stdout, stderr) {
      if (exitCode !== 0 || stdout !== "snapshot") {
        console.error("Status queue failed: " + stderr)
        Qt.quit()
        return
      }
      root.snapshots++
      root.queueDone = root.snapshots === 2
    }
  }

  Component.onCompleted: {
    refreshQueue.enqueue("status", [], {})
    refreshQueue.enqueue("status", [], {})
    refreshQueue.enqueue("status", [], {})
  }

  Timer {
    interval: 30
    repeat: true
    running: true
    onTriggered: {
      if (root.phase === 0 && tracker.loaded && tracker.diagnosticsReady && !tracker.busy) {
        root.phase = 1
        tracker.requestSync()
      } else if (root.phase === 1) {
        if (tracker.syncStatus === "uploading") {
          root.phase = 2
          tracker.stopTimer("tracked")
        } else tracker.refresh()
      } else if (root.phase === 2 && tracker.runningTimers === 0
        && tracker.syncStatus === "synced" && !tracker.busy && root.queueDone) {
        root.phase = 3
        // A periodic diagnostic result must also refresh externally changed
        // ledger state when systemd owns report scheduling.
        tracker.syncStatus = "stale"
        tracker.applyDiagnostics(JSON.stringify({ setupStatus: "Ready",
          backgroundChecksEnabled: true, backgroundChecksActive: true }), { checkReports: true })
      } else if (root.phase === 3 && tracker.syncStatus === "synced" && !tracker.busy) {
        root.phase = 4
        verification.running = true
      }
    }
  }

  Process {
    id: verification
    command: root.fakeBackend.concat(["verify"])
    onExited: function(exitCode) {
      if (exitCode === 0 && root.snapshots === 2) console.log("Service concurrency and refresh checks passed")
      else console.error("Service checks failed")
      Qt.quit()
    }
  }

  Timer {
    interval: 8000
    running: true
    onTriggered: {
      console.error("Service checks timed out in phase " + root.phase + ": " + tracker.backendError)
      Qt.quit()
    }
  }
}
