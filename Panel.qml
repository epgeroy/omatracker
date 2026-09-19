import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
// qs.Ui comes last so its TextField shadows the Qt Quick Controls one.
import qs.Ui
import "TaskModel.js" as TaskModel

Panel {
  id: root
  moduleName: "omatracker"
  ipcTarget: "omatracker"
  // We own the IPC handler so `open`/`close` route through the overrides
  // below, which also tear down an in-progress inline edit.
  manageIpc: false

  // Replacement bars deliberately expose no service facade to third-party
  // widgets. This client contains no durable business logic; it invokes the
  // Rust backend, which serializes state mutations across panel instances.
  readonly property var tracker: trackerClient
  readonly property var trackerState: tracker ? tracker.state : ({ projects: [], drive: ({}) })
  readonly property var tasks: tracker ? tracker.activeTasks : []
  readonly property bool loaded: tracker ? tracker.loaded : false
  readonly property real nowMs: tracker ? tracker.nowMs : Date.now()
  readonly property var activeProject: tracker ? tracker.activeProject : null
  readonly property var projects: trackerState.projects || []
  property bool projectsVisible: false
  property bool settingsVisible: false
  // Status responses deserialize a new project object each time. Track its id
  // so a refresh of the current project cannot erase an in-progress edit.
  property string settingsProjectId: ""

  // Row the keyboard cursor is on. `cursorActive` stays false until the user
  // actually presses j/k so a freshly opened panel isn't pre-highlighted.
  property int cursorIndex: 0
  property bool cursorActive: false

  // At most one row is expanded (action buttons revealed) or in edit mode at
  // a time; both are tracked by task id so list reordering can't strand them.
  property string expandedId: ""
  property string editingId: ""

  // Where the cursor sits inside the expanded row's action strip: -1 means
  // it is still on the row itself, 0..actionCount-1 means one of the buttons
  // has focus. Order matches the strip: start/pause, reset, edit, delete.
  property int actionIndex: -1
  readonly property int actionCount: 4

  // The `?` cheat sheet, drawn under the footer while open.
  property bool helpVisible: false
  readonly property var keyHelp: [
    { keys: "j / k", label: "Move between tasks" },
    { keys: "l", label: "Show the row's actions" },
    { keys: "j / k", label: "Step into the actions, and back out" },
    { keys: "h / l", label: "Move across them (h closes from the first)" },
    { keys: "Enter", label: "Run the focused action, else start/stop" },
    { keys: "e / r", label: "Edit / reset the task" },
    { keys: "d", label: "Delete the task" },
    { keys: "a", label: "Add a task" },
    { keys: "Tab", label: "Next bar panel" },
    { keys: "? / Esc", label: "Toggle this list / close" }
  ]

  // Leaving the row, collapsing it, or opening the editor all take the
  // action strip off screen, so the cursor has to come back to the row.
  onExpandedIdChanged: root.actionIndex = -1
  onCursorIndexChanged: root.actionIndex = -1

  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
  readonly property color mutedForeground: Qt.darker(contentForeground, 1.75)
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property real panelWidth: Style.space(420)

  readonly property int totalSeconds: tracker ? tracker.displayTotalSeconds : 0
  readonly property int runningCount: tracker ? tracker.runningTimers : 0
  readonly property bool anyRunning: runningCount > 0
  readonly property string totalText: TaskModel.formatDuration(totalSeconds)
  readonly property string activeProjectText: tracker ? tracker.activeProjectText : "00:00:00"

  readonly property string home: Quickshell.env("HOME") || ""
  readonly property string configuredPath: String(setting("dataPath", "~/.config/omarchy/omatracker.json"))
  readonly property string dataFilePath: configuredPath.indexOf("~/") === 0
    ? home + configuredPath.slice(1)
    : configuredPath

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  // ------------------------------------------------------------- lifecycle

  function open() {
    root.controller.show()
  }

  function close() {
    root.cancelEdit()
    root.actionIndex = -1
    root.helpVisible = false
    root.controller.hide()
  }

  // Escape peels off one layer at a time: the cheat sheet first, the panel
  // only once it is out of the way.
  function handleClose() {
    if (root.helpVisible) root.helpVisible = false
    else root.close()
  }

  function toggle() {
    root.opened ? root.close() : root.open()
  }

  function seedSettingsFields() {
    if (!root.activeProject) return
    root.settingsProjectId = root.activeProject.id
    projectNameField.text = root.activeProject.name
    clientNameField.text = root.activeProject.clientName
    companyNameField.text = root.activeProject.companyName
    weeklyReportBox.checked = root.activeProject.exportWeekly
    monthlyReportBox.checked = root.activeProject.exportMonthly
    driveRemoteField.text = root.trackerState.drive.remote
    driveFolderField.text = root.trackerState.drive.folder
  }

  function toggleProjects() {
    root.projectsVisible = !root.projectsVisible
    if (root.projectsVisible) Qt.callLater(root.seedSettingsFields)
  }

  function saveProjectSettings() {
    if (!root.tracker || !root.activeProject) return
    root.tracker.updateProject(root.activeProject.id, {
      name: projectNameField.text,
      clientName: clientNameField.text,
      companyName: companyNameField.text
    })
    root.tracker.updateDrive(driveRemoteField.text, driveFolderField.text, startupSyncBox.checked)
  }

  onActiveProjectChanged: {
    if (root.projectsVisible && root.activeProject
      && root.activeProject.id !== root.settingsProjectId)
      Qt.callLater(root.seedSettingsFields)
  }
  onTasksChanged: {
    root.clampCursor()
    if (root.editingId !== "" && TaskModel.indexOfId(root.tasks, root.editingId) < 0) root.cancelEdit()
  }

  // ----------------------------------------------------------- task actions

  function addTask() {
    if (!root.tracker) return
    root.tracker.addTask()
    root.cursorIndex = Math.max(0, root.tasks.length)
    root.cursorActive = true
    root.expandedId = ""
  }

  function removeTask(id) {
    if (!root.tracker) return
    if (root.expandedId === id) root.expandedId = ""
    if (root.editingId === id) root.cancelEdit()
    root.tracker.removeTask(id)
    root.clampCursor()
  }

  function startTimer(id) {
    if (root.tracker) root.tracker.startTimer(id)
  }

  function stopTimer(id) {
    if (root.tracker) root.tracker.stopTimer(id)
  }

  function toggleTimer(id) {
    var index = TaskModel.indexOfId(root.tasks, id)
    if (index < 0) return
    root.tasks[index].running ? root.stopTimer(id) : root.startTimer(id)
  }

  // Reset starts a new visible counter while retaining the closed session for
  // reports. The service records a running segment before rebasing it.
  function resetTimer(id) {
    if (root.tracker) root.tracker.resetTimer(id)
  }

  function resetAllTimers() {
    if (root.tracker) root.tracker.resetActiveProject()
  }

  function toggleTimerAt(index) {
    if (index < 0 || index >= root.tasks.length) return
    root.toggleTimer(root.tasks[index].id)
  }

  function toggleExpanded(id) {
    root.expandedId = root.expandedId === id ? "" : id
  }

  // ------------------------------------------------------------- edit mode

  function startEdit(id) {
    root.expandedId = ""
    root.editingId = id
    var index = TaskModel.indexOfId(root.tasks, id)
    if (index >= 0) {
      root.cursorIndex = index
      root.cursorActive = true
    }
  }

  function cancelEdit() {
    if (root.editingId === "") return
    root.editingId = ""
    root.refocusKeys()
  }

  // The duration field adds a dated manual ledger entry. It no longer rewrites
  // a lifetime counter, which would make a past period report unknowable.
  function commitEdit(id, titleText, timeText) {
    if (!root.tracker || TaskModel.indexOfId(root.tasks, id) < 0) return root.cancelEdit()
    root.tracker.renameAndAddManualTime(id, titleText, timeText)
    root.editingId = ""
    root.refocusKeys()
  }

  function refocusKeys() {
    Qt.callLater(function() {
      if (root.opened) keyCatcher.forceActiveFocus()
    })
  }

  // -------------------------------------------------------------- keyboard

  function clampCursor() {
    if (root.tasks.length === 0) {
      root.cursorIndex = 0
      return
    }
    root.cursorIndex = Math.max(0, Math.min(root.cursorIndex, root.tasks.length - 1))
  }

  readonly property bool onAction: root.actionIndex >= 0

  function moveCursor(dx, dy) {
    if (root.tasks.length === 0) return

    if (dy !== 0) {
      // The action strip is drawn under its row, so vertical movement walks
      // into it and back out again rather than skipping to the next task.
      if (root.onAction) {
        if (dy < 0) root.actionIndex = -1
        return
      }
      if (!root.cursorActive) {
        root.cursorActive = true
        if (dy > 0) return
      }
      if (dy > 0 && root.expandedId === root.tasks[root.cursorIndex].id) {
        root.actionIndex = 0
        return
      }
      root.cursorIndex = Math.max(0, Math.min(root.cursorIndex + dy, root.tasks.length - 1))
    }

    if (dx !== 0) {
      root.cursorActive = true
      if (root.onAction) {
        // h/l walk the strip; h off its first button is the only way left,
        // so that keeps its old meaning of collapsing the row.
        if (dx < 0 && root.actionIndex === 0) root.expandedId = ""
        else root.actionIndex = Math.max(0, Math.min(root.actionIndex + dx, root.actionCount - 1))
        return
      }
      var id = root.tasks[root.cursorIndex].id
      root.expandedId = dx > 0 ? id : (root.expandedId === id ? "" : root.expandedId)
    }
  }

  // Enter/Space: the focused action button when the cursor is in the strip,
  // otherwise the row's start/stop shortcut.
  function activateCursor() {
    root.cursorActive = true
    if (root.tasks.length === 0) return
    if (!root.onAction) return root.toggleTimerAt(root.cursorIndex)

    var id = root.tasks[root.cursorIndex].id
    if (root.actionIndex === 0) root.toggleTimer(id)
    else if (root.actionIndex === 1) root.resetTimer(id)
    else if (root.actionIndex === 2) root.startEdit(id)
    else if (root.actionIndex === 3) root.removeTask(id)
  }

  // The task the row shortcuts act on, or "" when nothing is highlighted.
  // Requiring `cursorActive` is what keeps a freshly opened panel — which
  // shows no highlight yet — from letting `d` delete the top task blind.
  function cursorTaskId() {
    if (!root.cursorActive || root.tasks.length === 0) return ""
    return root.tasks[root.cursorIndex].id
  }

  // Row shortcuts, mirroring the action strip's buttons one key each.
  function handleTextKey(text) {
    var key = String(text || "").toLowerCase()
    if (key === "?") return root.helpVisible = !root.helpVisible
    if (key === "a" || key === "n") return root.addTask()

    var id = root.cursorTaskId()
    if (id === "") return
    if (key === "e") root.startEdit(id)
    else if (key === "r") root.resetTimer(id)
    else if (key === "d") root.removeTask(id)
  }

  function deleteCursorTask() {
    var id = root.cursorTaskId()
    if (id !== "") root.removeTask(id)
  }

  Service {
    id: trackerClient
    dataPath: root.dataFilePath
  }

  IpcHandler {
    target: "omatracker"
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function add(): void { root.addTask() }
    function resetAll(): void { root.resetAllTimers() }
    function total(): string { return root.totalText }
    function sync(): void { if (root.tracker) root.tracker.requestSync() }
    function exportWeekly(): void { if (root.tracker) root.tracker.requestExport("weekly") }
    function exportMonthly(): void { if (root.tracker) root.tracker.requestExport("monthly") }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.totalText
    labelVisible: true
    hasVisualContent: true
    active: root.anyRunning
    tooltipText: root.anyRunning
      ? "Total " + root.totalText + " · " + root.runningCount + " running"
      : "Total " + root.totalText
    onPressed: function(b) {
      if (b === Qt.LeftButton) root.toggle()
    }
  }

  KeyboardPanel {
    id: popup
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: popup.fittedContentWidth(root.panelWidth)
    contentHeight: popup.fittedContentHeight(contentColumn.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      // The inline editor's text fields own every key while it is open.
      blocked: root.editingId !== ""

      onMoveRequested: function(dx, dy) { root.moveCursor(dx, dy) }
      onActivateRequested: root.activateCursor()
      onDeleteRequested: root.deleteCursorTask()
      onCloseRequested: root.handleClose()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(t) { root.handleTextKey(t) }

      Column {
        id: contentColumn
        width: Math.max(1, keyCatcher.width)
        spacing: Style.space(8)

        Item {
          width: parent.width
          height: Style.space(30)

          Text {
            anchors.left: parent.left
            anchors.leftMargin: Style.space(4)
            anchors.right: projectButton.left
            anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            text: root.activeProject ? root.activeProject.name + "  " + root.activeProjectText : "Loading projects"
            color: root.contentForeground
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.body
            elide: Text.ElideRight
          }

          PanelActionButton {
            id: projectButton
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            iconText: "󰆍"
            tooltipText: root.projectsVisible ? "Hide project settings" : "Manage projects, reports, and Drive"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            bordered: true
            onClicked: root.toggleProjects()
          }
        }

        Column {
          id: projectSettings
          visible: root.projectsVisible && root.activeProject
          width: parent.width
          spacing: Style.space(6)

          PanelSeparator { foreground: root.contentForeground }

          PanelSectionHeader {
            text: "PROJECT"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
          }

          TextField {
            id: projectNameField
            width: parent.width
            placeholderText: "Project name"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
          }

          TextField {
            id: clientNameField
            width: parent.width
            placeholderText: "Client name (optional)"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
          }

          TextField {
            id: companyNameField
            width: parent.width
            placeholderText: "Prepared by (optional)"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
          }

          Text {
            width: parent.width
            text: "PDF template: " + (root.activeProject && root.activeProject.templateId === "summary" ? "Summary" : "Detailed") + " (click to switch)"
            color: root.mutedForeground
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.bodySmall
            MouseArea {
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: root.tracker.updateProject(root.activeProject.id, {
                templateId: root.activeProject && root.activeProject.templateId === "summary" ? "detailed" : "summary"
              })
            }
          }

          Row {
            spacing: Style.space(12)

            CheckBox {
              id: weeklyReportBox
              text: "Weekly reports"
              checked: root.activeProject ? root.activeProject.exportWeekly : true
              onToggled: if (root.tracker && root.activeProject)
                root.tracker.updateProject(root.activeProject.id, { exportWeekly: checked })
            }

            CheckBox {
              id: monthlyReportBox
              text: "Monthly reports"
              checked: root.activeProject ? root.activeProject.exportMonthly : true
              onToggled: if (root.tracker && root.activeProject)
                root.tracker.updateProject(root.activeProject.id, { exportMonthly: checked })
            }
          }

          Text {
            width: parent.width
            text: root.tracker && root.tracker.backgroundChecksEnabled
              ? "Background report checks: on (click to disable)"
              : "Background report checks: off (click to enable)"
            color: root.mutedForeground
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.bodySmall
            MouseArea {
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: if (root.tracker)
                root.tracker.setBackgroundChecks(!root.tracker.backgroundChecksEnabled)
            }
          }

          Row {
            spacing: Style.space(8)

            PanelActionButton {
              iconText: "󰄬"
              tooltipText: "Save project settings"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.saveProjectSettings()
            }

            PanelActionButton {
              iconText: "󰐕"
              tooltipText: "Create project"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: {
                root.tracker.createProject("New project")
                Qt.callLater(root.seedSettingsFields)
              }
            }

            PanelActionButton {
              iconText: "󰉋"
              tooltipText: "Export previous weekly PDF"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.tracker.requestExport("weekly")
            }

            PanelActionButton {
              iconText: "󰉌"
              tooltipText: "Export previous monthly PDF"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.tracker.requestExport("monthly")
            }
          }

          Repeater {
            model: root.projects

            delegate: Text {
              id: projectOption
              required property var modelData
              width: projectSettings.width
              text: (modelData.id === root.activeProject.id ? "* " : "  ") + modelData.name
              color: modelData.id === root.activeProject.id ? root.contentForeground : root.mutedForeground
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  root.tracker.selectProject(projectOption.modelData.id)
                }
              }
            }
          }

          PanelSeparator { foreground: root.contentForeground }

          PanelSectionHeader {
            text: "GOOGLE DRIVE"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
          }

          TextField {
            id: driveRemoteField
            width: parent.width
            placeholderText: "rclone remote name, e.g. omatracker"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
          }

          TextField {
            id: driveFolderField
            width: parent.width
            placeholderText: "Drive folder"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
          }

          CheckBox {
            id: startupSyncBox
            text: "Sync local state on startup"
            checked: root.trackerState.drive.syncOnStartup === true
          }

          Row {
            spacing: Style.space(8)
            PanelActionButton {
              iconText: "󰄬"
              tooltipText: "Save Drive settings"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.saveProjectSettings()
            }
            PanelActionButton {
              iconText: "󰑓"
              tooltipText: "Sync local state to Google Drive now"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.tracker.requestSync()
            }
            PanelActionButton {
              iconText: "󰑐"
              tooltipText: "Retry pending PDF exports"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.tracker.retryReports()
            }
          }

          Text {
            width: parent.width
            text: root.tracker ? root.tracker.setupStatus + "\n" + root.tracker.syncStatus + "\n" + root.tracker.reportStatus : "Service unavailable"
            color: root.tracker && (root.tracker.syncError !== "" || root.tracker.backendError !== "" || root.tracker.reportStatus.indexOf("PDF pending:") === 0)
              ? (root.bar ? root.bar.urgent : Color.urgent) : root.mutedForeground
            wrapMode: Text.Wrap
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.caption
          }

          Text {
            visible: root.tracker && (root.tracker.syncError !== "" || root.tracker.backendError !== "")
            width: parent.width
            text: root.tracker ? (root.tracker.backendError || root.tracker.syncError) : ""
            color: root.bar ? root.bar.urgent : Color.urgent
            wrapMode: Text.Wrap
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.caption
          }
        }

        ListView {
          id: taskList
          width: parent.width
          height: Math.min(contentHeight, Style.space(340))
          spacing: Style.space(2)
          clip: true
          boundsBehavior: Flickable.StopAtBounds
          interactive: contentHeight > height
          visible: root.tasks.length > 0

          ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

          model: root.tasks
          currentIndex: root.cursorIndex
          onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)

          // ListView's delegate context doesn't reach into a nested
          // `component` declaration, so the wrapper forwards it explicitly.
          delegate: Item {
            required property var modelData
            required property int index
            width: ListView.view.width
            height: taskRow.implicitHeight

            TaskRow {
              id: taskRow
              width: parent.width
              task: parent.modelData
              index: parent.index
            }
          }
        }

        Text {
          visible: root.tasks.length === 0
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          topPadding: Style.space(14)
          bottomPadding: Style.space(6)
          text: "No tasks yet — press + to add one"
          color: root.mutedForeground
          font.family: root.contentFontFamily
          font.pixelSize: Style.font.bodySmall
          font.italic: true
        }

        PanelSeparator {
          foreground: root.contentForeground
        }

        Item {
          width: parent.width
          height: Style.space(30)

          Text {
            id: totalLabel
            anchors.left: parent.left
            anchors.leftMargin: Style.space(4)
            anchors.verticalCenter: parent.verticalCenter
            text: "󰄬  Project: " + root.activeProjectText
            color: root.anyRunning ? root.contentForeground : root.mutedForeground
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.body
          }

          Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(10)

            PanelActionButton {
              iconText: "󰜉"
              tooltipText: "Reset visible counters for this project"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              hoverColor: root.bar ? root.bar.urgent : Color.urgent
              bordered: true
              enabled: root.tasks.length > 0
              onClicked: root.resetAllTimers()
            }

            PanelActionButton {
              iconText: "󰋗"
              tooltipText: root.helpVisible ? "Hide keybinds" : "Show keybinds (?)"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              hasCursor: root.helpVisible
              onClicked: root.helpVisible = !root.helpVisible
            }

            PanelActionButton {
              iconText: "󰐕"
              tooltipText: "Add task"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
              bordered: true
              onClicked: root.addTask()
            }
          }
        }

        // ------------------------------------------------------ cheat sheet
        // Sits at the bottom of the same card, so the panel simply grows
        // downward to reveal it. `visible: false` keeps it out of the Column's
        // implicitHeight, which is what the popup sizes itself from.
        Column {
          id: helpCard
          visible: root.helpVisible
          width: parent.width
          spacing: Style.space(4)

          PanelSeparator {
            foreground: root.contentForeground
          }

          PanelSectionHeader {
            text: "KEYBINDS"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
          }

          Repeater {
            model: root.keyHelp

            delegate: Item {
              required property var modelData
              width: helpCard.width
              height: Style.space(19)

              Text {
                id: helpKeys
                anchors.left: parent.left
                anchors.leftMargin: Style.space(4)
                anchors.verticalCenter: parent.verticalCenter
                width: Style.space(64)
                text: parent.modelData.keys
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
              }

              Text {
                anchors.left: helpKeys.right
                anchors.leftMargin: Style.space(8)
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: parent.modelData.label
                color: root.mutedForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                elide: Text.ElideRight
              }
            }
          }

          Item {
            width: parent.width
            height: Style.space(2)
          }
        }
      }
    }
  }

  // A single task row: status dot, title, elapsed time, and a chevron that
  // reveals the per-task actions. Swaps to an inline title/time editor while
  // this task is the one being edited.
  component TaskRow: CursorSurface {
    id: row
    required property var task
    required property int index

    readonly property bool isEditing: root.editingId === task.id
    readonly property bool isExpanded: root.expandedId === task.id && !isEditing
    readonly property bool isRunning: task.running === true
    readonly property int seconds: root.tracker ? root.tracker.displayTaskSeconds(task) : 0
    // Running tasks read at full strength; idle ones recede into the muted
    // tint so the active timers are obvious at a glance.
    readonly property color rowForeground: isRunning ? root.contentForeground : root.mutedForeground

    readonly property bool isCurrent: root.cursorActive && root.cursorIndex === index && !isEditing
    // Only one hover-cursor highlight is allowed on screen at a time, so
    // once the cursor steps into the action strip the row drops back to the
    // quieter "selected" paint and the focused button takes the cursor.
    readonly property bool actionsFocused: isCurrent && isExpanded && root.onAction

    hasCursor: isCurrent && !actionsFocused
    current: actionsFocused
    foreground: root.contentForeground
    implicitHeight: rowColumn.implicitHeight + Style.space(6)

    function commit() {
      root.commitEdit(row.task.id, titleField.text, timeField.text)
    }

    Column {
      id: rowColumn
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(6)

      // ------------------------------------------------------- display mode
      Item {
        id: bodyItem
        visible: !row.isEditing
        width: parent.width
        height: Style.space(26)

        MouseArea {
          anchors.fill: parent
          hoverEnabled: true
          acceptedButtons: Qt.NoButton
          onContainsMouseChanged: if (containsMouse) {
            root.cursorActive = true
            root.cursorIndex = row.index
          }
        }

        Text {
          id: statusDot
          anchors.left: parent.left
          anchors.leftMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          text: row.isRunning ? "◉" : "○"
          color: row.rowForeground
          font.family: root.contentFontFamily
          font.pixelSize: Style.font.subtitle
        }

        PanelActionButton {
          id: chevron
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          size: Style.space(22)
          iconText: row.isExpanded ? "󰅀" : "󰅂"
          tooltipText: row.isExpanded ? "Hide actions" : "Show actions"
          foreground: row.rowForeground
          fontFamily: root.contentFontFamily
          onClicked: root.toggleExpanded(row.task.id)
        }

        Text {
          id: timeLabel
          anchors.right: chevron.left
          anchors.rightMargin: Style.space(6)
          anchors.verticalCenter: parent.verticalCenter
          text: TaskModel.formatDuration(row.seconds)
          color: row.rowForeground
          font.family: root.contentFontFamily
          font.pixelSize: Style.font.body
        }

        Text {
          id: titleLabel
          anchors.left: statusDot.right
          anchors.leftMargin: Style.space(10)
          anchors.right: timeLabel.left
          anchors.rightMargin: Style.space(10)
          anchors.verticalCenter: parent.verticalCenter
          text: row.task.title
          color: row.rowForeground
          font.family: root.contentFontFamily
          font.pixelSize: Style.font.body
          elide: Text.ElideRight
        }

        // Clicking the task name is the fast path for start/stop. Declared
        // after the row-wide hover area so it wins the press.
        MouseArea {
          anchors.top: titleLabel.top
          anchors.bottom: titleLabel.bottom
          anchors.left: statusDot.left
          anchors.right: titleLabel.right
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onContainsMouseChanged: if (containsMouse) {
            root.cursorActive = true
            root.cursorIndex = row.index
          }
          onClicked: root.toggleTimer(row.task.id)
        }
      }

      // ---------------------------------------------------------- edit mode
      Column {
        id: editItem
        visible: row.isEditing
        width: parent.width
        spacing: Style.space(6)

        // Seed the fields from the task each time the editor opens, then hand
        // it focus once the items have been laid out.
        onVisibleChanged: if (visible) {
          titleField.text = row.task.title
          timeField.text = ""
          Qt.callLater(function() {
            titleField.forceActiveFocus()
            titleField.selectAll()
          })
        }

        Item {
          width: parent.width
          height: Math.max(titleField.implicitHeight, timeField.implicitHeight)

          TextField {
            id: timeField
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Style.space(86)
            horizontalAlignment: Text.AlignHCenter
            placeholderText: "Add time"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
            Keys.onPressed: function(event) {
              if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                row.commit()
                event.accepted = true
              } else if (event.key === Qt.Key_Escape) {
                root.cancelEdit()
                event.accepted = true
              }
            }
          }

          TextField {
            id: titleField
            anchors.left: parent.left
            anchors.right: timeField.left
            anchors.rightMargin: Style.space(6)
            anchors.verticalCenter: parent.verticalCenter
            placeholderText: "Task name"
            foreground: root.contentForeground
            font.family: root.contentFontFamily
            Keys.onPressed: function(event) {
              if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                row.commit()
                event.accepted = true
              } else if (event.key === Qt.Key_Escape) {
                root.cancelEdit()
                event.accepted = true
              } else if (event.key === Qt.Key_Tab) {
                timeField.forceActiveFocus()
                timeField.selectAll()
                event.accepted = true
              }
            }
          }
        }

        Item {
          width: parent.width
          height: Style.space(24)

          PanelActionButton {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            iconText: "󰅖"
            tooltipText: "Cancel"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            bordered: true
            onClicked: root.cancelEdit()
          }
        }
      }

      // ------------------------------------------------------- row actions
      Item {
        visible: row.isExpanded
        width: parent.width
        height: Style.space(26)

        Row {
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(10)

          PanelActionButton {
            iconText: row.isRunning ? "󰏤" : "󰐊"
            tooltipText: row.isRunning ? "Pause timer" : "Start timer"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            bordered: true
            hasCursor: row.actionsFocused && root.actionIndex === 0
            onClicked: root.toggleTimer(row.task.id)
          }

          PanelActionButton {
            iconText: "󰜉"
            tooltipText: "Reset timer to 00:00:00"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            bordered: true
            hasCursor: row.actionsFocused && root.actionIndex === 1
            onClicked: root.resetTimer(row.task.id)
          }

          PanelActionButton {
            iconText: "󰏫"
            tooltipText: "Edit title and time"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            bordered: true
            hasCursor: row.actionsFocused && root.actionIndex === 2
            onClicked: root.startEdit(row.task.id)
          }

          PanelActionButton {
            iconText: "󰩹"
            tooltipText: "Delete task"
            foreground: root.contentForeground
            fontFamily: root.contentFontFamily
            hoverColor: root.bar ? root.bar.urgent : Color.urgent
            bordered: true
            hasCursor: row.actionsFocused && root.actionIndex === 3
            onClicked: root.removeTask(row.task.id)
          }
        }
      }
    }
  }
}
