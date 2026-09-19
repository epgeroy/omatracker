import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as App

FloatingWindow {
  id: window
  property bool milestoneObserved: false
  visible: true
  title: "OmaTracker · disposable preview"
  implicitWidth: Style.space(460)
  implicitHeight: Math.min(Style.space(680), view.implicitHeight + Style.space(32))
  color: Color.popups.background
  onVisibleChanged: if (!visible) Qt.quit()
  App.Service {
    id: backend
    dataPath: Quickshell.env("OMATRACKER_UI_LEDGER")
  }
  App.TrackerView {
    id: view
    anchors.fill: parent
    anchors.margins: Style.space(16)
    tracker: backend
    onCloseRequested: Qt.quit()
    Component.onCompleted: Qt.callLater(summon)
  }
  Connections {
    target: backend
    function onHourReached(hours) {
      console.log("Tracked-hour milestone: " + hours)
      if (Quickshell.env("OMATRACKER_PREVIEW_SMOKE") === "1") {
        if (hours !== 1 || backend.feedbackError !== "" || backend.audioError !== "") { console.error("Hourly audio dispatch failed: " + (backend.feedbackError || backend.audioError)); Qt.quit(); return }
        window.milestoneObserved = true
      }
    }
    function onAudioStatusChanged() { if (backend.audioStatus) console.log(backend.audioStatus) }
    function onAudioErrorChanged() { if (backend.audioError) console.error(backend.audioError) }
    function onAudioFinished() {
      if (window.milestoneObserved && Quickshell.env("OMATRACKER_PREVIEW_SMOKE") === "1") {
        console.log("Real backend hourly milestone and completed audio playback passed (volume zero)")
        passed.running = true
      }
    }
  }
  Process {
    id: passed
    command: ["touch", Quickshell.env("OMATRACKER_UI_RESULT")]
    onExited: Qt.quit()
  }
}
