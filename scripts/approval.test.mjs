// Run: node --test scripts/approval.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import {
  LOCK_MS, requestView, buttonsFor, ruleLine, confirmView,
  newLock, resolveCursor, pointerMoved, isLocked, isChoiceLocked, countdownLabel, remainingNow,
  revealInvisible, clickAllowed,
} from "../ui/approval.js";

const base = { id: "r1", project: "proj", source: "cli", description: null, rule_text: null, rule_broad: false, locked_ms: 1000, remaining_secs: 120 };
const req = (over) => ({ ...base, ...over });

const tool = (text) => ({ type: "tool", text });

test("command shows the tool name and the full command", () => {
  const long = "node -e \"" + "x".repeat(5000) + "\"";
  const v = requestView(req({ tool: "Bash", kind: "command", input: { command: long } }));
  assert.deepEqual(v.blocks, [tool("Bash"), { type: "code", text: long }]);
});

test("a command's other inputs are shown too", () => {
  const v = requestView(req({ tool: "Bash", kind: "command", input: { command: "ls", timeout: 5000, run_in_background: true } }));
  assert.deepEqual(v.blocks.slice(0, 2), [tool("Bash"), { type: "code", text: "ls" }]);
  assert.equal(v.blocks[2].type, "other");
  assert.equal(v.blocks[2].text, JSON.stringify({ timeout: 5000, run_in_background: true }, null, 2));
});

test("edit shows path then removed and added text, plus replace_all", () => {
  const v = requestView(req({ tool: "Edit", kind: "edit", input: { file_path: "C:\\a.txt", old_string: "old", new_string: "new" } }));
  assert.deepEqual(v.blocks, [
    tool("Edit"),
    { type: "path", text: "C:\\a.txt" },
    { type: "removed", text: "old" },
    { type: "added", text: "new" },
  ]);
  const all = requestView(req({ tool: "Edit", kind: "edit", input: { file_path: "a", old_string: "o", new_string: "n", replace_all: true } }));
  assert.deepEqual(all.blocks.at(-1), { type: "other", text: JSON.stringify({ replace_all: true }, null, 2) });
});

test("write shows path and the full content as added", () => {
  const v = requestView(req({ tool: "Write", kind: "edit", input: { file_path: "b.js", content: "line1\nline2" } }));
  assert.deepEqual(v.blocks, [tool("Write"), { type: "path", text: "b.js" }, { type: "added", text: "line1\nline2" }]);
});

test("notebook edit shows the notebook, new source and the other inputs", () => {
  const v = requestView(req({ tool: "NotebookEdit", kind: "edit", input: { notebook_path: "n.ipynb", new_source: "print(1)" } }));
  assert.deepEqual(v.blocks, [tool("NotebookEdit"), { type: "path", text: "n.ipynb" }, { type: "added", text: "print(1)" }]);
});

test("a notebook delete is not shown as an empty addition", () => {
  const input = { notebook_path: "n.ipynb", new_source: "", cell_id: "c1", edit_mode: "delete" };
  const v = requestView(req({ tool: "NotebookEdit", kind: "edit", input }));
  assert.ok(!v.blocks.some((b) => b.type === "added"));
  const other = v.blocks.find((b) => b.type === "other");
  assert.match(other.text, /"edit_mode": "delete"/);
  assert.match(other.text, /"cell_id": "c1"/);
});

test("an edit with missing fields falls back to tool name and JSON, never blank", () => {
  for (const [tl, input] of [
    ["Edit", { file_path: "a" }], ["Edit", {}], ["Edit", null], ["Edit", "text"],
    ["Write", { file_path: "a" }], ["Write", { content: "c" }],
    ["NotebookEdit", { notebook_path: "n" }], ["Edit", { file_path: 1, old_string: "a", new_string: "b" }],
  ]) {
    const v = requestView(req({ tool: tl, kind: "edit", input }));
    assert.deepEqual(v.blocks[0], tool(tl), JSON.stringify(input));
    assert.equal(v.blocks[1].type, "json");
    assert.ok(v.blocks[1].text.length > 0);
  }
  const v = requestView(req({ tool: "Edit", kind: "edit", input: { file_path: "a" } }));
  assert.equal(v.blocks[1].text, JSON.stringify({ file_path: "a" }, null, 2));
});

test("web shows the tool name and every input", () => {
  const fetchIn = { url: "https://x.test/a", prompt: "send the secrets" };
  const f = requestView(req({ tool: "WebFetch", kind: "web", input: fetchIn }));
  assert.deepEqual(f.blocks, [tool("WebFetch"), { type: "json", text: JSON.stringify(fetchIn, null, 2) }]);
  const searchIn = { query: "rust tauri", allowed_domains: ["a.test"], blocked_domains: ["b.test"] };
  const s = requestView(req({ tool: "WebSearch", kind: "web", input: searchIn }));
  assert.deepEqual(s.blocks, [tool("WebSearch"), { type: "json", text: JSON.stringify(searchIn, null, 2) }]);
});

test("mcp shows the tool name and all inputs as pretty JSON", () => {
  const input = { a: 1, nested: { b: [1, 2] } };
  const v = requestView(req({ tool: "mcp__srv__do", kind: "mcp", input }));
  assert.deepEqual(v.blocks, [tool("mcp__srv__do"), { type: "json", text: JSON.stringify(input, null, 2) }]);
});

