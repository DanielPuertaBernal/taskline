import QtQuick
import qs.Commons
import "logic.js" as Logic

// Month calendar popover for the task panel: Monday first, Spanish names,
// today outlined, the chosen day filled, a dot on days that already have
// pending tasks, prev/next month arrows and quick chips. The text field that
// opened it keeps keyboard focus and forwards keys through handleKey(), so
// typing shortcuts (hoy, +3d, 2026-12-01) still works and moves the cursor.
Rectangle {
  id: picker

  // Pending tasks per day ({ "YYYY-MM-DD": count }), for the dots.
  property var dueDays: ({})
  property string selectedIso: ""
  property string cursorIso: ""
  property int year: 2026
  property int month: 0
  property string todayIso: ""
  property color dotColor: "#ff9e64"
  property var ui: ({})   // panel palette

  signal picked(string iso)   // "" means "no date"
  signal dismissed()

  readonly property int cell: 36

  visible: false
  width: grid.width + 24
  height: content.implicitHeight + 24
  radius: 12
  color: picker.ui.bg
  border.color: picker.ui.border
  border.width: 1

  function isoToDate(iso) {
    const m = String(iso || "").match(/^(\d{4})-(\d{2})-(\d{2})$/)
    return m ? new Date(+m[1], +m[2] - 1, +m[3]) : null
  }

  function showMonthOf(date) {
    picker.year = date.getFullYear()
    picker.month = date.getMonth()
  }

  // Open with `iso` selected (or nothing selected when empty).
  function openWith(iso) {
    const now = new Date()
    picker.todayIso = Logic.isoDay(now)
    picker.selectedIso = isoToDate(iso) ? iso : ""
    picker.cursorIso = picker.selectedIso || picker.todayIso
    showMonthOf(isoToDate(picker.cursorIso))
    picker.visible = true
  }

  function close() {
    if (!picker.visible) return
    picker.visible = false
    picker.dismissed()
  }

  // Follow what is being typed: "+3d" moves the cursor to that day.
  function followText(text) {
    const d = Logic.parseQuickDate(text, new Date())
    if (!d) return
    picker.cursorIso = Logic.isoDay(d)
    showMonthOf(d)
  }

  function moveCursor(days) {
    const d = Logic.addDays(isoToDate(picker.cursorIso) || new Date(), days)
    picker.cursorIso = Logic.isoDay(d)
    showMonthOf(d)
  }

  function pageMonth(delta) {
    const next = Logic.shiftMonth(picker.year, picker.month, delta)
    picker.year = next.year
    picker.month = next.month
    const cur = isoToDate(picker.cursorIso)
    if (cur) {
      const last = new Date(next.year, next.month + 1, 0).getDate()
      picker.cursorIso = Logic.isoDay(new Date(next.year, next.month, Math.min(cur.getDate(), last)))
    }
  }

  function choose(iso) {
    picker.selectedIso = iso
    picker.visible = false
    picker.picked(iso)
  }

  function chooseChip(days) {
    choose(days === null ? "" : Logic.isoDay(Logic.addDays(new Date(), days)))
  }

  // Keys forwarded by the text field. Returns true when the key was used.
  // Enter picks the cursor day, unless the field holds text the calendar
  // cannot read (e.g. "friday"): then the field submits it as typed.
  function handleKey(event, fieldText) {
    if (!picker.visible) return false
    switch (event.key) {
    case Qt.Key_Left: moveCursor(-1); return true
    case Qt.Key_Right: moveCursor(1); return true
    case Qt.Key_Up: moveCursor(-7); return true
    case Qt.Key_Down: moveCursor(7); return true
    case Qt.Key_PageUp: pageMonth(-1); return true
    case Qt.Key_PageDown: pageMonth(1); return true
    case Qt.Key_Escape: close(); return true
    case Qt.Key_Return:
    case Qt.Key_Enter:
      if (String(fieldText || "").trim() !== "" && !Logic.parseQuickDate(fieldText, new Date())) {
        close()
        return false
      }
      choose(picker.cursorIso)
      return true
    }
    return false
  }

  // Swallow clicks on the popover background so they do not reach the list.
  MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }

  Column {
    id: content
    anchors { left: parent.left; top: parent.top; margins: 12 }
    spacing: 8

    // Month header with arrows.
    Item {
      width: grid.width
      height: 28

      Text {
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        text: "‹"
        color: prevArea.containsMouse ? picker.ui.accent : picker.ui.fg
        font.family: Style.font.family
        font.pixelSize: Style.font.heading
        MouseArea {
          id: prevArea
          anchors.fill: parent
          anchors.margins: -8
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: picker.pageMonth(-1)
        }
      }
      Text {
        anchors.centerIn: parent
        text: Logic.monthTitle(picker.year, picker.month)
        color: picker.ui.fg
        font.family: Style.font.family
        font.pixelSize: Style.font.title
        font.bold: true
      }
      Text {
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        text: "›"
        color: nextArea.containsMouse ? picker.ui.accent : picker.ui.fg
        font.family: Style.font.family
        font.pixelSize: Style.font.heading
        MouseArea {
          id: nextArea
          anchors.fill: parent
          anchors.margins: -8
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: picker.pageMonth(1)
        }
      }
    }

    Row {
      Repeater {
        model: Logic.WEEKDAYS
        delegate: Text {
          required property string modelData
          width: picker.cell
          horizontalAlignment: Text.AlignHCenter
          text: modelData
          color: picker.ui.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }
    }

    Grid {
      id: grid
      columns: 7

      Repeater {
        model: Logic.monthGrid(picker.year, picker.month)
        delegate: Rectangle {
          id: day
          required property var modelData
          readonly property bool isSelected: modelData.iso === picker.selectedIso
          readonly property bool isToday: modelData.iso === picker.todayIso
          readonly property bool isCursor: modelData.iso === picker.cursorIso
          readonly property bool hasTasks: (picker.dueDays[modelData.iso] || 0) > 0

          width: picker.cell
          height: picker.cell - 4
          radius: 8
          color: isSelected ? picker.ui.accent
            : (isCursor || dayArea.containsMouse) ? Qt.alpha(picker.ui.accent, 0.22) : "transparent"
          border.width: isToday && !isSelected ? 1.5 : 0
          border.color: picker.ui.accent

          Text {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: -2
            text: day.modelData.day
            color: day.isSelected ? picker.ui.bg
              : day.modelData.inMonth ? picker.ui.fg : Qt.alpha(picker.ui.muted, 0.7)
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            font.bold: day.isToday || day.isSelected
          }
          Rectangle {
            visible: day.hasTasks
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 3 }
            width: 4
            height: 4
            radius: 2
            color: day.isSelected ? picker.ui.bg : picker.dotColor
          }
          MouseArea {
            id: dayArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: picker.choose(day.modelData.iso)
          }
        }
      }
    }

    Flow {
      width: grid.width
      spacing: 6

      Repeater {
        model: Logic.CHIPS
        delegate: Rectangle {
          required property var modelData
          width: chipText.implicitWidth + 18
          height: chipText.implicitHeight + 8
          radius: height / 2
          color: Qt.alpha(picker.ui.accent, chipArea.containsMouse ? 0.28 : 0.12)

          Text {
            id: chipText
            anchors.centerIn: parent
            text: parent.modelData.label
            color: parent.modelData.days === null ? picker.ui.muted : picker.ui.fg
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
          MouseArea {
            id: chipArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: picker.chooseChip(parent.modelData.days)
          }
        }
      }
    }
  }
}
