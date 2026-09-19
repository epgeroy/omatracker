import QtQuick
import QtQuick.Controls as Controls
import qs.Commons
import qs.Ui as Ui
import "TaskModel.js" as TaskModel

FocusScope {
  id: root
  required property var tracker
  property bool panelOpen: true
  signal closeRequested()
  signal switchPanelRequested(int direction)
  property string page: "home"
  property var history: []
  property string selectedId: ""
  property string heroId: ""
  property bool heroFocused: true
  property bool actionsVisible: false
  property int actionIndex: 0
  property int choiceIndex: 0
  property string query: ""
  property var draftTask: null
  property string confirmAction: ""
  property string confirmId: ""
  property string confirmTitle: ""
  property string pendingAction: ""
  property string notice: ""
  property real savedScroll: 0
  readonly property var tasks: tracker.activeTasks || []
  readonly property var project: tracker.activeProject
  readonly property var preferences: tracker.preferences
  readonly property bool reducedMotion: preferences.reducedMotion === true
  readonly property color foreground: Color.popups.text
  // Blend against the actual popup background instead of darkening text: this
  // remains legible on both light and dark themes, including custom bar colors.
  readonly property color secondary: Qt.tint(Color.popups.background, Qt.rgba(foreground.r, foreground.g, foreground.b, 0.78))
  readonly property var selectedTask: tasks.find(function(t) { return t.id === root.selectedId }) || null
  readonly property var heroTask: {
    var remembered = tasks.find(function(t) { return t.id === root.heroId })
    if (remembered && remembered.running) return remembered
    return tasks.find(function(t) { return t.running }) || remembered || tasks[0] || null
  }
  readonly property bool picker: ["projects", "search", "commands", "running"].indexOf(page) >= 0
  readonly property var commandItems: [
    { id: "new", title: "New task", hint: "n" },
    { id: "projects", title: "Switch project", hint: "p" },
    { id: "new-project", title: "Create project", hint: "" },
    { id: "project", title: "Project settings", hint: "" },
    { id: "reports", title: "Reports and exports", hint: "" },
    { id: "settings", title: "Preferences and Google Drive", hint: "," },
    { id: "running", title: "Running timers · all projects", hint: "" },
    { id: "sync", title: "Sync to Google Drive", hint: "" },
    { id: "reset-project", title: "Reset visible project counters…", hint: "" },
    { id: "help", title: "Keyboard shortcuts", hint: "?" }
  ]
  readonly property var choices: {
    var source = page === "projects" ? tracker.state.projects || []
      : page === "search" ? tasks : page === "running" ? tracker.runningTasks || [] : commandItems
    var needle = query.trim().toLowerCase()
    return source.filter(function(item) { return String(item.name || item.title).toLowerCase().indexOf(needle) >= 0 })
  }
  readonly property string pageTitle: ({ projects: "Projects", project: "Project settings",
    reports: "Reports", settings: "Preferences & Drive", search: "Find a task", commands: "Commands",
    running: "Running · all projects", edit: draftTask ? "Edit task" : "New task",
    "new-project": "New project", confirm: "Confirm action", help: "Keyboard shortcuts" })[page] || "OmaTracker"
  implicitHeight: Math.min(Style.space(570), content.implicitHeight)

  function summon() { if (page === "home") { heroFocused = true; forceActiveFocus() } }
  function dismiss() { page = "home"; history = []; query = ""; actionsVisible = false; pendingAction = "" }
  function navigate(next) {
    if (page === "home") savedScroll = scroll.contentY
    history = history.concat([page]); page = next; query = ""; choiceIndex = 0
    actionsVisible = false
    Qt.callLater(function() { scroll.contentY = 0; if (body.item && body.item.focusInitial) body.item.focusInitial(); else root.forceActiveFocus() })
    if (!reducedMotion) entrance.restart()
  }
  function back() {
    if (pendingAction !== "") return
    if (page === "home") { if (actionsVisible) { actionsVisible = false; forceActiveFocus() } else closeRequested(); return }
    page = history.length ? history[history.length - 1] : "home"
    history = history.slice(0, -1); query = ""; choiceIndex = 0
    Qt.callLater(function() {
      if (page === "home") { scroll.contentY = savedScroll; root.forceActiveFocus() }
      else if (body.item && body.item.focusInitial) body.item.focusInitial()
    })
  }
  function toast(text) { notice = text; noticeTimer.restart() }
  function newTask() { draftTask = null; navigate("edit") }
  function editTask(task) { if (!task) return; draftTask = task; navigate("edit") }
  function toggleTask(task) {
    if (!task || !tracker.loaded) return
    heroId = task.id
    task.running ? tracker.stopTimer(task.id) : tracker.startTimer(task.id)
  }
  function confirm(action, id, title) {
    confirmAction = action; confirmId = id; confirmTitle = title; navigate("confirm")
  }
  function runCommand(id) {
    if (id === "new") newTask()
    else if (id === "sync") { tracker.requestSync(); toast("Sync requested") }
    else if (id === "reset-project") confirm(id, "", "Reset project counters?")
    else navigate(id)
  }
  function choose(index) {
    var item = choices[index]; if (!item) return
    if (page === "projects") { tracker.selectProject(item.id); page = "home"; history = []; heroId = ""; selectedId = ""; heroFocused = true; forceActiveFocus() }
    else if (page === "search") { selectedId = item.id; heroFocused = false; page = "home"; history = []; forceActiveFocus(); Qt.callLater(revealSelection) }
    else if (page === "running") {
      tracker.selectProject(item.projectId); heroId = item.id; selectedId = item.id
      page = "home"; history = []; heroFocused = true; forceActiveFocus()
    } else runCommand(item.id)
  }
  function revealSelection() { if (body.item && body.item.revealSelection) body.item.revealSelection() }
  function move(dy) {
    if (picker) { choiceIndex = Math.max(0, Math.min(choices.length - 1, choiceIndex + dy)); if (body.item) body.item.revealChoice(); return }
    if (page !== "home" || tasks.length === 0) return
    if (actionsVisible) { actionsVisible = false; forceActiveFocus(); return }
    var index = heroFocused ? -1 : tasks.findIndex(function(t) { return t.id === selectedId })
    index = Math.max(-1, Math.min(tasks.length - 1, index + dy))
    heroFocused = index < 0; selectedId = index < 0 ? "" : tasks[index].id
    forceActiveFocus(); revealSelection()
  }
  function rowAction(index) {
    var task = heroFocused ? heroTask : selectedTask; if (!task) return
    if (index === 0) toggleTask(task)
    else if (index === 1) editTask(task)
    else if (index === 2) confirm("reset", task.id, "Reset “" + task.title + "”?")
    else confirm("delete", task.id, "Delete “" + task.title + "”?")
  }

  Keys.onPressed: function(event) {
    if (event.key === Qt.Key_Escape) { back(); event.accepted = true; return }
    // Editors own text, navigation and Tab. No letter shortcuts escape a form.
    if (page !== "home" && !picker) return
    if (event.key === Qt.Key_K && (event.modifiers & Qt.ControlModifier)) { navigate("commands"); event.accepted = true; return }
    if (event.key === Qt.Key_Down || (!picker && event.text === "j")) move(1)
    else if (event.key === Qt.Key_Up || (!picker && event.text === "k")) move(-1)
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || (!picker && event.key === Qt.Key_Space)) {
      if (picker) choose(choiceIndex)
      else if (actionsVisible) rowAction(actionIndex)
      else if (heroFocused && !heroTask) newTask()
      else toggleTask(heroFocused ? heroTask : selectedTask)
    } else if (!picker && (event.key === Qt.Key_Right || event.text === "l")) {
      actionIndex = actionsVisible ? Math.min(3, actionIndex + 1) : 0; actionsVisible = true
    }
    else if (!picker && (event.key === Qt.Key_Left || event.text === "h")) { if (actionIndex === 0) actionsVisible = false; else actionIndex-- }
    else if (!picker && (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab)) switchPanelRequested(event.modifiers & Qt.ShiftModifier ? -1 : 1)
    else if (!picker && (event.text === "n" || event.text === "a")) newTask()
    else if (!picker && event.text === "e") editTask(heroFocused ? heroTask : selectedTask)
    else if (!picker && event.text === "p") navigate("projects")
    else if (!picker && event.text === "/") navigate("search")
    else if (!picker && event.text === ",") navigate("settings")
    else if (!picker && event.text === "?") navigate("help")
    else if (!picker && (event.text === "d" || event.text === "x")) rowAction(3)
    else if (!picker && event.text === "r") rowAction(2)
    else return
    event.accepted = true
  }

  Connections {
    target: root.tracker
    function onActionFinished(action, success) {
      if (root.pendingAction === action) {
        root.pendingAction = ""
        if (success) { root.back(); root.toast("✓ Saved") }
      }
    }
    function onHourReached(hours) { if (root.panelOpen) root.toast(hours + (hours === 1 ? " hour tracked" : " hours tracked")) }
  }
  Timer { id: noticeTimer; interval: 3500; onTriggered: root.notice = "" }
  ParallelAnimation {
    id: entrance
    NumberAnimation { target: body; property: "opacity"; from: 0; to: 1; duration: 140; easing.type: Easing.OutCubic }
    NumberAnimation { target: body; property: "x"; from: Style.space(6); to: 0; duration: 140; easing.type: Easing.OutCubic }
  }
  onReducedMotionChanged: if (reducedMotion) { entrance.stop(); body.opacity = 1; body.x = 0 }

  Flickable {
    id: scroll
    anchors.fill: parent
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    Controls.ScrollBar.vertical: Controls.ScrollBar { policy: Controls.ScrollBar.AsNeeded }
    Column {
      id: content
      width: scroll.width
      spacing: Style.space(10)
      Row {
        width: parent.width
        spacing: Style.space(6)
        Action {
          width: parent.width - menuButton.width - parent.spacing
          text: root.page === "home" ? (root.project ? root.project.name : "Loading…") + " ▾" : "‹ " + root.pageTitle
          leftAlign: true
          onClicked: root.page === "home" ? root.navigate("projects") : root.back()
        }
        Action { id: menuButton; text: "⋯ Menu"; onClicked: root.navigate("commands") }
      }
      Caption {
        visible: root.page === "home" && text !== ""
        text: root.project ? root.project.clientName : ""
      }
      Loader {
        id: body
        width: parent.width
        sourceComponent: root.page === "home" ? homePage : root.picker ? pickerPage
          : root.page === "edit" ? editorPage : root.page === "project" ? projectPage
          : root.page === "reports" ? reportPage : root.page === "settings" ? settingsPage
          : root.page === "new-project" ? newProjectPage : root.page === "confirm" ? confirmPage : helpPage
      }
      Caption {
        objectName: "errorMessage"
        visible: text !== ""
        text: root.tracker.backendError || root.tracker.syncError || root.tracker.feedbackError || ""
        color: Color.urgent
      }
      Action {
        visible: !root.tracker.loaded
        text: "Reconnect to backend"
        onClicked: root.tracker.refresh()
      }
      Caption { visible: root.notice !== ""; text: root.notice; color: root.foreground }
      Ui.PanelSeparator { foreground: root.foreground }
      Caption {
        text: root.page === "home" ? "↑↓ Navigate   / Find   p Projects   ? Help"
          : root.picker ? "Type to filter · ↑↓ choose · Enter open · Esc back" : "Tab next · Shift+Tab previous · Esc back"
      }
    }
  }

  component Caption: Text {
    width: parent.width
    color: root.secondary
    textFormat: Text.PlainText
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
    wrapMode: Text.Wrap
  }
  component Action: Controls.AbstractButton {
    id: action
    property color foreground: root.foreground
    property bool leftAlign: false
    property bool bordered: false
    property bool hasCursor: false
    property string tooltipText: ""
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    padding: Style.space(8)
    implicitWidth: Math.min(root.width, actionLabel.implicitWidth + padding * 2 + Style.space(2))
    implicitHeight: Math.max(Style.space(30), actionLabel.implicitHeight + Style.space(12))
    enabled: root.pendingAction === ""
    opacity: enabled ? 1 : 0.55
    Keys.onReturnPressed: if (enabled) clicked()
    Keys.onEnterPressed: if (enabled) clicked()
    contentItem: Text {
      id: actionLabel
      text: action.text
      textFormat: Text.PlainText
      font.family: Style.font.family
      font.pixelSize: Style.font.body
      color: action.foreground
      elide: Text.ElideRight
      horizontalAlignment: action.leftAlign ? Text.AlignLeft : Text.AlignHCenter
      verticalAlignment: Text.AlignVCenter
    }
    background: Ui.BorderSurface {
      radius: Style.cornerRadius
      color: action.pressed ? Style.pressedFillFor(action.foreground, Color.accent)
        : Style.controlFill(action.activeFocus, action.hovered || action.hasCursor, action.foreground, Color.accent)
      borderSpec: action.activeFocus || action.hasCursor ? Border.controlSpec("focus", action.foreground, Color.accent)
        : action.bordered ? Border.controlSpec("normal", action.foreground, Color.accent) : Border.none()
      Behavior on color { ColorAnimation { duration: root.reducedMotion ? 0 : 80 } }
    }
    Controls.ToolTip.visible: hovered && tooltipText !== ""
    Controls.ToolTip.text: tooltipText
    Controls.ToolTip.delay: 500
    Accessible.role: Accessible.Button
    Accessible.name: text
    onActiveFocusChanged: if (activeFocus) {
      var p = mapToItem(content, 0, 0)
      if (p.y < scroll.contentY) scroll.contentY = p.y
      else if (p.y + height > scroll.contentY + scroll.height) scroll.contentY = Math.max(0, p.y + height - scroll.height)
    }
  }
  component Field: Column {
    property string label: ""
    property alias text: input.text
    property alias placeholder: input.placeholderText
    property alias input: input
    signal accepted()
    width: parent.width
    spacing: Style.space(4)
    function focusInput() { input.forceActiveFocus(); input.selectAll() }
    Caption { text: parent.label }
    Ui.TextField {
      id: input
      width: parent.width
      foreground: root.foreground
      placeholderTextColor: root.secondary
      Accessible.name: parent.label
      onAccepted: parent.accepted()
      onActiveFocusChanged: if (activeFocus) {
        var p = mapToItem(content, 0, 0)
        scroll.contentY = Math.max(0, Math.min(scroll.contentY, p.y))
        if (p.y + height > scroll.contentY + scroll.height) scroll.contentY = Math.max(0, p.y + height - scroll.height)
      }
    }
  }
  component Toggle: Action {
    checkable: true
    property string label: ""
    width: parent.width
    leftAlign: true
    text: (checked ? "☑ " : "☐ ") + label
    Accessible.role: Accessible.CheckBox
    Accessible.checked: checked
  }

  Component {
    id: homePage
    Column {
      id: home
      spacing: Style.space(12)
      function revealSelection() {
        var index = root.tasks.findIndex(function(t) { return t.id === root.selectedId })
        if (index >= 0) taskList.positionViewAtIndex(index, ListView.Contain)
      }
      Rectangle {
        width: parent.width
        height: hero.implicitHeight + Style.space(28)
        radius: Style.cornerRadius
        color: Style.normalFillFor(root.foreground, Color.accent)
        Column {
          id: hero
          x: Style.space(14); y: Style.space(14)
          width: parent.width - Style.space(28)
          spacing: Style.space(10)
          Caption {
            text: !root.tracker.loaded ? "CONNECTING" : root.heroTask ? root.heroTask.running ? "● TRACKING" : "○ PAUSED" : "READY WHEN YOU ARE"
            color: root.heroTask && root.heroTask.running ? Color.accent : root.secondary
            SequentialAnimation on opacity {
              running: root.panelOpen && !root.reducedMotion && !!root.heroTask && root.heroTask.running
              loops: 1
              NumberAnimation { from: 0.5; to: 1; duration: 180 }
            }
          }
          Caption {
            text: root.heroTask ? root.heroTask.title : "What are you working on?"
            color: root.foreground
            font.pixelSize: Style.font.heading
            maximumLineCount: 2
            elide: Text.ElideRight
          }
          Caption {
            text: root.heroTask ? TaskModel.formatDuration(root.tracker.displayTaskSeconds(root.heroTask)) : "00:00:00"
            color: root.foreground
            font.pixelSize: Style.font.displayLarge
            horizontalAlignment: Text.AlignHCenter
            topPadding: Style.space(6); bottomPadding: Style.space(6)
          }
          Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Style.space(8)
            Action {
              objectName: "primaryAction"
              text: root.heroTask ? (root.heroTask.running ? "Ⅱ Pause" : "▶ Resume") + " · Space" : "+ New task · n"
              bordered: true
              hasCursor: root.page === "home" && root.heroFocused && !root.actionsVisible && root.activeFocus
              onClicked: root.heroTask ? root.toggleTask(root.heroTask) : root.newTask()
            }
            Action { visible: !!root.heroTask; text: "Edit"; onClicked: root.editTask(root.heroTask) }
          }
        }
      }
      Action {
        visible: root.tracker.runningTimers > 1 || root.tracker.runningTimers > root.tracker.activeProjectRunningTimers
        text: root.tracker.runningTimers + " running · all projects →"
        onClicked: root.navigate("running")
      }
      Row {
        width: parent.width
        Caption { width: parent.width - add.width; anchors.verticalCenter: parent.verticalCenter; text: "TASKS" }
        Action { id: add; text: "+ New"; onClicked: root.newTask() }
      }
      ListView {
        id: taskList
        objectName: "taskList"
        width: parent.width
        height: Math.min(contentHeight, Style.space(180))
        clip: true
        model: root.tasks
        spacing: Style.space(3)
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: Controls.ScrollBar { policy: Controls.ScrollBar.AsNeeded }
        delegate: Ui.CursorSurface {
          id: taskRow
          required property var modelData
          width: taskList.width
          height: Style.space(36)
          foreground: root.foreground
          hasCursor: !root.heroFocused && root.selectedId === modelData.id && !root.actionsVisible
          Accessible.role: Accessible.ListItem
          Accessible.name: modelData.title + (modelData.running ? ", running, " : ", paused, ") + duration.text
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              root.selectedId = taskRow.modelData.id; root.heroFocused = false
              root.actionsVisible = false; root.forceActiveFocus(); root.toggleTask(taskRow.modelData)
            }
          }
          Text {
            id: marker
            x: Style.space(6); anchors.verticalCenter: parent.verticalCenter
            text: taskRow.hasCursor ? "›" : taskRow.modelData.running ? "●" : "○"
            color: taskRow.modelData.running ? Color.accent : root.secondary
            font.family: Style.font.family; font.pixelSize: Style.font.body
          }
          Caption {
            anchors.left: marker.right; anchors.leftMargin: Style.space(8)
            anchors.right: duration.left; anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            text: taskRow.modelData.title; color: root.foreground
            wrapMode: Text.NoWrap; elide: Text.ElideRight
          }
          Text {
            id: duration
            anchors.right: more.left; anchors.rightMargin: Style.space(4); anchors.verticalCenter: parent.verticalCenter
            text: TaskModel.formatDuration(root.tracker.displayTaskSeconds(taskRow.modelData))
            color: root.secondary; font.family: Style.font.family; font.pixelSize: Style.font.bodySmall
          }
          Action {
            id: more
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
            text: "⋯"; tooltipText: "Task actions"
            onClicked: { root.selectedId = taskRow.modelData.id; root.heroFocused = false; root.actionsVisible = !root.actionsVisible; root.actionIndex = 0; root.forceActiveFocus() }
          }
        }
      }
      Item {
        width: parent.width
        height: root.actionsVisible ? actionContent.implicitHeight : 0
        opacity: root.actionsVisible ? 1 : 0
        visible: height > 0
        enabled: root.actionsVisible
        clip: true
        Behavior on height { NumberAnimation { duration: root.reducedMotion ? 0 : 120; easing.type: Easing.OutCubic } }
        Behavior on opacity { NumberAnimation { duration: root.reducedMotion ? 0 : 120 } }
        Column {
          id: actionContent
          width: parent.width
          spacing: Style.space(4)
          Caption { text: "Actions · " + ((root.heroFocused ? root.heroTask : root.selectedTask) || { title: "" }).title }
          Flow {
            width: parent.width; spacing: Style.space(4)
            Repeater {
              model: ["Start / pause", "Edit", "Reset…", "Delete…"]
              Action {
                required property string modelData
                required property int index
                text: modelData; hasCursor: root.actionIndex === index
                onClicked: root.rowAction(index)
              }
            }
          }
        }
      }
      Caption { visible: root.tasks.length === 0; text: "Start with one small task. Press n to name it." }
      Caption { text: "Project total   " + root.tracker.activeProjectText; color: root.foreground }
      Action {
        width: parent.width; leftAlign: true
        text: root.tracker.syncError ? "Drive needs attention →" : root.tracker.state.drive && root.tracker.state.drive.remote
          ? root.tracker.syncStatus + " →" : "Drive not configured →"
        onClicked: root.navigate("settings")
      }
    }
  }

  Component {
    id: pickerPage
    Column {
      spacing: Style.space(8)
      function focusInitial() { search.forceActiveFocus() }
      function revealChoice() { results.positionViewAtIndex(root.choiceIndex, ListView.Contain) }
      Ui.TextField {
        id: search
        objectName: "searchInput"
        width: parent.width; foreground: root.foreground; placeholderTextColor: root.secondary
        placeholderText: "Type to filter…"
        Accessible.name: "Filter " + root.pageTitle
        onTextChanged: { root.query = text; root.choiceIndex = 0 }
        Keys.onDownPressed: root.move(1)
        Keys.onUpPressed: root.move(-1)
        onAccepted: root.choose(root.choiceIndex)
      }
      ListView {
        id: results
        width: parent.width; height: Math.min(contentHeight, Style.space(300))
        model: root.choices; clip: true; spacing: Style.space(3)
        Controls.ScrollBar.vertical: Controls.ScrollBar { policy: Controls.ScrollBar.AsNeeded }
        delegate: Action {
          required property var modelData
          required property int index
          width: results.width; leftAlign: true; hasCursor: root.choiceIndex === index
          text: (root.page === "projects" && root.project && root.project.id === modelData.id ? "✓ " : "")
            + (modelData.name || modelData.title) + (modelData.hint ? "  · " + modelData.hint : "")
          tooltipText: text
          onClicked: root.choose(index)
        }
      }
      Caption { visible: root.choices.length === 0; text: "No matches. Try another name." }
      Action { visible: root.page === "projects"; text: "+ Create project"; onClicked: root.navigate("new-project") }
    }
  }

  Component {
    id: editorPage
    Column {
      spacing: Style.space(12)
      function focusInitial() { title.focusInput() }
      function save() {
        if (!title.text.trim() || root.pendingAction !== "") return
        root.pendingAction = root.draftTask ? "task-edit" : "task-add"
        if (root.draftTask) root.tracker.renameAndAddManualTime(root.draftTask.id, title.text, duration.text)
        else root.tracker.addTask(title.text)
      }
      Field { id: title; label: "Task name"; text: root.draftTask ? root.draftTask.title : ""; placeholder: "What are you working on?"; onAccepted: save() }
      Field { id: duration; visible: !!root.draftTask; label: "Add time (optional)"; placeholder: "e.g. 25m or 1h30m"; onAccepted: save() }
      Caption { visible: !!root.draftTask; text: "Manual time is included in reports, but not the hourly click." }
      Action { text: root.pendingAction ? "Saving…" : "Save task"; bordered: true; enabled: title.text.trim() !== "" && root.pendingAction === ""; onClicked: save() }
    }
  }
  Component {
    id: newProjectPage
    Column {
      spacing: Style.space(12)
      function focusInitial() { name.focusInput() }
      function save() { if (name.text.trim() && root.pendingAction === "") { root.pendingAction = "project-create"; root.tracker.createProject(name.text) } }
      Field { id: name; label: "Project name"; placeholder: "A new beginning"; onAccepted: save() }
      Action { text: "Create project"; bordered: true; enabled: name.text.trim() !== "" && root.pendingAction === ""; onClicked: save() }
    }
  }
  Component {
    id: projectPage
    Column {
      spacing: Style.space(12)
      property string projectId: root.project ? root.project.id : ""
      function focusInitial() { name.focusInput() }
      Field { id: name; label: "Project name"; Component.onCompleted: text = root.project ? root.project.name : "" }
      Field { id: client; label: "Client (optional)"; Component.onCompleted: text = root.project ? root.project.clientName : "" }
      Field { id: company; label: "Prepared by (optional)"; Component.onCompleted: text = root.project ? root.project.companyName : "" }
      Action {
        text: "Save project"; bordered: true
        enabled: name.text.trim() !== "" && root.pendingAction === ""
        onClicked: { root.pendingAction = "project-update"; root.tracker.updateProject(projectId, { name: name.text, clientName: client.text, companyName: company.text }) }
      }
    }
  }
  Component {
    id: reportPage
    Column {
      spacing: Style.space(10)
      property string projectId: root.project ? root.project.id : ""
      function focusInitial() { weekly.forceActiveFocus() }
      Caption { text: "Automatic reports · " + (root.project ? root.project.name : "") }
      Toggle { id: weekly; label: "Weekly reports"; Component.onCompleted: checked = !!root.project && root.project.exportWeekly }
      Toggle { id: monthly; label: "Monthly reports"; Component.onCompleted: checked = !!root.project && root.project.exportMonthly }
      Toggle { id: detailed; label: "Detailed PDF (off = summary)"; Component.onCompleted: checked = !!root.project && root.project.templateId !== "summary" }
      Action {
        text: "Save report settings"; bordered: true
        onClicked: { root.pendingAction = "project-update"; root.tracker.updateProject(projectId, {
          exportWeekly: weekly.checked, exportMonthly: monthly.checked, templateId: detailed.checked ? "detailed" : "summary" }) }
      }
      Ui.PanelSeparator { foreground: root.foreground }
      Action { text: "Export previous completed week"; onClicked: { root.tracker.requestExport("weekly"); root.toast("Weekly export queued") } }
      Action { text: "Export previous completed month"; onClicked: { root.tracker.requestExport("monthly"); root.toast("Monthly export queued") } }
      Action { text: "Retry pending exports"; onClicked: root.tracker.retryReports() }
      Action {
        text: root.tracker.backgroundChecksEnabled ? "Disable background report checks" : "Enable background report checks"
        onClicked: root.tracker.setBackgroundChecks(!root.tracker.backgroundChecksEnabled)
      }
      Caption { text: root.tracker.reportStatus + "\n" + root.tracker.setupStatus }
    }
  }
  Component {
    id: settingsPage
    Column {
      spacing: Style.space(10)
      function focusInitial() { hourly.forceActiveFocus() }
      Caption { text: "FEEDBACK" }
      Toggle { id: hourly; label: "Wooden click each tracked hour"; Component.onCompleted: checked = root.preferences.hourlyClick }
      Caption { text: "Pauses do not count. Overlapping timers count once. No other actions make sounds." }
      Field {
        id: volume; label: "Click volume · 0–100%"
        Component.onCompleted: text = String(root.preferences.volume)
        input.validator: IntValidator { bottom: 0; top: 100 }
      }
      Action { text: "Preview wooden click"; enabled: volume.input.acceptableInput; onClicked: root.tracker.previewClick(Number(volume.text)) }
      Toggle { id: motion; label: "Reduced motion"; Component.onCompleted: checked = root.preferences.reducedMotion }
      Action {
        text: "Save preferences"; bordered: true; enabled: volume.input.acceptableInput && root.pendingAction === ""
        onClicked: { root.pendingAction = "preferences"; root.tracker.updatePreferences(hourly.checked, Number(volume.text), motion.checked) }
      }
      Ui.PanelSeparator { foreground: root.foreground }
      Caption { text: "GOOGLE DRIVE" }
      Field { id: remote; label: "rclone remote"; placeholder: "e.g. time-tracker"; Component.onCompleted: text = root.tracker.state.drive.remote || "" }
      Field { id: folder; label: "Drive folder"; Component.onCompleted: text = root.tracker.state.drive.folder || "OmaTracker" }
      Toggle { id: startup; label: "Sync local state on startup"; Component.onCompleted: checked = root.tracker.state.drive.syncOnStartup === true }
      Action {
        text: "Save Drive settings"; bordered: true
        onClicked: { root.pendingAction = "drive-update"; root.tracker.updateDrive(remote.text, folder.text, startup.checked) }
      }
      Action { text: "Sync now"; onClicked: { root.tracker.requestSync(); root.toast("Sync requested") } }
      Caption { text: root.tracker.syncStatus }
    }
  }
  Component {
    id: confirmPage
    Column {
      spacing: Style.space(12)
      function focusInitial() { cancel.forceActiveFocus() }
      Caption { text: root.confirmTitle; font.pixelSize: Style.font.heading; color: root.foreground }
      Caption { text: root.confirmAction === "delete" ? "The task leaves your list. Its recorded time remains in the report ledger."
        : "Visible counters start again at zero. Recorded time remains in reports; running timers keep running." }
      Action { id: cancel; text: "Cancel"; bordered: true; onClicked: root.back() }
      Action {
        text: root.confirmAction === "delete" ? "Delete task" : "Reset counters"; foreground: Color.urgent
        onClicked: {
          if (root.confirmAction === "delete") { root.pendingAction = "task-remove"; root.tracker.removeTask(root.confirmId) }
          else if (root.confirmAction === "reset") { root.pendingAction = "task-reset"; root.tracker.resetTimer(root.confirmId) }
          else { root.pendingAction = "task-reset-project"; root.tracker.resetActiveProject() }
        }
      }
    }
  }
  Component {
    id: helpPage
    Column {
      spacing: Style.space(8)
      Caption { text: "↑↓ / j k    Move between timer and tasks\nEnter / Space    Activate focused action\n←→ / h l    Task actions\nn / a    New task\ne    Edit focused task\nr / d / x    Confirm reset / delete\np    Project picker\n/    Find task\nCtrl+K    Search commands\n,    Preferences & Drive\n?    This help\nEsc    Back, then close\nTab / Shift+Tab    Next / previous bar panel\n\nInside forms, Tab moves between controls. Typing never triggers task shortcuts." }
      Action { text: "Back"; onClicked: root.back() }
    }
  }
}
