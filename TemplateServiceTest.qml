import QtQuick
import Quickshell
import Quickshell.Io

Item {
  id: root
  property string work: Quickshell.env("OMATRACKER_TEST_DIR")
  property int phase: 0
  property string openedPath: ""
  property bool failed: false

  function check(condition, message) {
    if (condition) return true
    failed = true
    console.error(message)
    Qt.quit()
    return false
  }

  Service {
    id: tracker
    feedbackEnabled: false
    dataPath: root.work + "/unused.json"
    backendCommand: ["sh", decodeURIComponent(String(Qt.resolvedUrl("tests/template-backend.sh")).slice(7)), root.work]
    // Exercise desktop-open dispatch without launching applications in tests.
    function openTemplateFile(path) { root.openedPath = path }
  }

  Timer {
    interval: 20
    repeat: true
    running: !root.failed
    onTriggered: {
      if (tracker.busy || !tracker.loaded || !tracker.diagnosticsReady) return
      if (root.phase === 0) {
        root.phase++
        tracker.refreshTemplates()
      } else if (root.phase === 1) {
        if (!root.check(tracker.templates.length === 2, "Template catalog was not loaded")) return
        root.phase++
        tracker.createTemplate("custom", "detailed", "project")
      } else if (root.phase === 2) {
        if (!root.check(tracker.activeProject.templateId === "user:custom"
          && root.openedPath === "/custom/template.typ", "Create did not select and open the custom template")) return
        root.phase++
        tracker.updateProject("project", { accentColor: "#abcdef", paper: "letter", logoPath: "/a logo.svg" })
      } else if (root.phase === 3) {
        if (!root.check(tracker.templateError === "", "Appearance flags did not reach backend")) return
        root.phase++
        tracker.previewTemplate("user:custom", "project")
      } else if (root.phase === 4) {
        if (!root.check(tracker.templateError.indexOf("syntax error") >= 0, "Preview error was not preserved")) return
        root.phase++
        tracker.refresh()
      } else if (root.phase === 5) {
        if (!root.check(tracker.templateError.indexOf("syntax error") >= 0 && tracker.loaded,
          "Status refresh erased template errors or disabled tracking")) return
        root.phase++
        tracker.previewTemplate("detailed", "project")
      } else if (root.phase === 6) {
        if (!root.check(tracker.templateError === "" && root.openedPath === "/preview.pdf", "Preview did not recover")) return
        root.phase++
        verification.running = true
      }
    }
  }

  Process {
    id: verification
    command: ["touch", root.work + "/templates-passed"]
    onExited: function(exitCode) {
      if (exitCode === 0) console.log("Template service workflow checks passed")
      Qt.quit()
    }
  }

  Timer {
    interval: 8000
    running: true
    onTriggered: {
      console.error("Template service checks timed out in phase " + root.phase + ": " + tracker.templateError)
      Qt.quit()
    }
  }
}
