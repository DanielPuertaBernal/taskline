import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Centered task manager, sized like Omarchy's floating terminal (875x600):
// a pie of today's done vs pending tasks beside the full list, where each
// task can be completed, edited or deleted, plus a field to add new ones.
// The bar widget toggles it through the `danso.tasks-panel` IPC target;
// Escape, the close button or closing the window hides it. The Taskwarrior
// on-exit hook keeps it in sync.
Item {
  id: root

  property var shell: null

  property bool shown: false
  property var tasks: []
  property string editingUuid: ""

  readonly property int done: tasks.filter(t => t.status === "completed").length
  readonly property int pending: tasks.length - done
  readonly property real progress: tasks.length > 0 ? done / tasks.length : 0

  function refresh() {
    if (!exportProc.running) exportProc.running = true
  }

  function apply(json) {
    let list = []
    try { list = JSON.parse(json) } catch (e) { return }
    list.sort((a, b) => {
      if (a.status !== b.status) return a.status === "pending" ? -1 : 1
      return (b.urgency || 0) - (a.urgency || 0)
    })
    root.tasks = list
  }

  // argv is passed without a shell; splitting on whitespace lets Taskwarrior
  // parse modifiers such as project:Name or due:tomorrow.
  function words(text) {
    return String(text).trim().split(/\s+/).filter(w => w !== "")
  }

  function run(args) {
    if (actionProc.running) return
    actionProc.command = ["task", "rc.verbose=nothing", "rc.confirmation=off", "rc.bulk=0"].concat(args)
    actionProc.running = true
  }

  function addTask(text) {
    const w = words(text)
    if (w.length > 0) run(["add"].concat(w))
  }

  function toggleDone(task) {
    if (task.status === "pending") run([task.uuid, "done"])
    else run([task.uuid, "modify", "status:pending"])
  }

  function saveEdit(task, text) {
    const w = words(text)
    root.editingUuid = ""
    if (w.length > 0 && text.trim() !== task.description) run([task.uuid, "modify"].concat(w))
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

          Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Enter guarda · Esc cierra\nproject:X  due:friday  priority:H"
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

          TextField {
            id: input
            width: parent.width
            font.pixelSize: Style.font.title
            placeholderText: "Nueva tarea y Enter…"
            onAccepted: { root.addTask(text); text = "" }
          }

          ListView {
            id: list
            width: parent.width
            height: parent.height - input.height - parent.spacing
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

              Text {
                visible: !row.editing
                anchors {
                  left: check.right; leftMargin: 12
                  right: actions.left; rightMargin: 12
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
                onVisibleChanged: if (visible) { text = row.modelData.description; forceActiveFocus(); selectAll() }
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
