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

console.log(`\n${passed} passed, ${failed} failed`)
process.exit(failed ? 1 : 0)
