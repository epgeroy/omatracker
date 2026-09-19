import QtQuick
import Quickshell
import Quickshell.Io

// Real external CLI mutations must reach an idle widget through its regular poll.
Item {
  id: root
  property string work: Quickshell.env("OMATRACKER_TEST_DIR")
  property string projectId: Quickshell.env("OMATRACKER_REFRESH_PROJECT")
  property string clientId: Quickshell.env("OMATRACKER_REFRESH_CLIENT")
  property string taskId: Quickshell.env("OMATRACKER_REFRESH_TASK")
  property int phase: 0
  property bool failed: false

  Service {
    id: tracker
    feedbackEnabled: false
    dataPath: root.work + "/external.json"
  }

  BackendQueue {
    id: external
    commandPrefix: [tracker.backendPath, "--data-path", tracker.dataPath]
    onCompleted: function(action, context, exitCode, stdout, stderr) {
      if (exitCode !== 0) {
        root.failed = true
        console.error("External CLI mutation failed: " + stdout + stderr)
        Qt.quit()
      }
      // Deliberately do not call tracker.refresh(): the normal poll must observe it.
    }
  }

  Timer {
    interval: 40
    repeat: true
    running: !root.failed
    onTriggered: {
      if (!tracker.loaded || !tracker.diagnosticsReady || tracker.busy || external.busy) return
      if (root.phase === 0 && tracker.activeProject.id === root.projectId
        && tracker.activeTasks.some(function(task) { return task.id === root.taskId })) {
        root.phase = 1
        external.enqueue("task-delete", ["agent", "task.remove", "--input", JSON.stringify({id: root.taskId})], {})
      } else if (root.phase === 1 && tracker.activeTasks.length === 0) {
        root.phase = 2
        external.enqueue("project-delete", ["agent", "project.remove", "--input", JSON.stringify({project: root.projectId})], {})
        external.enqueue("client-delete", ["agent", "client.remove", "--input", JSON.stringify({id: root.clientId})], {})
      } else if (root.phase === 2 && tracker.activeProject.id === "project-unassigned"
        && !tracker.state.projects.some(function(project) { return project.id === root.projectId })) {
        root.phase = 3
        tracker.applyBackendError("Simulated temporary backend timeout")
      } else if (root.phase === 3 && tracker.loaded && tracker.backendError === "") {
        root.phase = 4
        marker.running = true
      }
    }
  }

  Process {
    id: marker
    command: ["touch", root.work + "/external-refresh-passed"]
    onExited: function(exitCode) {
      if (exitCode === 0) console.log("External CLI deletion and temporary backend failure recovery checks passed")
      Qt.quit()
    }
  }

  Timer {
    interval: 20000
    running: true
    onTriggered: {
      console.error("External refresh timed out in phase " + root.phase + ": " + tracker.backendError)
      Qt.quit()
    }
  }
}
