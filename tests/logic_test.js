// Tests for omarchy/danso.tasks/logic.js. Run: TZ=America/Bogota node tests/logic_test.js
"use strict"
const fs = require("fs")
const path = require("path")
const vm = require("vm")
const assert = require("assert")

const source = fs.readFileSync(path.join(__dirname, "../omarchy/danso.tasks/logic.js"), "utf8")
  .replace(/^\.pragma library\s*$/m, "")
const sandbox = { module: { exports: {} } }
vm.runInNewContext(source, sandbox)
const L = sandbox.module.exports

let passed = 0
let failed = 0
function test(name, fn) {
  try { fn(); passed++; console.log("ok   " + name) } catch (e) { failed++; console.log("FAIL " + name + "\n     " + e.message) }
}
const eq = (a, b) => assert.deepStrictEqual(JSON.parse(JSON.stringify(a)), b)

// 2026-11-24 10:00 in Bogota (UTC-5)
const NOW = new Date(Date.UTC(2026, 10, 24, 15, 0, 0))
const day = (d) => "202611" + d + "T050000Z" // local midnight in Bogota

test("quick due inputs map to Taskwarrior dates", () => {
  eq(["hoy", "mañana", "+3d", "3d", "+2w", "2026-12-01", "friday", "today"].map(L.normalizeDue),
    ["today", "tomorrow", "today+3d", "today+3d", "today+14d", "2026-12-01", "friday", "today"])
})

test("add args combine text, due field and priority", () => {
  eq(L.addArgs("Pay rent project:Home", "+3d", "H"), ["Pay", "rent", "project:Home", "due:today+3d", "priority:H"])
  eq(L.addArgs("Pay rent due:mañana pri:m", "", ""), ["Pay", "rent", "due:tomorrow", "priority:M"])
  eq(L.addArgs("x due:hoy", "+9d", ""), ["x", "due:today"]) // inline wins
  eq(L.addArgs("   ", "+3d", "H"), [])
})

test("edit text round-trips through edit args", () => {
  const task = { description: "Plan trip", project: "Fun", due: day(27), priority: "L" }
  eq(L.editText(task), "Plan trip project:Fun due:2026-11-27 priority:L")
  eq(L.editArgs(task, "Plan the trip due:+1d priority:H"), ["Plan", "the", "trip", "due:today+1d", "priority:H"])
  eq(L.editArgs(task, "Plan the trip"), ["Plan", "the", "trip", "due:", "priority:"]) // removed -> cleared
  eq(L.editArgs({ description: "x" }, "y"), ["y"])
})

test("due state and Spanish labels", () => {
  const t = (due, status) => ({ due, status: status || "pending" })
  eq([day(23), day(24), day(25), day(27)].map(d => L.dueState(t(d), NOW)), ["overdue", "today", "upcoming", "upcoming"])
  eq(L.dueState(t(day(23), "completed"), NOW), "")
  eq(L.dueState({ status: "pending" }, NOW), "")
  eq([day(23), day(24), day(25), day(27)].map(d => L.dueLabel(t(d), NOW)), ["vencida · 23 nov", "hoy", "mañana", "vie 27 nov"])
  eq(L.dueLabel(t("20270105T050000Z"), NOW), "5 ene 2027")
})

const tasks = [
  { description: "no date", status: "pending", entry: "1" },
  { description: "next week", status: "pending", due: "20261201T050000Z", entry: "2" },
  { description: "overdue", status: "pending", due: day(20), entry: "3" },
  { description: "tomorrow H", status: "pending", due: day(25), priority: "H", entry: "4" },
  { description: "high no date", status: "pending", priority: "H", entry: "5" },
  { description: "medium", status: "pending", priority: "M", entry: "6" },
  { description: "done", status: "completed", end: day(24), due: day(1), entry: "0" },
]

test("sort by due: soonest first, no date last, completed at the end", () => {
  eq(L.sortTasks(tasks, "due").map(t => t.description),
    ["overdue", "tomorrow H", "next week", "no date", "high no date", "medium", "done"])
})

test("sort by priority: H > M > L > none, ties by due", () => {
  eq(L.sortTasks(tasks, "priority").map(t => t.description),
    ["tomorrow H", "high no date", "medium", "overdue", "next week", "no date", "done"])
})

test("sort does not mutate its input", () => {
  const before = tasks.map(t => t.description)
  L.sortTasks(tasks, "priority")
  eq(tasks.map(t => t.description), before)
})

// ------------------------------------------------------------- calendar --
test("month grid is Monday-first, 6 weeks, marks days outside the month", () => {
  const grid = L.monthGrid(2026, 10) // November 2026 starts on a Sunday
  eq(grid.length, 42)
  eq(grid[0].iso, "2026-10-26") // Monday before Nov 1
  eq(grid[6].iso, "2026-11-01")
  eq(grid[6].inMonth, true)
  eq(grid[0].inMonth, false)
  eq(grid[41].iso, "2026-12-06")
  eq(L.monthGrid(2027, 1)[0].iso, "2027-02-01") // Feb 2027 starts on a Monday
})

