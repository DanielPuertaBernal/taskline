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
// a due date and a priority. Focusing or clicking the date field (or the
// calendar button while editing) opens a month calendar (DatePicker.qml);
// typed shortcuts keep working. The list sorts by due date or priority; the
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
  // Field the calendar writes to, and whether it is a row's edit field
  // (then it rewrites the due: word instead of replacing the text).
  property var pickerField: null
  property bool pickerEdit: false

  onEditingUuidChanged: {
    if (root.pickerEdit) datePicker.visible = false
    if (root.priorityField) priorityPicker.visible = false
  }

  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/taskline"
  // The shell loads plugins through a qs:/// URL; strip any scheme to get the path.
  readonly property string updater: Qt.resolvedUrl("taskline-update").toString().replace(/^[a-z]+:\/\//, "")
  // Theme: "system" uses the Omarchy theme's own colors; "light" / "dark"
  // force Tokyo Night Day / Tokyo Night. Saved as theme= in
  // ~/.config/taskline/omarchy.conf (the file that also holds auto_update).
  property string themeMode: "system"
  readonly property string configDir: (Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/taskline"
  readonly property bool systemLight: Logic.isLight(Commons.Color.background.r, Commons.Color.background.g,
                                                    Commons.Color.background.b)
  readonly property bool lightTheme: themeMode === "system" ? systemLight : themeMode === "light"
  readonly property var ui: themeMode === "system" ? ({
    bg: Commons.Color.background, surface: Commons.Color.popups.background,
    border: Qt.alpha(Commons.Color.popups.border, 0.6), hover: Qt.alpha(Commons.Color.accent, 0.12),
    fg: Commons.Color.popups.text, muted: Commons.Color.muted, track: Commons.Color.muted,
    accent: Commons.Color.accent, red: Commons.Color.urgent
  }) : Logic.themePalette(themeMode)
  // Status colors (due, priority, calendar dots), Day variants on light themes.
  readonly property var colors: Logic.palette(lightTheme)
  readonly property var priorityOptions: Logic.PRIORITY_OPTIONS.map(o => ({
    value: o.value, label: o.label, iconUrl: Logic.priorityIconUrl(o.value, root.colors.priority[o.value] || "")
  }))

  // Hover hints (Tip.qml): shown after a delay, above the control, never
  // while a popover is open.
  property var tipAnchor: null
  property string tipText: ""
  readonly property var dueColors: colors.due
  readonly property var priorityColors: colors.priority
  // Field the priority popover writes to when editing (null: the add row).
  property var priorityField: null

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

  function setTheme(mode) {
    root.themeMode = mode === "light" || mode === "dark" ? mode : "system"
    saveThemeProc.command = ["sh", "-c",
      'mkdir -p "$1" && f="$1/omarchy.conf" && touch "$f" && { grep -v "^[[:space:]]*theme[[:space:]]*=" "$f"; echo "theme=$2"; } > "$f.tmp" && mv "$f.tmp" "$f"',
      "sh", root.configDir, root.themeMode]
    saveThemeProc.running = true
  }

  // Which add-row trigger is under a point of the panel: "date", "priority" or "".
  function triggerAt(x, y) {
    const hit = item => {
      const p = item.mapToItem(surface, 0, 0)
      return x >= p.x && x <= p.x + item.width && y >= p.y && y <= p.y + item.height
    }
    return hit(dueInput) ? "date" : hit(priorityButton) ? "priority" : ""
  }

  // Popover rules: a trigger toggles its own popover, opens it while the other one
  // closes, and "" (anywhere else) only closes. One popover at a time.
  function toggleTrigger(which) {
    const dateOpen = datePicker.visible && !root.pickerEdit
    const priorityOpen = priorityPicker.visible && !root.priorityField
    datePicker.visible = false
    priorityPicker.visible = false
    if (which === "date" && !dateOpen) {
      dueInput.forceActiveFocus()
      root.openPicker(dueInput, dueInput, false)
    } else if (which === "priority" && !priorityOpen) {
      root.openPriority(priorityButton, null)
    }
  }

  function showTip(anchor, text) {
    root.tipAnchor = anchor
    root.tipText = text
    tipTimer.restart()
  }

  function hideTip(anchor) {
    if (anchor && root.tipAnchor !== anchor) return
    tipTimer.stop()
    tip.visible = false
    root.tipAnchor = null
  }

  function placeTip() {
    const a = root.tipAnchor
    if (!a || !a.visible || datePicker.visible || priorityPicker.visible) return
    tip.text = root.tipText
    const p = a.mapToItem(surface, 0, 0)
    tip.x = Math.max(8, Math.min(p.x + a.width / 2 - tip.width / 2, surface.width - tip.width - 8))
    tip.y = p.y - tip.height - 6 >= 4 ? p.y - tip.height - 6 : p.y + a.height + 6
    tip.visible = true
  }

  // Place a popover under `anchor`, or above it when there is no room.
  function placePopover(popover, anchor) {
    const p = anchor.mapToItem(surface, 0, anchor.height + 6)
    popover.x = Math.max(12, Math.min(p.x + anchor.width - popover.width, surface.width - popover.width - 12))
    popover.y = p.y + popover.height > surface.height - 12 ? p.y - anchor.height - 12 - popover.height : p.y
  }

  // Open the priority popover under `anchor`; `field` is a row's edit field or null.
  function openPriority(anchor, field) {
    root.hideTip()
    root.priorityField = field
    datePicker.visible = false
    let current = root.newPriority
    if (field) {
      const m = String(field.text).match(/(?:^|\s)(?:priority|pri):(\S+)/)
      current = m ? m[1].toUpperCase() : ""
    }
    priorityPicker.openWith(current)
    placePopover(priorityPicker, anchor)
  }

  function priorityPicked(p) {
    const field = root.priorityField
    if (field) {
      field.text = Logic.setPriorityToken(field.text, p)
      field.forceActiveFocus()
      field.cursorPosition = field.text.length
    } else {
      root.newPriority = p
      input.forceActiveFocus()
    }
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
    root.hideTip()
    datePicker.visible = false
    priorityPicker.visible = false
    root.shown = false
    root.editingUuid = ""
  }

  function dueIsoIn(text, isEdit) {
    let value = text
    if (isEdit) {
      const m = String(text).match(/(?:^|\s)due:(\S+)/)
      value = m ? m[1] : ""
    }
    const d = Logic.parseQuickDate(value, new Date())
    return d ? Logic.isoDay(d) : ""
  }

  // Open the calendar under `anchor` (above it when there is no room).
  function openPicker(field, anchor, isEdit) {
    root.hideTip()
    root.pickerField = field
    root.pickerEdit = isEdit
    priorityPicker.visible = false
    datePicker.openWith(dueIsoIn(field.text, isEdit))
    placePopover(datePicker, anchor)
  }

  function pickerPicked(iso) {
    const field = root.pickerField
    if (!field) return
    field.text = root.pickerEdit ? Logic.setDueToken(field.text, iso) : iso
    field.forceActiveFocus()
    if (root.pickerEdit) field.cursorPosition = field.text.length
  }

  IpcHandler {
    target: "danso.tasks-panel"

    function refresh(): void { root.refresh() }
    function toggle(): void { if (root.shown) root.close(); else root.open() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    // light, dark or system (saved like the panel's own theme switch).
    function setTheme(mode: string): string { root.setTheme(mode); return root.themeMode }
    // Open the panel with the calendar of the date field showing.
    function calendar(): void {
      root.open()
      // The window takes focus after it is mapped; focus the date field then.
      calendarFocusTimer.restart()
    }
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

  Process {
    id: loadThemeProc
    running: true
    command: ["sh", "-c", 'cat "$1/omarchy.conf" 2>/dev/null', "sh", root.configDir]
    stdout: StdioCollector {
      onStreamFinished: root.themeMode = Logic.themeFromConf(text)
    }
  }

  Process { id: saveThemeProc }

  Timer {
    id: calendarFocusTimer
    interval: 200
    onTriggered: { dueInput.forceActiveFocus(); root.openPicker(dueInput, dueInput, false) }
  }

  Timer {
    id: tipTimer
    interval: 450
    onTriggered: root.placeTip()
  }

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
    color: root.ui.bg

    onVisibleChanged: {
      if (visible) input.forceActiveFocus()
      else root.close()
    }

    Rectangle {
      id: surface
      anchors.fill: parent
      color: "transparent"
      focus: true

      Keys.onEscapePressed: {
        if (priorityPicker.visible) priorityPicker.close()
        else if (datePicker.visible) datePicker.close()
        else if (root.editingUuid !== "") root.editingUuid = ""
        else root.close()
      }

      Text {
        id: closeButton
        z: 1
        anchors { top: parent.top; right: parent.right; margins: 16 }
        text: ""
        color: closeArea.containsMouse ? root.ui.red : root.ui.muted
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
            color: root.ui.fg
            font.family: Style.font.family
            font.pixelSize: Style.font.display
            font.bold: true
          }

          Canvas {
            id: pie
            width: 220
            height: 220

            property real value: root.progress
            property color doneColor: root.ui.accent
            property color pendingColor: root.ui.track
            property color holeColor: root.ui.bg
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
              color: root.ui.fg
              font.family: Style.font.family
              font.pixelSize: Style.font.displayLarge
              font.bold: true
            }
          }

          Column {
            spacing: 8

            Text {
              text: "● Hechas: " + root.done
              color: root.ui.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.heading
            }
            Text {
              text: "● Pendientes: " + root.pending
              color: root.ui.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.heading
            }
          }

          Rectangle {
            width: sortLabel.implicitWidth + 24
            height: sortLabel.implicitHeight + 10
            radius: 6
            color: sortArea.containsMouse ? Qt.alpha(root.ui.accent, 0.2) : Qt.alpha(root.ui.accent, 0.08)

            Text {
              id: sortLabel
              anchors.centerIn: parent
              text: "⇅  Orden: " + (root.sortMode === "priority" ? "prioridad" : "fecha")
              color: root.ui.fg
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

          // Tema: Claro / Oscuro switch, plus "Auto" to follow the Omarchy theme.
          ThemeSwitch {
            ui: root.ui
            mode: root.themeMode
            effectiveLight: root.lightTheme
            onChosen: function(mode) { root.setTheme(mode) }
          }

          Text {
            width: parent.width
            topPadding: 6
            wrapMode: Text.WordWrap
            lineHeight: 1.25
            text: "Enter guarda · Esc cierra\nFecha: calendario o hoy, +3d,\n2026-12-01 · due:friday\npriority:H · project:X"
            color: root.ui.muted
            opacity: 0.85
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
              foreground: root.ui.fg
              accent: root.ui.accent
              placeholderTextColor: root.ui.muted
              width: parent.width - dueInput.width - priorityButton.width - 2 * parent.spacing
              font.pixelSize: Style.font.title
              placeholderText: "Nueva tarea y Enter…"
              onAccepted: inputRow.submit()
            }

            TextField {
              id: dueInput
              foreground: root.ui.fg
              accent: root.ui.accent
              placeholderTextColor: root.ui.muted
              width: 150
              font.pixelSize: Style.font.title
              placeholderText: "\uf073  fecha"
              onAccepted: inputRow.submit()
              // The calendar opens only from a click on this field (or Alt+Down here),
              // never just because the field got focus.
              onActiveFocusChanged: if (!activeFocus && !root.pickerEdit) datePicker.visible = false
              onPressed: root.toggleTrigger("date")
              onTextChanged: if (datePicker.visible && !root.pickerEdit) datePicker.followText(text)
              Keys.onPressed: function(event) {
                if (event.key === Qt.Key_Down && (event.modifiers & Qt.AltModifier) && !datePicker.visible) {
                  root.openPicker(dueInput, dueInput, false)
                  event.accepted = true
                } else if (!root.pickerEdit && datePicker.handleKey(event, text)) event.accepted = true
              }
            }

            Rectangle {
              id: priorityButton
              width: 44
              height: input.height
              radius: 6
              color: Qt.alpha(root.ui.accent, priorityArea.containsMouse ? 0.2 : 0.08)

              Text {
                anchors.centerIn: parent
                visible: root.newPriority === ""
                text: "–"
                color: root.ui.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.title
                font.bold: true
              }
              Image {
                anchors.centerIn: parent
                visible: root.newPriority !== ""
                width: 22
                height: 22
                sourceSize: Qt.size(44, 44)
                source: Logic.priorityIconUrl(root.newPriority, root.priorityColors[root.newPriority] || "")
              }
              MouseArea {
                id: priorityArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.toggleTrigger("priority")
                onContainsMouseChanged: containsMouse
                  ? root.showTip(priorityButton, "Prioridad: " + (Logic.priorityName(root.newPriority) || "sin prioridad").toLowerCase())
                  : root.hideTip(priorityButton)
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
              color: hover.hovered ? Qt.alpha(root.ui.accent, 0.12) : "transparent"

              HoverHandler { id: hover }

              // Done / undo toggle.
              Text {
                id: check
                anchors { left: parent.left; leftMargin: 8; verticalCenter: parent.verticalCenter }
                text: row.isDone ? "" : ""
                color: row.isDone ? root.ui.accent : root.ui.fg
                font.pixelSize: Style.font.heading
                MouseArea {
                  anchors.fill: parent
                  anchors.margins: -6
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.toggleDone(row.modelData)
                }
              }

              // Priority icon (Lucide): Alta red chevrons, Media yellow, Baja green.
              Image {
                id: marker
                anchors { left: check.right; leftMargin: 8; verticalCenter: parent.verticalCenter }
                width: 20
                height: 20
                sourceSize: Qt.size(40, 40)
                source: Logic.priorityIconUrl(row.modelData.priority, root.priorityColors[row.modelData.priority] || "")
                HoverHandler {
                  onHoveredChanged: hovered && marker.source != ""
                    ? root.showTip(marker, "Prioridad " + Logic.priorityName(row.modelData.priority).toLowerCase())
                    : root.hideTip(marker)
                }
              }

              Text {
                id: dueLabel
                visible: !row.editing && text !== ""
                anchors { right: actions.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
                text: Logic.dueLabel(row.modelData, root.now)
                color: root.dueColors[Logic.dueState(row.modelData, root.now)] || root.ui.muted
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
                color: row.isDone ? root.ui.muted : root.ui.fg
                font.strikeout: row.isDone
                font.family: Style.font.family
                font.pixelSize: Style.font.title
              }

              TextField {
                id: editField
                foreground: root.ui.fg
                accent: root.ui.accent
                placeholderTextColor: root.ui.muted
                visible: row.editing
                anchors {
                  left: check.right; leftMargin: 8
                  right: actions.left; rightMargin: 8
                  verticalCenter: parent.verticalCenter
                }
                font.pixelSize: Style.font.title
                onVisibleChanged: if (visible) { text = Logic.editText(row.modelData); forceActiveFocus(); selectAll() }
                onAccepted: root.saveEdit(row.modelData, text)
                onTextChanged: {
                  if (!datePicker.visible || root.pickerField !== editField) return
                  const m = text.match(/(?:^|\s)due:(\S+)/)
                  if (m) datePicker.followText(m[1])
                }
                Keys.onPressed: function(event) {
                  if (root.pickerEdit && root.pickerField === editField && datePicker.handleKey(event, ""))
                    event.accepted = true
                }
              }

              Row {
                id: actions
                anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                spacing: 16
                opacity: hover.hovered || row.editing ? 1 : 0.35

                // Priority for the task, while editing.
                Item {
                  id: editPriorityButton
                  visible: row.editing
                  width: 22
                  height: 22
                  anchors.verticalCenter: parent.verticalCenter
                  Image {
                    anchors.fill: parent
                    sourceSize: Qt.size(44, 44)
                    readonly property string current: (String(editField.text).match(/(?:^|\s)(?:priority|pri):(\S+)/) || ["", ""])[1].toUpperCase()
                    source: current ? Logic.priorityIconUrl(current, root.priorityColors[current] || "")
                                    : Logic.priorityIconUrl("M", root.ui.muted)
                    opacity: current ? 1 : 0.5
                  }
                  MouseArea {
                    id: editPriorityArea
                    anchors.fill: parent
                    anchors.margins: -6
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: priorityPicker.visible && root.priorityField === editField
                      ? priorityPicker.close() : root.openPriority(editPriorityButton, editField)
                    onContainsMouseChanged: containsMouse ? root.showTip(editPriorityButton, "Prioridad")
                                                          : root.hideTip(editPriorityButton)
                  }
                }

                // Calendar for the task's due date, while editing.
                Text {
                  id: calendarButton
                  visible: row.editing
                  text: "\uf073"
                  color: root.ui.accent
                  font.pixelSize: Style.font.heading
                  MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    hoverEnabled: true
                    onContainsMouseChanged: containsMouse ? root.showTip(calendarButton, "Fecha")
                                                          : root.hideTip(calendarButton)
                    onClicked: {
                      if (datePicker.visible && root.pickerField === editField) datePicker.close()
                      else root.openPicker(editField, calendarButton, true)
                    }
                  }
                }

                Text {
                  text: ""
                  color: root.ui.fg
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
                  color: root.ui.red
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
              color: root.ui.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.title
            }
          }
        }
      }

      // A click outside an open popover closes it and is not passed on, unless it lands
      // on the other popover's trigger (date field / priority button), which opens it.
      MouseArea {
        anchors.fill: parent
        z: 19
        enabled: datePicker.visible || priorityPicker.visible
        onClicked: function(mouse) { root.toggleTrigger(root.triggerAt(mouse.x, mouse.y)) }
      }

      OptionMenu {
        id: priorityPicker
        z: 20
        ui: root.ui
        options: root.priorityOptions
        onPicked: function(p) { root.priorityPicked(p) }
      }

      Tip {
        id: tip
        z: 30
        ui: root.ui
      }

      DatePicker {
        id: datePicker
        z: 20
        dueDays: Logic.dueDays(root.tasks)
        dotColor: root.colors.dot
        ui: root.ui
        onPicked: function(iso) { root.pickerPicked(iso) }
        onDismissed: if (root.pickerField) root.pickerField.forceActiveFocus()
      }
    }
  }
}
