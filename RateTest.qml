import QtQuick
import Quickshell
import Quickshell.Io
import "RateModel.js" as RateModel

// Exercise QML -> CLI -> ledger -> presentation in a disposable home.
Item {
  id: root
  property string work: Quickshell.env("OMATRACKER_TEST_DIR")
  property int phase: 0
  property string firstProjectId: ""
  property string taskId: ""
  property bool failed: false

  function check(condition, message) {
    if (condition) return true
    root.failed = true
    console.error("Rate test: " + message)
    Qt.quit()
    return false
  }

  Service {
    id: tracker
    feedbackEnabled: false
    dataPath: root.work + "/rates.json"
  }

  Component.onCompleted: {
    var usd = { currency: "USD", fractionDigits: 2, amountText: "USD fallback" }
    check(RateModel.estimateText({ amountMinor: 8000 }, usd, 5400) === "USD 120.00", "90 minutes")
    check(RateModel.estimateText({ amountMinor: 1 }, usd, 1799) === "USD 0.00", "below half-cent")
    check(RateModel.estimateText({ amountMinor: 1 }, usd, 1800) === "USD 0.01", "half-cent")
    check(RateModel.estimateText({ amountMinor: 0 }, usd, 5400) === "USD 0.00", "zero rate")
    check(RateModel.estimateText({ amountMinor: 1000000000 }, usd, 2147483647) === "USD 5965232352777.78", "large exact estimate")
    check(RateModel.estimateText({ amountMinor: 1000000000 }, usd, 1e15) === "USD fallback", "unsafe integer fallback")
    check(RateModel.estimateText({ amountMinor: 125 }, {currency: "JPY", fractionDigits: 0}, 1800) === "JPY 63", "JPY rounding")
    check(RateModel.estimateText({ amountMinor: 1001 }, {currency: "KWD", fractionDigits: 3}, 1800) === "KWD 0.501", "KWD rounding")
    check(RateModel.estimateText(null, null, 5400) === "", "unset rate")
  }

  Timer {
    interval: 30
    repeat: true
    running: !root.failed
    onTriggered: {
      if (!tracker.loaded || !tracker.diagnosticsReady || tracker.busy) return
      if (root.phase === 0) {
        root.firstProjectId = tracker.activeProject.id
        root.phase++
        tracker.updateProject(root.firstProjectId, {hourlyRate: "80", currency: "USD"})
      } else if (root.phase === 1) {
        if (!root.check(tracker.activeProjectEstimate && tracker.activeProjectEstimate.hourlyRate === "80.00", "saved rate")) return
        root.phase++
        tracker.addTask()
      } else if (root.phase === 2) {
        if (!root.check(tracker.activeTasks.length === 1, "created task")) return
        root.taskId = tracker.activeTasks[0].id
        root.phase++
        tracker.renameAndAddManualTime(root.taskId, "Rate smoke test", "1h30m")
      } else if (root.phase === 3) {
        if (!root.check(tracker.activeProjectAmountText === "USD 120.00", "manual time estimate")) return
        tracker.activeProjectRunningTimers = 2
        tracker.nowMs = tracker.statusSnapshotMs + 30000
        if (!root.check(tracker.activeProjectAmountText === "USD 121.33", "two live timers")) return
        root.phase++
        tracker.updateProject(root.firstProjectId, {hourlyRate: "bad", currency: "USD"})
      } else if (root.phase === 4) {
        if (!root.check(tracker.projectUpdateError !== "" && tracker.activeProjectEstimate.hourlyRate === "80.00", "invalid save error survives refresh")) return
        root.phase++
        tracker.refresh()
      } else if (root.phase === 5) {
        if (!root.check(tracker.projectUpdateError !== "", "error survives another refresh")) return
        root.phase++
        tracker.createProject("Second project")
      } else if (root.phase === 6) {
        if (!root.check(tracker.activeProject.id !== root.firstProjectId && tracker.activeProjectEstimate === null && tracker.projectUpdateError === "", "switch to unrated project")) return
        root.phase++
        tracker.updateProject(tracker.activeProject.id, {hourlyRate: "0", currency: "KWD"})
      } else if (root.phase === 7) {
        if (!root.check(tracker.activeProjectAmountText === "KWD 0.000", "zero KWD rate")) return
        root.phase++
        tracker.selectProject(root.firstProjectId)
      } else if (root.phase === 8) {
        if (!root.check(tracker.activeProjectAmountText === "USD 120.00", "switch back restores estimate")) return
        root.phase++
        tracker.updateProject(root.firstProjectId, {clearRate: true})
      } else if (root.phase === 9) {
        if (!root.check(tracker.activeProjectEstimate === null && tracker.activeProjectAmountText === "", "clear rate")) return
        root.phase++
        marker.running = true
      }
    }
  }

  Process {
    id: marker
    command: ["touch", root.work + "/rates-passed"]
    onExited: function(exitCode) {
      if (exitCode === 0) console.log("Project rate service and live estimate checks passed")
      Qt.quit()
    }
  }

  Timer {
    interval: 10000
    running: true
    onTriggered: {
      console.error("Rate checks timed out in phase " + root.phase + ": " + tracker.backendError)
      Qt.quit()
    }
  }
}
