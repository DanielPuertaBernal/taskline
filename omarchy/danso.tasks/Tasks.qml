import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Daily Taskwarrior progress for the bar: tasks completed today against
// today's completed plus every task still pending. A Taskwarrior on-exit hook
// calls the `danso.tasks refresh` IPC so the bar moves as soon as a task
// changes; the timer only covers the day rollover and edits made elsewhere.
BarWidget {
  id: root
  moduleName: "danso.tasks"

  readonly property int segments: setting("segments", 10)

  property bool available: false
  property int done: 0
  property int total: 0
  property var pendingTitles: []

  readonly property real progress: total > 0 ? done / total : 0
  readonly property string progressBar: {
    const filled = Math.round(progress * segments)
    return "▰".repeat(filled) + "▱".repeat(segments - filled)
  }

  function refresh() {
    if (!exportProc.running) exportProc.running = true
  }

  function apply(json) {
    let tasks = []
    try { tasks = JSON.parse(json) } catch (e) { return }
    const pending = tasks.filter(t => t.status === "pending")
    pending.sort((a, b) => (b.urgency || 0) - (a.urgency || 0))
    root.done = tasks.length - pending.length
    root.total = tasks.length
    root.pendingTitles = pending.slice(0, 5).map(t => "• " + t.description)
    root.available = true
  }

  visible: available && total > 0
  implicitWidth: visible ? button.implicitWidth : 0
  implicitHeight: button.implicitHeight

  IpcHandler {
    target: "danso.tasks"

    function refresh(): void { root.broadcast("refresh") }
  }

  // Hooks are off so this export does not fire the on-exit hook that
  // triggers it, which would refresh in a loop.
  Process {
    id: exportProc
    command: ["task", "rc.hooks=off", "rc.verbose=nothing", "rc.confirmation=off",
              "( status:pending or ( status:completed and end.after:today ) )", "export"]
    stdout: StdioCollector {
      onStreamFinished: root.apply(text)
    }
    onExited: function(exitCode) {
      if (exitCode !== 0) root.available = false
    }
  }

  Timer {
    interval: 60000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "  " + root.progressBar + "  " + root.done + "/" + root.total
    active: root.total > 0 && root.done === root.total
    tooltipText: root.pendingTitles.length > 0
      ? "Pendientes:\n" + root.pendingTitles.join("\n")
      : "Todo listo por hoy"

    onPressed: function(b) {
      if (!root.bar) return
      if (b === Qt.RightButton) root.bar.run("omarchy-launch-floating-terminal-with-presentation task next")
      else root.bar.run("omarchy-shell danso.tasks-panel toggle")
    }
  }
}
