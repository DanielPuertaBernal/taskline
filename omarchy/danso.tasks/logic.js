.pragma library
// Pure helpers for the task panel: Taskwarrior arguments for due dates and
// priorities, due states and labels, and sorting. Kept free of QML so
// tests/logic_test.js can run it under node.

var PRIORITIES = ["H", "M", "L"]
var MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"]
var DAYS = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"]

function words(text) {
  return String(text || "").trim().split(/\s+/).filter(function (w) { return w !== "" })
}

// Quick input -> a Taskwarrior date: hoy/mañana, +3d/3d/+2w and YYYY-MM-DD.
// Anything else (friday, eom, 2026-12-01T10:00...) goes to Taskwarrior as is.
function normalizeDue(value) {
  var v = String(value || "").trim()
  var lower = v.toLowerCase()
  if (lower === "hoy") return "today"
  if (lower === "mañana" || lower === "manana") return "tomorrow"
  var m = lower.match(/^\+?(\d{1,4})([dw])$/)
  if (m) return "today+" + (parseInt(m[1], 10) * (m[2] === "w" ? 7 : 1)) + "d"
  return v
}

function normalizePriority(value) {
  // Taskwarrior rejects anything but H, M, L or empty, so no need to filter.
  return String(value || "").trim().toUpperCase()
}

// Rewrite due:/priority:/pri: words so quick input works inline too.
function taskArgs(text) {
  return words(text).map(function (w) {
    var i = w.indexOf(":")
    if (i < 0) return w
    var key = w.slice(0, i), value = w.slice(i + 1)
    if (key === "due") return "due:" + (value ? normalizeDue(value) : "")
    if (key === "priority" || key === "pri") return "priority:" + normalizePriority(value)
    return w
  })
}

function hasKey(args, keys) {
  return args.some(function (a) {
    var i = a.indexOf(":")
    return i > 0 && keys.indexOf(a.slice(0, i)) >= 0
  })
}

// Arguments for `task add`: the text plus the separate due field and priority.
function addArgs(text, due, priority) {
  var args = taskArgs(text)
  if (args.length === 0) return []
  if (String(due || "").trim() !== "" && !hasKey(args, ["due"])) args.push("due:" + normalizeDue(due))
  if (priority && !hasKey(args, ["priority", "pri"])) args.push("priority:" + normalizePriority(priority))
  return args
}

function parseDate(value) {
  var m = String(value || "").match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/)
  if (!m) return null
  return new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]))
}

function isoDay(date) {
  function pad(n) { return (n < 10 ? "0" : "") + n }
  return date.getFullYear() + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate())
}

// Whole local days from `now` to `date` (negative when in the past).
function dayDiff(date, now) {
  var a = new Date(date.getFullYear(), date.getMonth(), date.getDate())
  var b = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  return Math.round((a - b) / 86400000)
}

// What the edit field shows; editArgs reads it back.
function editText(task) {
  var parts = [task.description]
  if (task.project) parts.push("project:" + task.project)
  var due = parseDate(task.due)
  if (due) parts.push("due:" + isoDay(due))
  if (PRIORITIES.indexOf(task.priority) >= 0) parts.push("priority:" + task.priority)
  return parts.join(" ")
}

// Arguments for `task <uuid> modify`. The edit field holds the due date and
// priority, so removing them from the text clears them.
function editArgs(task, text) {
  var args = taskArgs(text)
  if (args.length === 0) return []
  if (task.due && !hasKey(args, ["due"])) args.push("due:")
  if (task.priority && !hasKey(args, ["priority", "pri"])) args.push("priority:")
  return args
}

// "overdue", "today", "upcoming" or "" for pending tasks.
function dueState(task, now) {
  var due = parseDate(task.due)
  if (!due || task.status !== "pending") return ""
  var diff = dayDiff(due, now)
  return diff < 0 ? "overdue" : (diff === 0 ? "today" : "upcoming")
}

function dueLabel(task, now) {
  var due = parseDate(task.due)
  if (!due) return ""
  var diff = dayDiff(due, now)
  var short = due.getDate() + " " + MONTHS[due.getMonth()]
    + (due.getFullYear() !== now.getFullYear() ? " " + due.getFullYear() : "")
  if (diff < 0) return "vencida · " + short
  if (diff === 0) return "hoy"
  if (diff === 1) return "mañana"
  return (due.getFullYear() !== now.getFullYear() ? "" : DAYS[due.getDay()] + " ") + short
}

