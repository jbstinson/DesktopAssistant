// Run: node --test scripts/approval.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import {
  LOCK_MS, requestView, buttonsFor, ruleLine, confirmView, nextStep,
  newLock, resolveCursor, pointerMoved, isLocked, isChoiceLocked, countdownLabel, remainingNow,
} from "../ui/approval.js";

const base = { id: "r1", project: "proj", source: "cli", description: null, rule_text: null, rule_broad: false, locked_ms: 1000, remaining_secs: 120 };
const req = (over) => ({ ...base, ...over });

test("command shows the full command in one code block", () => {
  const long = "node -e \"" + "x".repeat(5000) + "\"";
  const v = requestView(req({ tool: "Bash", kind: "command", input: { command: long } }));
  assert.equal(v.blocks.length, 1);
  assert.deepEqual(v.blocks[0], { type: "code", text: long });
});

test("edit shows path then removed and added text", () => {
  const v = requestView(req({ tool: "Edit", kind: "edit", input: { file_path: "C:\\a.txt", old_string: "old", new_string: "new" } }));
  assert.deepEqual(v.blocks, [
    { type: "path", text: "C:\\a.txt" },
    { type: "removed", text: "old" },
    { type: "added", text: "new" },
  ]);
});

test("write shows path and the full content as added", () => {
  const v = requestView(req({ tool: "Write", kind: "edit", input: { file_path: "b.js", content: "line1\nline2" } }));
  assert.deepEqual(v.blocks, [{ type: "path", text: "b.js" }, { type: "added", text: "line1\nline2" }]);
});

test("notebook edit shows the notebook and the new source", () => {
  const v = requestView(req({ tool: "NotebookEdit", kind: "edit", input: { notebook_path: "n.ipynb", new_source: "print(1)" } }));
  assert.deepEqual(v.blocks, [{ type: "path", text: "n.ipynb" }, { type: "added", text: "print(1)" }]);
});

test("an edit with missing fields falls back to tool name and JSON, never blank", () => {
  for (const [tool, input] of [
    ["Edit", { file_path: "a" }], ["Edit", {}], ["Edit", null], ["Edit", "text"],
    ["Write", { file_path: "a" }], ["Write", { content: "c" }],
    ["NotebookEdit", { notebook_path: "n" }], ["Edit", { file_path: 1, old_string: "a", new_string: "b" }],
  ]) {
    const v = requestView(req({ tool, kind: "edit", input }));
    assert.equal(v.blocks[0].text, tool, JSON.stringify(input));
    assert.equal(v.blocks[1].type, "json");
    assert.ok(v.blocks[1].text.length > 0);
  }
  const v = requestView(req({ tool: "Edit", kind: "edit", input: { file_path: "a" } }));
  assert.equal(v.blocks[1].text, JSON.stringify({ file_path: "a" }, null, 2));
});

test("web shows the url or the query", () => {
  assert.deepEqual(requestView(req({ tool: "WebFetch", kind: "web", input: { url: "https://x.test/a", prompt: "p" } })).blocks,
    [{ type: "text", text: "https://x.test/a" }]);
  assert.deepEqual(requestView(req({ tool: "WebSearch", kind: "web", input: { query: "rust tauri" } })).blocks,
    [{ type: "text", text: "rust tauri" }]);
  const v = requestView(req({ tool: "WebFetch", kind: "web", input: {} }));
  assert.equal(v.blocks[0].text, "WebFetch");
});

test("mcp shows the tool name and all inputs as pretty JSON", () => {
  const input = { a: 1, nested: { b: [1, 2] } };
  const v = requestView(req({ tool: "mcp__srv__do", kind: "mcp", input }));
  assert.deepEqual(v.blocks, [{ type: "text", text: "mcp__srv__do" }, { type: "json", text: JSON.stringify(input, null, 2) }]);
});

test("unknown kind and missing input still show something", () => {
  const v = requestView(req({ tool: "Odd", kind: "other", input: undefined }));
  assert.equal(v.blocks[0].text, "Odd");
  assert.ok(v.blocks[1].text.length > 0);
  assert.ok(requestView(req({ tool: "Bash", kind: "command", input: { command: 5 } })).blocks[0].text.length > 0);
});

test("header carries project, source label and position", () => {
  const v = requestView(req({ tool: "Bash", kind: "command", input: { command: "ls" } }), 3);
  assert.equal(v.project, "proj");
  assert.equal(v.source, "Terminal");
  assert.equal(v.position, "1 of 3");
  assert.equal(requestView(req({ tool: "Bash", kind: "command", input: { command: "ls" } }), 1).position, "1 of 1");
});

