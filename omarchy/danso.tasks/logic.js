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

if (typeof module !== "undefined") {
  module.exports = {
    normalizeDue: normalizeDue, taskArgs: taskArgs, addArgs: addArgs, editText: editText, editArgs: editArgs,
    dueState: dueState, dueLabel: dueLabel, sortTasks: sortTasks, parseDate: parseDate
  }
}