function priorityRank(task) {
  var i = PRIORITIES.indexOf(task.priority)
  return i < 0 ? PRIORITIES.length : i
}

function compareDue(a, b) {
  var da = parseDate(a.due), db = parseDate(b.due)
  if (!da && !db) return 0
  if (!da) return 1
  if (!db) return -1
  return da - db
}

// Pending first (sorted by mode), then completed (latest first).
// due: soonest first, no date last. priority: H > M > L > none, ties by due.
function sortTasks(list, mode) {
  var copy = list.slice()
  copy.sort(function (a, b) {
    if (a.status !== b.status) return a.status === "pending" ? -1 : 1
    if (a.status !== "pending") return String(b.end || "").localeCompare(String(a.end || ""))
    var c = 0
    if (mode === "priority") c = priorityRank(a) - priorityRank(b)
    if (c === 0) c = compareDue(a, b)
    if (c === 0) c = String(a.entry || "").localeCompare(String(b.entry || ""))
    return c
  })
  return copy
}

// --------------------------------------------------------------- calendar --
var MONTH_NAMES = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
                   "septiembre", "octubre", "noviembre", "diciembre"]
var WEEKDAYS = ["lu", "ma", "mi", "ju", "vi", "sá", "do"] // Monday first
var CHIPS = [
  { label: "Hoy", days: 0 },
  { label: "Mañana", days: 1 },
  { label: "+1 semana", days: 7 },
  { label: "Sin fecha", days: null }
]

function addDays(date, n) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + n)
}

function monthTitle(year, month) {
  return MONTH_NAMES[month] + " " + year
}

function shiftMonth(year, month, delta) {
  var d = new Date(year, month + delta, 1)
  return { year: d.getFullYear(), month: d.getMonth() }
}

// 42 cells (6 weeks), Monday first: { iso, day, inMonth }.
function monthGrid(year, month) {
  var first = new Date(year, month, 1)
  var start = addDays(first, -((first.getDay() + 6) % 7))
  var cells = []
  for (var i = 0; i < 42; i++) {
    var d = addDays(start, i)
    cells.push({ iso: isoDay(d), day: d.getDate(), inMonth: d.getMonth() === month })
  }
  return cells
}

// The local day quick input points at, or null when the calendar cannot tell
// (Taskwarrior still gets the text as typed, e.g. "friday").
function parseQuickDate(text, now) {
  var v = String(text || "").trim().toLowerCase()
  var today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  if (v === "hoy" || v === "today") return today
  if (v === "mañana" || v === "manana" || v === "tomorrow") return addDays(today, 1)
  var m = v.match(/^\+?(\d{1,4})([dw])$/)
  if (m) return addDays(today, parseInt(m[1], 10) * (m[2] === "w" ? 7 : 1))
  m = v.match(/^(\d{4})-(\d{2})-(\d{2})$/)
  if (m) {
    var d = new Date(+m[1], +m[2] - 1, +m[3])
    return d.getMonth() === +m[2] - 1 && d.getDate() === +m[3] ? d : null
  }
  return null
}

// { "YYYY-MM-DD": count } of pending tasks due that local day.
function dueDays(list) {
  var days = {}
  list.forEach(function (t) {
    var due = parseDate(t.due)
    if (!due || t.status !== "pending") return
    var key = isoDay(due)
    days[key] = (days[key] || 0) + 1
  })
  return days
}

// Put due:<iso> into edit text, replacing any due: word; "" removes it.
function setDueToken(text, iso) {
  var parts = words(text).filter(function (w) { return w.indexOf("due:") !== 0 })
  if (iso) {
    var at = parts.findIndex(function (w) { return w.indexOf("priority:") === 0 || w.indexOf("pri:") === 0 })
    parts.splice(at < 0 ? parts.length : at, 0, "due:" + iso)
  }
  return parts.join(" ")
}

// --------------------------------------------------------- priority icons --
// Lucide icons (ISC, see LICENSE-lucide.txt), identical to assets/icons/*.svg.
var SVG_OPEN = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" '
  + 'stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
