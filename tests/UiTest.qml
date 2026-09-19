import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as App

FloatingWindow {
  id: window
  visible: true
  color: Color.popups.background
  implicitWidth: 440; implicitHeight: 650
  QtObject {
    id: fake
    property var activeTasks: [
      { id: "one", title: "Production ready", running: true, displaySeconds: 608 },
      { id: "two", title: "Documentation", running: false, displaySeconds: 120 }]
    property var activeProject: ({ id: "p", name: "Test project", clientName: "Test client", companyName: "", exportWeekly: true, exportMonthly: false, templateId: "detailed" })
    property var state: ({ projects: [activeProject], drive: { remote: "test", folder: "Tracker", syncOnStartup: false } })
    property var runningTasks: [activeTasks[0]]
    property var preferences: ({ hourlyClick: true, volume: 25, reducedMotion: true })
    property bool loaded: true
    property int runningTimers: 1
    property int activeProjectRunningTimers: 1
    property string activeProjectText: "00:12:08"
    property string syncStatus: "Synced"
    property string syncError: ""
    property string backendError: ""
    property string feedbackError: ""
    property string reportStatus: "No reports queued"
    property string setupStatus: "Ready"
    property bool backgroundChecksEnabled: false
    property string lastAction: ""
    signal actionFinished(string action, bool success)
    signal hourReached(int hours)
    function displayTaskSeconds(task) { return task.displaySeconds }
    function stopTimer(id) { lastAction = "stop:" + id }
    function startTimer(id) { lastAction = "start:" + id }
    function addTask(title) { lastAction = "add:" + title }
    function renameAndAddManualTime(id, title, time) { lastAction = "edit:" + id + ":" + title + ":" + time }
    function removeTask(id) { lastAction = "remove:" + id }
    function resetTimer(id) { lastAction = "reset:" + id }
    function resetActiveProject() { lastAction = "reset-project" }
    function selectProject(id) { lastAction = "project:" + id }
    function requestSync() { lastAction = "sync" }
    function requestExport(period) { lastAction = "export:" + period }
    function retryReports() { lastAction = "retry" }
    function setBackgroundChecks(enabled) { lastAction = "background:" + enabled }
    function updateProject(id, changes) { lastAction = "project-update" }
    function updateDrive(remote, folder, startup) { lastAction = "drive-update" }
    function updatePreferences(click, volume, motion) { lastAction = "preferences" }
    function previewClick(volume) { lastAction = "preview" }
    function createProject(name) { lastAction = "create:" + name }
  }
  App.TrackerView { id: view; width: 420; height: 600; tracker: fake }
  TestCase {
    id: suite
    name: "FocusCard"
    when: true
    function keyClicks(text) { for (var i = 0; i < text.length; i++) keyClick(text[i]) }
    function cleanup() { console.log(qtest_results.functionName + ": " + (qtest_results.skipped ? "skipped (Wayland-only)" : qtest_results.failed ? "FAILED" : "passed")) }
    function cleanupTestCase() {
      console.log("UI failures: " + qtest_results.failCount)
      if (qtest_results.failCount === 0) { passed.running = true; wait(200) }
    }
    function init() { view.dismiss(); fake.lastAction = ""; view.summon(); wait(20) }
    function test_primary_and_navigation() {
      keyClick(Qt.Key_Space); compare(fake.lastAction, "stop:one")
      keyClick(Qt.Key_J); compare(view.selectedId, "one")
      keyClick(Qt.Key_J); compare(view.selectedId, "two")
      compare(view.heroTask.id, "one")
      keyClick(Qt.Key_Space); compare(fake.lastAction, "start:two")
    }
    function test_editor_owns_shortcuts_and_keeps_failed_draft() {
      keyClick(Qt.Key_N); tryCompare(view, "page", "edit")
      wait(20)
      keyClicks("jkrd task")
      compare(fake.lastAction, "")
      keyClick(Qt.Key_Return); compare(fake.lastAction, "add:jkrd task")
      fake.actionFinished("task-add", false)
      compare(view.page, "edit")
      keyClick(Qt.Key_Return); compare(fake.lastAction, "add:jkrd task")
      fake.actionFinished("task-add", true)
      compare(view.page, "home")
    }
    function test_search_and_escape() {
      keyClick(Qt.Key_Slash); wait(20)
      keyClicks("doc"); compare(view.choices.length, 1)
      keyClick(Qt.Key_Return); compare(view.page, "home"); compare(view.selectedId, "two")
      keyClick(Qt.Key_Comma); wait(20); compare(view.page, "settings")
      keyClick(Qt.Key_Escape); compare(view.page, "home")
    }
    function test_delete_requires_explicit_confirmation() {
      keyClick(Qt.Key_D); wait(20)
      compare(view.page, "confirm"); compare(fake.lastAction, "")
      keyClick(Qt.Key_Return); compare(view.page, "home"); compare(fake.lastAction, "")
    }
    function test_all_pages_load() {
      var pages = ["projects", "project", "reports", "settings", "running", "commands", "new-project", "help"]
      for (var i = 0; i < pages.length; i++) {
        view.navigate(pages[i]); wait(30); compare(view.page, pages[i]); view.back(); wait(20)
      }
    }
    function test_actions_start_with_primary_and_keep_target() {
      keyClick(Qt.Key_J); keyClick(Qt.Key_J); keyClick(Qt.Key_L)
      compare(view.actionIndex, 0)
      keyClick(Qt.Key_Return); compare(fake.lastAction, "start:two")
      keyClick(Qt.Key_H); verify(!view.actionsVisible)
      keyClick(Qt.Key_K); keyClick(Qt.Key_K)
      verify(view.heroFocused)
      keyClick(Qt.Key_D); compare(view.confirmId, "one")
    }
    function test_settings_tab_and_text_are_local() {
      keyClick(Qt.Key_Comma); wait(20)
      keyClick(Qt.Key_Tab); keyClick(Qt.Key_A, Qt.ControlModifier); keyClicks("40")
      compare(fake.lastAction, "")
      keyClick(Qt.Key_Tab); keyClick(Qt.Key_Return)
      compare(fake.lastAction, "preview")
    }
    function test_empty_state_primary_creates_task() {
      var tasks = fake.activeTasks
      fake.activeTasks = []
      keyClick(Qt.Key_Space); compare(view.page, "edit")
      fake.activeTasks = tasks
    }
    function test_popup_component_loads() {
      if (Quickshell.env("QT_QPA_PLATFORM") !== "wayland") { skip("Layer-shell integration runs with --wayland"); return }
      var component = Qt.createComponent("plugin/Panel.qml")
      if (component.status !== Component.Ready) console.error("Panel load: " + component.errorString())
      compare(component.status, Component.Ready, component.errorString())
      var panel = component.createObject(window.contentItem, { settings: { dataPath: Quickshell.env("OMATRACKER_UI_LEDGER") } })
      verify(panel !== null, component.errorString())
      panel.service.feedbackEnabled = false
      verify(panel.service !== null)
      panel.open(); wait(180); verify(panel.opened)
      panel.close(); wait(180); verify(!panel.opened)
      panel.destroy()
    }
    function test_snapshot() {
      var path = Quickshell.env("OMATRACKER_UI_SNAPSHOT")
      if (path) { view.notice = ""; wait(100); grabImage(view).save(path) }
    }
  }
  Process {
    id: passed
    command: ["touch", Quickshell.env("OMATRACKER_UI_RESULT")]
  }
  Timer { interval: 15000; running: true; onTriggered: { console.error("UI test timeout"); Qt.quit() } }
}
