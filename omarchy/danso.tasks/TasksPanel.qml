import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "logic.js" as Logic

// Centered task manager, sized like Omarchy's floating terminal (875x600):
// a pie of today's done vs pending tasks beside the full list, where each
// task can be completed, edited or deleted, plus fields to add new ones with
// a due date and a priority. The list sorts by due date or priority; the
// choice is saved in ~/.local/state/taskline/omarchy-sort.
// Once the service loads (and every few hours after) it starts the
// taskline-update script detached; the script itself checks at most once a
// day, verifies the release checksum and honours the opt-out.
// The bar widget toggles it through the `danso.tasks-panel` IPC target;
// Escape, the close button or closing the window hides it. The Taskwarrior
// on-exit hook keeps it in sync.
Item {
  id: root

  property var shell: null

  property bool shown: false
  property var tasks: []
  property string editingUuid: ""
  property string sortMode: "due"
  property string newPriority: ""
  property var now: new Date()

  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/taskline"
  // The shell loads plugins through a qs:/// URL; strip any scheme to get the path.
  readonly property string updater: Qt.resolvedUrl("taskline-update").toString().replace(/^[a-z]+:\/\//, "")
  readonly property var dueColors: ({ overdue: "#f7768e", today: "#ff9e64", upcoming: "#7dcfff" })
  readonly property var priorityColors: ({ H: "#f7768e", M: "#e0af68", L: "#9ece6a" })

  readonly property int done: tasks.filter(t => t.status === "completed").length
  readonly property int pending: tasks.length - done
  readonly property real progress: tasks.length > 0 ? done / tasks.length : 0

  function refresh() {
    if (!exportProc.running) exportProc.running = true
  }

  function apply(json) {
    let list = []
    try { list = JSON.parse(json) } catch (e) { return }
    root.now = new Date()
    root.tasks = Logic.sortTasks(list, root.sortMode)
  }

  function setSort(mode) {
    root.sortMode = mode === "priority" ? "priority" : "due"
    root.tasks = Logic.sortTasks(root.tasks, root.sortMode)
  }

  function toggleSort() {
    setSort(root.sortMode === "due" ? "priority" : "due")
    saveSortProc.command = ["sh", "-c", 'mkdir -p "$1" && printf "%s\\n" "$2" > "$1/omarchy-sort"',
                            "sh", root.stateDir, root.sortMode]
    saveSortProc.running = true
  }

  function cyclePriority() {
    const order = ["", "H", "M", "L"]
    root.newPriority = order[(order.indexOf(root.newPriority) + 1) % order.length]
  }

  function run(args) {
    if (actionProc.running) return
    actionProc.command = ["task", "rc.verbose=nothing", "rc.confirmation=off", "rc.bulk=0"].concat(args)
    actionProc.running = true
  }

  // argv is passed without a shell; Logic splits on whitespace so Taskwarrior
  // parses modifiers such as project:Name, due:+3d or priority:H.
  function addTask(text, due) {
    const args = Logic.addArgs(text, due, root.newPriority)
    if (args.length === 0) return false
    run(["add"].concat(args))
    root.newPriority = ""
    return true
  }

  function toggleDone(task) {
    if (task.status === "pending") run([task.uuid, "done"])
    else run([task.uuid, "modify", "status:pending"])
  }

  function saveEdit(task, text) {
    root.editingUuid = ""
    if (text.trim() === Logic.editText(task)) return
    const args = Logic.editArgs(task, text)
    if (args.length > 0) run([task.uuid, "modify"].concat(args))
  }

  function deleteTask(task) {
    run([task.uuid, "delete"])
  }

  function open() {
    root.shown = true
    root.refresh()
  }

  function close() {
    root.shown = false
    root.editingUuid = ""
  }

  IpcHandler {
    target: "danso.tasks-panel"

    function refresh(): void { root.refresh() }
    function toggle(): void { if (root.shown) root.close(); else root.open() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
  }

  // Hooks are off so this export does not re-trigger the refresh hook.
  Process {
    id: exportProc
    command: ["task", "rc.hooks=off", "rc.verbose=nothing", "rc.confirmation=off",
              "( status:pending or ( status:completed and end.after:today ) )", "export"]
    stdout: StdioCollector {
      onStreamFinished: root.apply(text)
    }
  }

  // Hooks stay on here so the on-exit hook also refreshes the bar widget.
  Process {
    id: actionProc
    onExited: root.refresh()
  }

  Process {
    id: loadSortProc
    running: true
    command: ["sh", "-c", 'cat "$1/omarchy-sort" 2>/dev/null', "sh", root.stateDir]
    stdout: StdioCollector {
      onStreamFinished: root.setSort(text.trim())
    }
  }

  Process { id: saveSortProc }

  // setsid -f detaches the updater, so reloading the plugins after an update
  // (which destroys this service) does not kill it halfway.
  Process {
    id: updateProc
    command: ["setsid", "-f", "bash", root.updater, "--auto"]
  }

  Timer {
    interval: 15000
    running: true
    repeat: false
    onTriggered: { if (!updateProc.running) updateProc.running = true; updateTimer.running = true }
  }

  Timer {
    id: updateTimer
    interval: 6 * 3600 * 1000
    repeat: true
    onTriggered: if (!updateProc.running) updateProc.running = true
  }

  Timer {
    interval: 60000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  // A regular toplevel window rather than a layer surface, so Hyprland manages
  // it like any other: Super+drag moves it, it can change monitors, and focus
  // follows clicks. ~/.config/hypr/hyprland.lua matches this title and gives
  // it the floating-window tag (float, centered, 875x600).
  FloatingWindow {
    id: panel
    title: "Tareas"
    visible: root.shown
    implicitWidth: 875
    implicitHeight: 600
    color: Commons.Color.background

    onVisibleChanged: {
      if (visible) input.forceActiveFocus()
      else root.close()
    }

    Rectangle {
      anchors.fill: parent
      color: "transparent"
      focus: true

      Keys.onEscapePressed: {
        if (root.editingUuid !== "") root.editingUuid = ""
        else root.close()
      }

      Text {
        z: 1
        anchors { top: parent.top; right: parent.right; margins: 16 }
        text: ""
        color: closeArea.containsMouse ? Commons.Color.urgent : Commons.Color.muted
        font.pixelSize: Style.font.heading
        MouseArea {
          id: closeArea
          anchors.fill: parent
          anchors.margins: -8
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.close()
        }
      }

      Row {
        anchors { fill: parent; margins: 24; rightMargin: 48 }
        spacing: 24

        // ---- Summary: title, pie and legend ----
        Column {
          id: summary
          width: 240
          spacing: 20

          Text {
            text: "Tareas de hoy"
            color: Commons.Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.display
            font.bold: true
          }

          Canvas {
            id: pie
            width: 220
            height: 220

            property real value: root.progress
            property color doneColor: Commons.Color.accent
            property color pendingColor: Commons.Color.muted
            property color holeColor: Commons.Color.background
            onValueChanged: requestPaint()
            onDoneColorChanged: requestPaint()
            onPendingColorChanged: requestPaint()
            onHoleColorChanged: requestPaint()

            onPaint: {
              const ctx = getContext("2d")
              const c = width / 2
              const start = -Math.PI / 2
              const split = start + value * 2 * Math.PI
              ctx.reset()
              ctx.fillStyle = pendingColor
              ctx.beginPath(); ctx.moveTo(c, c); ctx.arc(c, c, c, split, start + 2 * Math.PI); ctx.closePath(); ctx.fill()
              if (value > 0) {
                ctx.fillStyle = doneColor
                ctx.beginPath(); ctx.moveTo(c, c); ctx.arc(c, c, c, start, split); ctx.closePath(); ctx.fill()
              }
              // Donut hole so the percentage reads cleanly on top.
              ctx.fillStyle = holeColor
              ctx.beginPath(); ctx.arc(c, c, c * 0.58, 0, 2 * Math.PI); ctx.fill()
            }

            Text {
              anchors.centerIn: parent
              text: Math.round(root.progress * 100) + "%"
              color: Commons.Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.displayLarge
              font.bold: true
            }
          }

          Column {
            spacing: 8

            Text {
              text: "● Hechas: " + root.done
              color: Commons.Color.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.heading
            }
            Text {
              text: "● Pendientes: " + root.pending
              color: Commons.Color.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.heading
            }
          }

          Rectangle {
            width: sortLabel.implicitWidth + 24
            height: sortLabel.implicitHeight + 10
            radius: 6
            color: sortArea.containsMouse ? Qt.alpha(Commons.Color.accent, 0.2) : Qt.alpha(Commons.Color.accent, 0.08)

            Text {
              id: sortLabel
              anchors.centerIn: parent
              text: "⇅  Orden: " + (root.sortMode === "priority" ? "prioridad" : "fecha")
              color: Commons.Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.title
            }
            MouseArea {
              id: sortArea
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleSort()
            }
          }

          Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Enter guarda · Esc cierra\nFecha: hoy, mañana, +3d, 2026-12-01\nproject:X  due:friday  priority:H"
            color: Commons.Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        // ---- Task list and input ----
        Column {
          width: parent.width - summary.width - parent.spacing
          height: parent.height
          spacing: 12

          Row {
            id: inputRow
            width: parent.width
            spacing: 8

            function submit() {
              if (root.addTask(input.text, dueInput.text)) { input.text = ""; dueInput.text = "" }
              input.forceActiveFocus()
            }

            TextField {
              id: input
              width: parent.width - dueInput.width - priorityButton.width - 2 * parent.spacing
              font.pixelSize: Style.font.title
              placeholderText: "Nueva tarea y Enter…"
              onAccepted: inputRow.submit()
            }

            TextField {
              id: dueInput
              width: 150
              font.pixelSize: Style.font.title
              placeholderText: "fecha: hoy, +3d"
              onAccepted: inputRow.submit()
            }

            Rectangle {
              id: priorityButton
              width: 44
              height: input.height
              radius: 6
              color: Qt.alpha(Commons.Color.accent, priorityArea.containsMouse ? 0.2 : 0.08)

              Text {
                anchors.centerIn: parent
                text: root.newPriority === "" ? "–" : root.newPriority
                color: root.priorityColors[root.newPriority] || Commons.Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.title
                font.bold: true
              }
              MouseArea {
                id: priorityArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.cyclePriority()
              }
            }
          }

          ListView {
            id: list
            width: parent.width
            height: parent.height - inputRow.height - parent.spacing
            clip: true
            spacing: 4
            model: root.tasks
            boundsBehavior: Flickable.StopAtBounds

            delegate: Rectangle {
              id: row
              required property var modelData
              readonly property bool isDone: modelData.status === "completed"
              readonly property bool editing: root.editingUuid === modelData.uuid

              width: ListView.view.width
              height: 40
              radius: 8
              color: hover.hovered ? Qt.alpha(Commons.Color.accent, 0.12) : "transparent"

              HoverHandler { id: hover }

              // Done / undo toggle.
              Text {
                id: check
                anchors { left: parent.left; leftMargin: 8; verticalCenter: parent.verticalCenter }
                text: row.isDone ? "" : ""
                color: row.isDone ? Commons.Color.accent : Commons.Color.popups.text
                font.pixelSize: Style.font.heading
                MouseArea {
                  anchors.fill: parent
                  anchors.margins: -6
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.toggleDone(row.modelData)
                }
              }

              // Priority marker: H red, M yellow, L green.
              Rectangle {
                id: marker
                anchors { left: check.right; leftMargin: 8; verticalCenter: parent.verticalCenter }
                width: 4
                height: 20
                radius: 2
                color: root.priorityColors[row.modelData.priority] || "transparent"
              }

              Text {
                id: dueLabel
                visible: !row.editing && text !== ""
                anchors { right: actions.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
                text: Logic.dueLabel(row.modelData, root.now)
                color: root.dueColors[Logic.dueState(row.modelData, root.now)] || Commons.Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.title
              }

              Text {
                visible: !row.editing
                anchors {
                  left: marker.right; leftMargin: 10
                  right: dueLabel.visible ? dueLabel.left : actions.left; rightMargin: 12
                  verticalCenter: parent.verticalCenter
                }
                elide: Text.ElideRight
                text: row.modelData.description
                  + (row.modelData.project ? "   · " + row.modelData.project : "")
                color: row.isDone ? Commons.Color.muted : Commons.Color.popups.text
                font.strikeout: row.isDone
                font.family: Style.font.family
                font.pixelSize: Style.font.title
              }

              TextField {
                id: editField
                visible: row.editing
                anchors {
                  left: check.right; leftMargin: 8
                  right: actions.left; rightMargin: 8
                  verticalCenter: parent.verticalCenter
                }
                font.pixelSize: Style.font.title
                onVisibleChanged: if (visible) { text = Logic.editText(row.modelData); forceActiveFocus(); selectAll() }
                onAccepted: root.saveEdit(row.modelData, text)
              }

              Row {
                id: actions
                anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                spacing: 16
                opacity: hover.hovered || row.editing ? 1 : 0.35

                Text {
                  text: ""
                  color: Commons.Color.popups.text
                  font.pixelSize: Style.font.heading
                  MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.editingUuid = row.editing ? "" : row.modelData.uuid
                  }
                }
                Text {
                  text: ""
                  color: Commons.Color.urgent
                  font.pixelSize: Style.font.heading
                  MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.deleteTask(row.modelData)
                  }
                }
              }
            }

            Text {
              anchors.centerIn: parent
              visible: root.tasks.length === 0
              text: "Sin tareas. Agrega la primera arriba."
              color: Commons.Color.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.title
            }
          }
        }
      }
    }
  }
}