var PRIORITY_ICONS = {
  H: SVG_OPEN + '<path d="m17 11-5-5-5 5"/><path d="m17 18-5-5-5 5"/></svg>',
  M: SVG_OPEN + '<line x1="5" x2="19" y1="9" y2="9"/><line x1="5" x2="19" y1="15" y2="15"/></svg>',
  L: SVG_OPEN + '<path d="m6 9 6 6 6-6"/></svg>'
}
var PRIORITY_NAMES = { H: "Alta", M: "Media", L: "Baja" }
var PRIORITY_OPTIONS = [
  { value: "H", label: "Alta" },
  { value: "M", label: "Media" },
  { value: "L", label: "Baja" },
  { value: "", label: "Sin prioridad" }
]

function priorityName(p) {
  return PRIORITY_NAMES[p] || ""
}

// A data: URL of the priority's icon drawn in `color`, or "" for none.
function priorityIconUrl(p, color) {
  var svg = PRIORITY_ICONS[p]
  if (!svg) return ""
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg.replace("currentColor", String(color)))
}

// Put priority:<p> into edit text, replacing priority:/pri:; "" removes it.
function setPriorityToken(text, p) {
  var parts = words(text).filter(function (w) { return w.indexOf("priority:") !== 0 && w.indexOf("pri:") !== 0 })
  if (p) parts.push("priority:" + p)
  return parts.join(" ")
}

// ------------------------------------------------------------------ themes --
// Tokyo Night (Oscuro) and Tokyo Night Day (Claro). Day text colors are
// darkened just enough for WCAG AA (4.5:1) on bg and surface; `muted` is the
// text-safe comment color and `track` the decorative one (pie, borders).
var THEMES = {
  dark: {
    bg: "#1a1b26", surface: "#24283b", border: "#414868", hover: "#292e42", fg: "#c0caf5",
    muted: "#8990b5", track: "#565f89", accent: "#7aa2f7", purple: "#bb9af7", green: "#9ece6a",
    orange: "#ff9e64", red: "#f7768e", yellow: "#e0af68", cyan: "#7dcfff"
  },
  light: {
    bg: "#e1e2e7", surface: "#d0d5e3", border: "#a8aecb", hover: "#c4c8da", fg: "#3257ad",
    muted: "#505985", track: "#a8aecb", accent: "#1357b5", purple: "#7519ec", green: "#4a6230",
    orange: "#8d4900", red: "#b6093b", yellow: "#705631", cyan: "#006283"
  }
}

function themePalette(mode) {
  return THEMES[mode === "light" ? "light" : "dark"]
}

// `theme=light|dark|system` from ~/.config/taskline/omarchy.conf (default system).
function themeFromConf(text) {
  var m = String(text || "").match(/^[ \t]*theme[ \t]*=[ \t]*(\w+)[ \t]*$/m)
  var v = m ? m[1].toLowerCase() : ""
  return v === "light" || v === "dark" ? v : "system"
}

// Whether an Omarchy theme background (0..1 channels) is light.
function isLight(r, g, b) {
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5
}

// Status colors (due states, priorities, calendar dots) for a light or dark UI.
function palette(light) {
  var t = themePalette(light ? "light" : "dark")
  return {
    due: { overdue: t.red, today: t.orange, upcoming: t.cyan },
    priority: { H: t.red, M: t.yellow, L: t.green },
    dot: t.orange
  }
}

function channel(hex, i) {
  var c = parseInt(String(hex).slice(1 + 2 * i, 3 + 2 * i), 16) / 255
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
}

function contrast(a, b) {
  function lum(h) { return 0.2126 * channel(h, 0) + 0.7152 * channel(h, 1) + 0.0722 * channel(h, 2) }
  var la = lum(a), lb = lum(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}

if (typeof module !== "undefined") {
  module.exports = {
    normalizeDue: normalizeDue, taskArgs: taskArgs, addArgs: addArgs, editText: editText, editArgs: editArgs,
    dueState: dueState, dueLabel: dueLabel, sortTasks: sortTasks, parseDate: parseDate,
    isoDay: isoDay, monthGrid: monthGrid, monthTitle: monthTitle, shiftMonth: shiftMonth,
    parseQuickDate: parseQuickDate, dueDays: dueDays, setDueToken: setDueToken, addDays: addDays,
    WEEKDAYS: WEEKDAYS, CHIPS: CHIPS, PRIORITY_ICONS: PRIORITY_ICONS, PRIORITY_OPTIONS: PRIORITY_OPTIONS,
    priorityIconUrl: priorityIconUrl, priorityName: priorityName, setPriorityToken: setPriorityToken,
    isLight: isLight, palette: palette, themePalette: themePalette, themeFromConf: themeFromConf,
    contrast: contrast
  }
}
