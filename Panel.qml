import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "omatracker"
  ipcTarget: "omatracker"
  manageIpc: false
  readonly property var service: tracker
  readonly property string configuredPath: String(setting("dataPath", "~/.config/omarchy/omatracker.json"))
  readonly property string dataFilePath: configuredPath.indexOf("~/") === 0
    ? (Quickshell.env("HOME") || "") + configuredPath.slice(1) : configuredPath
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  function open() { controller.show(); tracker.refresh(); Qt.callLater(view.summon) }
  function close() { controller.hide(); view.dismiss() }
  function toggle() { opened ? close() : open() }

  Service { id: tracker; dataPath: root.dataFilePath }
  IpcHandler {
    target: "omatracker"
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function add(): void { root.open(); view.newTask() }
    function resetAll(): void { root.open(); view.confirm("reset-project", "", "Reset project counters?") }
    function total(): string { return tracker.totalText }
    function sync(): void { tracker.requestSync() }
    function exportWeekly(): void { tracker.requestExport("weekly") }
    function exportMonthly(): void { tracker.requestExport("monthly") }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: tracker.totalText
    labelVisible: true
    hasVisualContent: true
    active: tracker.anyRunning
    tooltipText: "OmaTracker · " + tracker.runningTimers + " running · " + tracker.totalText
    onPressed: function(b) { if (b === Qt.LeftButton) root.toggle() }
  }

  KeyboardPanel {
    id: popup
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: view
    contentWidth: popup.fittedContentWidth(Style.space(420))
    contentHeight: popup.fittedContentHeight(view.implicitHeight, Style.space(650))
    TrackerView {
      id: view
      anchors.fill: parent
      tracker: root.service
      panelOpen: root.opened
      onCloseRequested: root.close()
      onSwitchPanelRequested: function(direction) { root.switchPanel(direction) }
    }
  }
}