test("buttons: deny far left, then allow once, session, always; link below", () => {
  const withRule = buttonsFor(req({ rule_text: "Bash: node *" }));
  assert.deepEqual(withRule.buttons.map((b) => b.choice), ["deny", "allow_once", "allow_session", "always_allow"]);
  assert.deepEqual(withRule.buttons.map((b) => b.label), ["Deny", "Allow once", "Allow this session", "Always allow"]);
  assert.equal(withRule.link.choice, "release");
  assert.equal(withRule.link.label, "Answer in Claude Code");
});

test("session and always buttons need a rule", () => {
  const none = buttonsFor(req({ rule_text: null }));
  assert.deepEqual(none.buttons.map((b) => b.choice), ["deny", "allow_once"]);
});

test("rule line appears only with a rule", () => {
  assert.equal(ruleLine(req({ rule_text: null })), null);
  assert.match(ruleLine(req({ rule_text: "Bash: node *" })), /Bash: node \*/);
});

test("always allow is two steps: confirm sits away from where always allow was", () => {
  assert.equal(nextStep("main", "always_allow"), "confirm");
  assert.equal(nextStep("confirm", "cancel"), "main");
  assert.equal(nextStep("main", "deny"), "main");
  assert.equal(nextStep("main", "allow_once"), "main");
  const r = req({ rule_text: "Bash: node *", rule_broad: true });
  const main = buttonsFor(r).buttons;
  const c = confirmView(r);
  assert.equal(c.ruleText, "Bash: node *");
  assert.ok(c.warning && c.warning.length > 0);
  assert.deepEqual(c.buttons.map((b) => b.choice), ["always_allow", "cancel"]);
  // A second click of a double-click lands on the last slot, which was Always allow's.
  assert.equal(main.at(-1).choice, "always_allow");
  assert.equal(c.buttons.at(-1).choice, "cancel");
  assert.equal(confirmView(req({ rule_text: "Bash: ls", rule_broad: false })).warning, null);
});

test("lock holds buttons for the time and until the cursor check resolves", () => {
  const l = newLock(0, LOCK_MS);
  assert.ok(isLocked(l, 5000), "locked until the cursor is known");
  const clear = resolveCursor(l, false);
  assert.ok(isLocked(clear, 0));
  assert.ok(isLocked(clear, LOCK_MS - 1));
  assert.ok(!isLocked(clear, LOCK_MS));
});

test("remaining lock time from the snapshot is honoured", () => {
  const l = resolveCursor(newLock(100, 400), false);
  assert.ok(isLocked(l, 499));
  assert.ok(!isLocked(l, 500));
  assert.ok(!isLocked(resolveCursor(newLock(100, 0), false), 100));
});

test("cursor already over the panel keeps buttons locked until it leaves and comes back", () => {
  let l = resolveCursor(newLock(0, LOCK_MS), true);
  assert.ok(isLocked(l, 60000), "time alone does not unlock");
  l = pointerMoved(l, true);
  assert.ok(isLocked(l, 60000), "still inside");
  l = pointerMoved(l, false);
  assert.ok(isLocked(l, 60000), "left but not back");
  l = pointerMoved(l, true);
  assert.ok(isLocked(l, LOCK_MS - 1), "back, but the time lock still runs");
  assert.ok(!isLocked(l, LOCK_MS));
});

test("pointer movement before the check resolves is ignored; outside start is not blocked", () => {
  let l = pointerMoved(newLock(0, LOCK_MS), true);
  l = resolveCursor(l, false);
  assert.ok(!isLocked(l, LOCK_MS));
  l = pointerMoved(pointerMoved(l, true), false);
  assert.ok(!isLocked(l, LOCK_MS), "later enters never lock");
});

test("the Answer in Claude Code link is exempt from the lock", () => {
  const l = newLock(0, LOCK_MS);
  assert.ok(isChoiceLocked(l, "deny", 0));
  assert.ok(isChoiceLocked(l, "allow_once", 0));
  assert.ok(isChoiceLocked(l, "always_allow", 0));
  assert.ok(!isChoiceLocked(l, "release", 0));
  assert.ok(!isChoiceLocked(l, "cancel", 0));
});

test("each new lock starts fresh", () => {
  const old = resolveCursor(newLock(0, LOCK_MS), false);
  assert.ok(!isLocked(old, 5000));
  assert.ok(isLocked(newLock(5000, LOCK_MS), 5000));
});

test("countdown counts down from the snapshot and never goes negative", () => {
  assert.equal(remainingNow(120, 1000, 1000), 120);
  assert.equal(remainingNow(120, 1000, 1999), 120);
  assert.equal(remainingNow(120, 1000, 2000), 119);
  assert.equal(remainingNow(2, 0, 9000), 0);
  assert.equal(countdownLabel(75), "1:15");
  assert.equal(countdownLabel(5), "0:05");
  assert.equal(countdownLabel(0), "0:00");
});
