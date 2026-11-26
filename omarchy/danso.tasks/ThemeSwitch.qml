import QtQuick
import qs.Commons

// Theme control for the panel's bottom-left column, styled like the sort
// button: "Tema" + a pill switch (sun left = Claro, moon right = Oscuro) with
// a sliding knob, and a small "Auto" chip that follows the Omarchy theme.
// Space or Enter flips the switch when it has keyboard focus.
Row {
  id: control

  property var ui: ({})
  property string mode: "system"   // light, dark or system
  property bool effectiveLight: false

  signal chosen(string mode)

  readonly property bool isLight: mode === "system" ? effectiveLight : mode === "light"

  spacing: 8

  function flip() { control.chosen(control.isLight ? "dark" : "light") }

  Rectangle {
    id: box
    width: content.implicitWidth + 24
    height: content.implicitHeight + 10
    radius: 6
    color: Qt.alpha(control.ui.accent, boxArea.containsMouse ? 0.2 : 0.08)
    border.width: switchFocus.activeFocus ? 2 : 0
    border.color: control.ui.accent

    Row {
      id: content
      anchors.centerIn: parent
      spacing: 10

      Text {
        anchors.verticalCenter: parent.verticalCenter
        text: "Tema"
        color: control.ui.fg
        font.family: Style.font.family
        font.pixelSize: Style.font.title
      }

      // Pill switch with a sliding knob.
      Rectangle {
        id: pill
        anchors.verticalCenter: parent.verticalCenter
        width: 54
        height: 26
        radius: height / 2
        color: Qt.alpha(control.ui.accent, 0.18)
        border.width: 1
        border.color: Qt.alpha(control.ui.accent, 0.45)

        Text {
          anchors { left: parent.left; leftMargin: 7; verticalCenter: parent.verticalCenter }
          text: "\u{F0599}"   // sun
          color: control.isLight ? control.ui.bg : control.ui.muted
          font.pixelSize: Style.font.caption + 2
          z: 2
        }
        Text {
          anchors { right: parent.right; rightMargin: 7; verticalCenter: parent.verticalCenter }
          text: "\u{F0594}"   // moon
          color: control.isLight ? control.ui.muted : control.ui.bg
          font.pixelSize: Style.font.caption + 2
          z: 2
        }
        Rectangle {
          id: knob
          width: 22
          height: 22
          radius: 11
          y: 2
          x: control.isLight ? 2 : pill.width - width - 2
          color: control.ui.accent
          Behavior on x { NumberAnimation { duration: Style.duration(160); easing.type: Easing.OutCubic } }
        }
      }
    }

    // Keyboard focus target: Tab reaches it, Space/Enter flip it.
    Item {
      id: switchFocus
      anchors.fill: parent
      activeFocusOnTab: true
      Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
          control.flip()
          event.accepted = true
        }
      }
    }

    MouseArea {
      id: boxArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: control.flip()
    }
  }

  // "Auto": follow the Omarchy theme (Sistema).
  Rectangle {
    anchors.verticalCenter: parent.verticalCenter
    width: autoText.implicitWidth + 16
    height: autoText.implicitHeight + 8
    radius: height / 2
    color: control.mode === "system" ? Qt.alpha(control.ui.accent, 0.28)
      : Qt.alpha(control.ui.accent, autoArea.containsMouse ? 0.16 : 0.06)
    border.width: control.mode === "system" ? 1 : 0
    border.color: control.ui.accent

    Text {
      id: autoText
      anchors.centerIn: parent
      text: "Auto"
      color: control.mode === "system" ? control.ui.fg : control.ui.muted
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    MouseArea {
      id: autoArea
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: control.chosen("system")
    }
  }
}
