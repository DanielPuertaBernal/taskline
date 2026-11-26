import QtQuick
import qs.Commons

// Hover hint in the panel's own style (surface, border, rounded, small text),
// replacing Qt's default yellow tooltip. TasksPanel shows it after a delay,
// above the hovered control, and never while a popover is open.
Rectangle {
  id: tip

  property alias text: label.text
  property var ui: ({})

  visible: false
  width: label.implicitWidth + 16
  height: label.implicitHeight + 8
  radius: 6
  color: ui.surface || "transparent"
  border.color: ui.border || "transparent"
  border.width: 1

  Text {
    id: label
    anchors.centerIn: parent
    color: tip.ui.fg || "black"
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
