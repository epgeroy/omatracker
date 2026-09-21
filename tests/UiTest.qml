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
      { id: "one", title: "Production ready", status: "tracking", displaySeconds: 608 },
      { id: "two", title: "Documentation", status: "stopped", displaySeconds: 120 }]
    property var cockpitTasks: [
      { id: "one", title: "Production ready", status: "tracking", reason: "tracking", displaySeconds: 608 },
      { id: "two", title: "Documentation", status: "stopped", reason: "recentlyTracked", displaySeconds: 120 },
      { id: "three", title: "Unstarted task", status: "stopped", reason: "new", displaySeconds: 0 }]
    property var completedTasks: []
    property var activeProject: ({ id: "p", name: "Test project", clientName: "Test client", companyName: "", exportWeekly: true, exportMonthly: false, templateId: "detailed", accentColor: "#476a89", paper: "a4", logoPath: "" })
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
    property var activeProjectEstimate: null
    property string activeProjectAmountText: ""
    property var templates: [{ id: "detailed", name: "Detailed" }, { id: "user:custom", name: "Custom" }]
    property string templateError: ""
    property string templateStatus: ""
    property string reportStatus: "No reports queued"
    property string invoiceStatus: "No invoices"
    property var invoiceSettings: ({cadence: "monthly", templateId: "invoice"})
    property var invoices: []
    property var timeEntries: []
    property var nextEntryOffset: null
    property string setupStatus: "Ready"
    property bool backgroundChecksEnabled: false
    property string lastAction: ""
    property var lastChanges: ({})
    signal actionFinished(string action, bool success)
    signal hourReached(int hours)
    function displayTaskSeconds(task) { return task.displaySeconds }
    function stopTimer(id) { lastAction = "stop:" + id }
    function startTimer(id) { lastAction = "start:" + id }
    function completeTask(id) { lastAction = "complete:" + id }
    function reopenTask(id) { lastAction = "reopen:" + id }
    function addTask(title) { lastAction = "add:" + title }
    function renameAndAddManualTime(id, title, time, rates) { lastAction = "edit:" + id + ":" + title + ":" + time; lastChanges = rates || {} }
    function removeTask(id) { lastAction = "remove:" + id }
    function resetTimer(id) { lastAction = "reset:" + id }
    function resetActiveProject() { lastAction = "reset-project" }
    function selectProject(id) { lastAction = "project:" + id }
    function requestSync() { lastAction = "sync" }
    function requestExport(period) { lastAction = "export:" + period }
    function retryReports() { lastAction = "retry" }
    function refreshInvoices() { lastAction = "invoices" }
    function configureBilling(id, cadence) { lastAction = "billing:" + id; lastChanges = {cadence: cadence} }
    function invoiceAction(action, input) { lastAction = "invoice:" + action; lastChanges = input }
    function refreshEntries(offset) {}
    function correctEntry(id, revision, delta, reason) { lastAction = "correct:" + id }
    function setBackgroundChecks(enabled) { lastAction = "background:" + enabled }
    function updateProject(id, changes) { lastAction = "project-update:" + id; lastChanges = changes }
    function updateDrive(remote, folder, startup) { lastAction = "drive-update" }
    function updatePreferences(click, volume, motion) { lastAction = "preferences" }
    function previewClick(volume) { lastAction = "preview" }
    function createProject(name) { lastAction = "create:" + name }
    function refreshTemplates() {}
    function previewTemplate(id, project) { lastAction = "preview-template" }
    function editTemplate(id) { lastAction = "edit-template" }
    function createTemplate(name, from, project) { lastAction = "create-template" }
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
    function test_cockpit_labels_and_toggle_use_curated_tasks() {
      compare(view.tasks.length, 3)
      compare(view.tasks[1].reason, "recentlyTracked")
      var list = findChild(view, "taskList")
      verify(list !== null)
      compare(list.count, 3)
      var label = findChild(view, "cockpitReason-two")
      verify(label !== null)
      compare(label.text, "Recently tracked")
      label = findChild(view, "cockpitReason-three")
      verify(label !== null)
      compare(label.text, "New")
      keyClick(Qt.Key_J); keyClick(Qt.Key_J); keyClick(Qt.Key_Space)
      compare(fake.lastAction, "start:two")
    }
    function test_mouse_then_keyboard_changes_focus_target() {
      mouseClick(findChild(view, "primaryAction"))
      compare(fake.lastAction, "stop:one")
      keyClick(Qt.Key_J); keyClick(Qt.Key_J); keyClick(Qt.Key_Space)
      compare(fake.lastAction, "start:two")
    }
    function test_report_save_preserves_template() {
      view.navigate("reports"); wait(20)
      keyClick(Qt.Key_Tab); keyClick(Qt.Key_Tab); keyClick(Qt.Key_Return)
      compare(fake.lastAction, "billing:p")
      verify(fake.lastChanges.templateId === undefined)
      compare(fake.lastChanges.cadence, "monthly")
      fake.actionFinished("project-update", true)
    }
    function test_invoice_rows_use_explicit_id_and_revision() {
      fake.invoices = [{id:"inv-test",revision:3,state:"draft",number:"",totalText:"USD 80.00",from:"2025-08-01",to:"2025-09-01",renderStatus:"pending",uploadStatus:"pending"}]
      view.navigate("reports"); wait(30)
      var issue = findChild(view, "issueInvoice-inv-test")
      verify(issue !== null)
      issue.clicked()
      compare(fake.lastAction, "invoice:issue")
      compare(fake.lastChanges.id, "inv-test")
      compare(fake.lastChanges.revision, 3)
      fake.invoices = []
    }
    function test_entry_rows_load_billing_and_correction_history() {
      fake.timeEntries = [{entry:{id:"entry-test",taskTitle:"Design",seconds:3600,startedAt:1754820000000},billing:{resolved:true,rate:{currency:"USD"},revision:0},corrections:[]}]
      view.navigate("entries"); wait(30)
      compare(view.page, "entries")
      fake.timeEntries = []
    }
    function test_rate_fields_keep_draft_and_save() {
      view.navigate("project"); wait(20)
      var rate = findChild(view, "hourlyRate")
      rate.input.forceActiveFocus(); keyClicks("85.25")
      keyClick(Qt.Key_Tab); keyClicks("usd")
      compare(rate.text, "85.25")
      keyClick(Qt.Key_Tab); keyClick(Qt.Key_Return)
      compare(fake.lastChanges.hourlyRate, "85.25")
      compare(fake.lastChanges.currency, "USD")
      fake.actionFinished("project-update", true)
    }
    function test_task_rate_editor_passes_explicit_backfill_and_entity_token() {
      view.editTask({id:"one",title:"Task with no rate",rateSource:"project",rate:null,hourlyRate:"",entityRevision:"task-token"})
      wait(30)
      findChild(view, "taskInheritRate").checked = false
      findChild(view, "taskRate").text = "50.00"
      findChild(view, "taskCurrency").text = "eur"
      findChild(view, "taskApplyExisting").checked = true
      findChild(view, "saveTask").clicked()
      compare(fake.lastChanges.rate, "50.00")
      compare(fake.lastChanges.currency, "EUR")
      compare(fake.lastChanges.applyExisting, true)
      compare(fake.lastChanges.entityRevision, "task-token")
      fake.actionFinished("task-edit", true)
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
        compare(view.page, "confirm"); compare(view.confirmAction, "delete"); compare(fake.lastAction, "")
        keyClick(Qt.Key_Return); compare(view.page, "home"); compare(fake.lastAction, "")
    }
    function test_complete_requires_confirmation_and_done_tasks_can_reopen() {
      keyClick(Qt.Key_J); keyClick(Qt.Key_J); keyClick(Qt.Key_L); keyClick(Qt.Key_L); keyClick(Qt.Key_L)
      compare(view.actionIndex, 2)
      keyClick(Qt.Key_Return); compare(view.page, "confirm"); compare(fake.lastAction, "")
      keyClick(Qt.Key_Tab); keyClick(Qt.Key_Return); compare(fake.lastAction, "complete:two")
      fake.actionFinished("task-complete", true); compare(view.page, "home")

      fake.completedTasks = [{ id: "done", title: "Completed task", status: "done", displaySeconds: 300 }]
      verify(!view.completedExpanded)
      view.completedExpanded = true; wait(20)
      var reopen = findChild(view, "reopenTask-done")
      verify(reopen !== null)
      reopen.clicked(); compare(fake.lastAction, "reopen:done")
      fake.completedTasks = []
    }
    function test_all_pages_load() {
      var pages = ["projects", "project", "reports", "entries", "templates", "settings", "running", "commands", "new-project", "help"]
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
      var cockpit = fake.cockpitTasks
      fake.activeTasks = []
      fake.cockpitTasks = []
      keyClick(Qt.Key_Space); compare(view.page, "edit")
      fake.activeTasks = tasks
      fake.cockpitTasks = cockpit
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