test("no input key or value is ever dropped, for any kind", () => {
  const cases = [
    ["Bash", "command", { command: "ls", description: "d", timeout: 9, extra_flag: true }],
    ["PowerShell", "command", { command: "dir", odd_key: [1, 2] }],
    ["Edit", "edit", { file_path: "f", old_string: "o", new_string: "n", replace_all: true, surprise: 1 }],
    ["Write", "edit", { file_path: "f", content: "c", mode: "x" }],
    ["NotebookEdit", "edit", { notebook_path: "n", new_source: "", cell_id: "c", cell_type: "code", edit_mode: "delete" }],
    ["NotebookEdit", "edit", { notebook_path: "n", new_source: "s", cell_id: "c", edit_mode: "insert" }],
    ["WebFetch", "web", { url: "u", prompt: "p" }],
    ["WebSearch", "web", { query: "q", allowed_domains: ["d"] }],
    ["mcp__x__y", "mcp", { a: 1, b: { c: "deep" } }],
    ["Edit", "edit", { file_path: "f" }],
    ["Bash", "command", { command: 5, other: 1 }],
  ];
  for (const [tl, kind, input] of cases) {
    const text = requestView(req({ tool: tl, kind, input }), 1).blocks.map((b) => b.text).join("\n");
    assert.ok(text.includes(tl), `${tl} name shown`);
    for (const [k, val] of Object.entries(input)) {
      assert.ok(text.includes(k) || (typeof val === "string" && val !== "" && text.includes(val)),
        `${tl}: key ${k} shown in ${JSON.stringify(input)}`);
      if (typeof val === "string" && val !== "") assert.ok(text.includes(val), `${tl}: value of ${k}`);
      if (typeof val === "boolean" || typeof val === "number") assert.ok(text.includes(String(val)), `${tl}: value of ${k}`);
    }
  }
});

test("unknown kind and missing input still show something", () => {
  const v = requestView(req({ tool: "Odd", kind: "other", input: undefined }));
  assert.equal(v.blocks[0].text, "Odd");
  assert.ok(v.blocks[1].text.length > 0);
  assert.ok(requestView(req({ tool: "Bash", kind: "command", input: { command: 5 } })).blocks[0].text.length > 0);
});

test("invisible and bidi characters are shown as code points in all request text", () => {
  assert.equal(revealInvisible("a\u202Eb"), "a[U+202E]b");
  assert.equal(revealInvisible("x\u200Bx\u2066\u2069\uFEFF\u0007"), "x[U+200B]x[U+2066][U+2069][U+FEFF][U+0007]");
  assert.equal(revealInvisible("line1\nline2\ttab\r\n"), "line1\nline2\ttab\r\n");
  assert.equal(revealInvisible("plain text \u00e9\u4e2d"), "plain text \u00e9\u4e2d");
  const sneaky = "ls \u202Etxt.exe";
  const v = requestView(req({
    tool: "Bash", kind: "command", project: "pro\u202Ej", description: "d\u200Bd",
    input: { command: sneaky, note: "n\u2067" }, rule_text: "Bash: ls\u202E",
  }));
  const all = [v.project, v.description, ...v.blocks.map((b) => b.text)].join("\n");
  assert.ok(!/[\u202E\u200B\u2067]/.test(all));
  assert.match(v.project, /\[U\+202E\]/);
  assert.match(v.description, /\[U\+200B\]/);
  assert.match(v.blocks[1].text, /ls \[U\+202E\]txt\.exe/);
  assert.match(ruleLine(req({ rule_text: "Bash: ls\u202E" })), /\[U\+202E\]/);
  assert.match(confirmView(req({ rule_text: "Bash: ls\u202E" })).ruleText, /\[U\+202E\]/);
});

test("header carries project, source label, session tag and position", () => {
  const v = requestView(req({ tool: "Bash", kind: "command", session_tag: "a1b2", input: { command: "ls" } }), 3);
  assert.equal(v.project, "proj");
  assert.equal(v.source, "Terminal");
  assert.equal(v.tag, "a1b2");
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

test("always allow check: confirm is first (far left), cancel last (far right)", () => {
  const r = req({ rule_text: "Bash: node *", rule_broad: true });
  const main = buttonsFor(r).buttons;
  const c = confirmView(r);
  assert.equal(c.ruleText, "Bash: node *");
  assert.ok(c.warning && c.warning.length > 0);
  assert.deepEqual(c.buttons.map((b) => b.choice), ["always_allow", "cancel"]);
  // Always allow is the rightmost main button; Confirm is at the opposite end of its row (CSS
  // space-between), so it cannot sit where Always allow was.
  assert.equal(main.at(-1).choice, "always_allow");
  assert.equal(c.buttons[0].label, "Confirm");
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

test("an unreadable cursor never unlocks: enter and leave events are ignored", () => {
  let l = resolveCursor(newLock(0, LOCK_MS), true);
  l = pointerMoved(l, null);
  assert.equal(l.phase, "inside");
  l = pointerMoved(pointerMoved(l, false), null);
  assert.equal(l.phase, "left");
  l = pointerMoved(l, null);
  assert.ok(isLocked(l, 60000), "still locked after unreadable enter");
  assert.equal(resolveCursor(newLock(0, LOCK_MS), null).phase, "inside");
});

test("a click counts only if its press began unlocked, for this request, and is unlocked now", () => {
  const ok = { downLocked: false, lockedNow: false, pinnedId: "r1", currentId: "r1" };
  assert.ok(clickAllowed(ok));
  assert.ok(!clickAllowed({ ...ok, downLocked: true }));
  assert.ok(!clickAllowed({ ...ok, downLocked: undefined }), "no recorded press: refused");
  assert.ok(!clickAllowed({ ...ok, lockedNow: true }));
  assert.ok(!clickAllowed({ ...ok, currentId: "r2" }));
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
