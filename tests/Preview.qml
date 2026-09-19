import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as App

FloatingWindow {
  id: window
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
      if (Quickshell.env("OMATRACKER_PREVIEW_SMOKE") === "1") {
        if (hours !== 1 || backend.feedbackError !== "") { console.error("Hourly audio dispatch failed: " + backend.feedbackError); Qt.quit(); return }
        console.log("Real backend hourly milestone and audio dispatch passed")
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