test("Spanish calendar names", () => {
  eq(L.monthTitle(2026, 10), "noviembre 2026")
  eq(L.WEEKDAYS.join(" "), "lu ma mi ju vi sá do")
})

test("shiftMonth wraps years", () => {
  eq(L.shiftMonth(2026, 11, 1), { year: 2027, month: 0 })
  eq(L.shiftMonth(2026, 0, -1), { year: 2025, month: 11 })
})

test("quick input parses to the day the calendar highlights", () => {
  const iso = (t) => { const d = L.parseQuickDate(t, NOW); return d ? L.isoDay(d) : null }
  eq(["hoy", "today", "mañana", "+3d", "3d", "+1w", "2026-12-01", "friday", "", "2026-13-40"].map(iso),
    ["2026-11-24", "2026-11-24", "2026-11-25", "2026-11-27", "2026-11-27", "2026-12-01", "2026-12-01", null, null, null])
})

test("chips", () => {
  eq(L.CHIPS.map(c => c.label), ["Hoy", "Mañana", "+1 semana", "Sin fecha"])
  eq(L.CHIPS.map(c => c.days), [0, 1, 7, null])
})

test("days with pending tasks get a dot", () => {
  eq(JSON.parse(JSON.stringify(L.dueDays(tasks))), { "2026-11-20": 1, "2026-11-25": 1, "2026-12-01": 1 })
})

test("setDueToken updates the edit text", () => {
  eq(L.setDueToken("Plan trip project:Fun due:2026-11-27 priority:L", "2026-12-03"),
    "Plan trip project:Fun due:2026-12-03 priority:L")
  eq(L.setDueToken("Plan trip", "2026-12-03"), "Plan trip due:2026-12-03")
  eq(L.setDueToken("Plan trip due:2026-11-27 priority:L", ""), "Plan trip priority:L")
})

// ------------------------------------------------------- priority icons --
test("embedded icons are the bundled Lucide SVGs", () => {
  for (const [p, name] of [["H", "high"], ["M", "medium"], ["L", "low"]]) {
    const file = fs.readFileSync(path.join(__dirname, "../assets/icons/priority-" + name + ".svg"), "utf8").trim()
    eq(L.PRIORITY_ICONS[p], file)
  }
})

test("icon urls are colored data urls; no priority means no icon", () => {
  const url = L.priorityIconUrl("H", "#f7768e")
  assert.ok(url.startsWith("data:image/svg+xml;utf8,"))
  assert.ok(decodeURIComponent(url.split(",")[1]).includes('stroke="#f7768e"'))
  eq(L.priorityIconUrl("", "#fff"), "")
  eq(L.priorityIconUrl(undefined, "#fff"), "")
})

test("priority picker options and names", () => {
  eq(L.PRIORITY_OPTIONS.map(o => o.value + ":" + o.label), ["H:Alta", "M:Media", "L:Baja", ":Sin prioridad"])
  eq(L.priorityName("M"), "Media")
  eq(L.priorityName(""), "")
})

test("setPriorityToken updates the edit text", () => {
  eq(L.setPriorityToken("Plan trip due:2026-11-27 priority:L", "H"), "Plan trip due:2026-11-27 priority:H")
  eq(L.setPriorityToken("Plan trip pri:M", ""), "Plan trip")
  eq(L.setPriorityToken("Plan trip", "L"), "Plan trip priority:L")
})

test("Tokyo Night palette for dark and Tokyo Night Day for light themes", () => {
  eq(L.isLight(0.1, 0.1, 0.15), false)
  eq(L.isLight(0.88, 0.89, 0.91), true)
  eq(L.palette(false).priority, { H: "#f7768e", M: "#e0af68", L: "#9ece6a" })
  eq(L.palette(true).priority, { H: "#b6093b", M: "#705631", L: "#4a6230" })
  eq(L.palette(true).due, { overdue: "#b6093b", today: "#8d4900", upcoming: "#006283" })
  eq(L.themePalette("light").bg, "#e1e2e7")
  eq(L.themePalette("dark").bg, "#1a1b26")
})

test("every text color meets WCAG AA (4.5:1) on the background and surface", () => {
  for (const mode of ["light", "dark"]) {
    const t = L.themePalette(mode)
    for (const key of ["fg", "muted", "accent", "red", "orange", "yellow", "green", "cyan", "purple"]) {
      for (const back of ["bg", "surface"]) {
        const ratio = L.contrast(t[key], t[back])
        assert.ok(ratio >= 4.5, `${mode} ${key} on ${back}: ${ratio.toFixed(2)}`)
      }
    }
  }
})

test("theme setting is read from omarchy.conf", () => {
  eq(L.themeFromConf("auto_update=false\ntheme = light\n"), "light")
  eq(L.themeFromConf("theme=dark"), "dark")
  eq(L.themeFromConf("theme=neon"), "system")
  eq(L.themeFromConf(""), "system")
})

console.log(`\n${passed} passed, ${failed} failed`)
process.exit(failed ? 1 : 0)
