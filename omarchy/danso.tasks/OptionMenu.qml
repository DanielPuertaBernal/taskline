import QtQuick
import qs.Commons
import "logic.js" as Logic

// Small popover listing choices, each with an icon (data: URL) or a Nerd Font
// glyph and a label. Used for the priority (Alta, Media, Baja, Sin prioridad)
// and for the theme (Claro, Oscuro, Sistema).
Rectangle {
  id: menu

  property var ui: ({})        // panel palette (bg, fg, muted, accent, border, ...)
  property var options: []     // [{ value, label, iconUrl?, glyph? }]
  property string current: ""

  signal picked(string value)

  visible: false
  width: 180
  height: list.implicitHeight + 12
  radius: 10
  color: ui.bg || "transparent"
  border.color: ui.border || "transparent"
  border.width: 1

  function openWith(value) {
    menu.current = value || ""
    menu.visible = true
  }

  function close() { menu.visible = false }

  MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

  Column {
    id: list
    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 6 }

    Repeater {
      model: menu.options
      delegate: Rectangle {
        id: option
        required property var modelData
        readonly property bool isCurrent: modelData.value === menu.current
        width: list.width
        height: 34
        radius: 6
        color: optionArea.containsMouse ? Qt.alpha(menu.ui.accent, 0.18)
          : isCurrent ? Qt.alpha(menu.ui.accent, 0.08) : "transparent"

        Item {
          id: iconBox
          anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
          width: 20
          height: 20
          Image {
            anchors.fill: parent
            visible: !!option.modelData.iconUrl
            sourceSize: Qt.size(40, 40)
            source: option.modelData.iconUrl || ""
          }
          Text {
            anchors.centerIn: parent
            visible: !!option.modelData.glyph
            text: option.modelData.glyph || ""
            color: menu.ui.fg
            font.pixelSize: Style.font.title
          }
        }
        Text {
          anchors { left: iconBox.right; leftMargin: 10; verticalCenter: parent.verticalCenter }
          text: option.modelData.label
          color: option.modelData.value === "" ? menu.ui.muted : menu.ui.fg
          font.family: Style.font.family
          font.pixelSize: Style.font.body
          font.bold: option.isCurrent
        }
        MouseArea {
          id: optionArea
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: { menu.visible = false; menu.picked(option.modelData.value) }
        }
      }
    }
  }
}
