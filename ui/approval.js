// The approval panel. The pure helpers are exported for tests (scripts/approval.test.mjs); the
// DOM code only runs when mountApproval is called. Everything that came from a request goes in
// through textContent, because commands and paths are untrusted text.
//
// The window never has keyboard focus and a stray click must not approve anything, so buttons
// are locked for a moment after a request appears, and for longer when the cursor was already
// over the panel. Rust enforces the time part again; only this file can see the cursor part.

import { sourceLabel } from "./panel.js";

/** Buttons stay locked this long after a request becomes current (Rust uses the same). */
export const LOCK_MS = 1000;

const SOURCE_FALLBACK = "(unnamed)";

// Text that can display differently from what it is: bidi controls, zero-width and other
// invisible format characters, control characters, and blank Hangul fillers. Tabs and line
// breaks are ordinary layout and stay.
const INVISIBLE = /[\p{Cf}\p{Cc}\u2028\u2029\u115F\u1160\u3164\uFFA0]/gu;
const KEEP = new Set(["\n", "\r", "\t"]);

/** Shows invisible characters as [U+202E] so the panel text is what will run. */
export function revealInvisible(text) {
  return String(text).replace(INVISIBLE, (ch) =>
    KEEP.has(ch) ? ch : `[U+${ch.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}]`);
}

function pretty(input) {
  if (input === undefined) return "(no input)";
  try {
    return JSON.stringify(input, null, 2) ?? "(no input)";
  } catch {
    return "(unreadable input)";
  }
}

const isText = (v) => typeof v === "string";
const isFilled = (v) => typeof v === "string" && v.length > 0;

/** The tool name and every input as JSON: used when a request does not have the expected shape. */
function fallbackBlocks(req) {
  return [
    { type: "tool", text: isFilled(req.tool) ? req.tool : "(unknown tool)" },
    { type: "json", text: pretty(req.input) },
  ];
}

/** Every input key not already shown, so no input is ever hidden (replace_all, edit_mode...). */
function otherInputs(input, shown) {
  const rest = Object.fromEntries(Object.entries(input).filter(([k]) => !shown.includes(k)));
  return Object.keys(rest).length ? [{ type: "other", text: pretty(rest) }] : [];
}

function blocksFor(req) {
  const input = req.input !== null && typeof req.input === "object" && !Array.isArray(req.input) ? req.input : null;
  if (!input) return null;
  const tool = { type: "tool", text: req.tool };
  switch (req.kind) {
    case "command":
      if (!isFilled(input.command)) return null;
      return [tool, { type: "code", text: input.command }, ...otherInputs(input, ["command"])];
    case "edit":
      if (req.tool === "Write" && isFilled(input.file_path) && isText(input.content)) {
        return [tool, { type: "path", text: input.file_path }, { type: "added", text: input.content },
          ...otherInputs(input, ["file_path", "content"])];
      }
      if (req.tool === "NotebookEdit" && isFilled(input.notebook_path) && isText(input.new_source)) {
        // A delete has no new source to show as added; it stays in the other inputs.
        const deleting = input.edit_mode === "delete";
        return [tool, { type: "path", text: input.notebook_path },
          ...(deleting ? [] : [{ type: "added", text: input.new_source }]),
          ...otherInputs(input, deleting ? ["notebook_path"] : ["notebook_path", "new_source"])];
      }
      if (req.tool === "Edit" && isFilled(input.file_path) && isText(input.old_string) && isText(input.new_string)) {
        return [tool, { type: "path", text: input.file_path }, { type: "removed", text: input.old_string },
          { type: "added", text: input.new_string }, ...otherInputs(input, ["file_path", "old_string", "new_string"])];
      }
      return null;
    default:
      // Web, MCP and anything else: the tool name and all inputs.
      return null;
  }
}

/**
 * What the panel shows for a request: header text, the description, and the content blocks.
 * The tool name is always shown, and no input is dropped. A request that does not look as
 * expected shows its tool name and raw input; the content area is never blank. All text has
 * invisible characters made visible.
 */
export function requestView(req, count = 1) {
  const blocks = (blocksFor(req) ?? fallbackBlocks(req)).map((b) => ({ ...b, text: revealInvisible(b.text) }));
  return {
    project: revealInvisible(req.project || SOURCE_FALLBACK),
    source: sourceLabel(req.source),
    tag: revealInvisible(req.session_tag || ""),
    position: `1 of ${Math.max(1, count)}`,
    description: isFilled(req.description) ? revealInvisible(req.description) : null,
    blocks,
  };
}

const LABELS = {
  deny: "Deny",
  allow_once: "Allow once",
  allow_session: "Allow this session",
  always_allow: "Always allow",
  release: "Answer in Claude Code",
  confirm: "Confirm",
  cancel: "Cancel",
};

/**
 * The main row, left to right (Deny sits alone on the far left), and the plain link below it.
 * Session and always buttons exist only when the request has a rule to save.
 */
export function buttonsFor(req) {
  const choices = ["deny", "allow_once"];
  if (req.rule_text) choices.push("allow_session", "always_allow");
  return {
    buttons: choices.map((choice) => ({ choice, label: LABELS[choice] })),
    link: { choice: "release", label: LABELS.release },
  };
}

/** The rule Allow this session and Always allow would save, or null when there is none. */
export function ruleLine(req) {
  return req.rule_text ? revealInvisible(`Saves as a rule: ${req.rule_text}`) : null;
}

/**
 * The second step of Always allow. Confirm sits at the far left of its row and Cancel at the
 * far right, so Confirm never overlaps the rectangle Always allow had (in x), and a second
 * click of a double-click lands on nothing that approves.
 */
export function confirmView(req) {
  return {
    ruleText: revealInvisible(req.rule_text ?? ""),
    warning: req.rule_broad
      ? "This rule is broad. It will also allow other requests that start the same way."
      : null,
    buttons: [
      { choice: "always_allow", label: LABELS.confirm },
      { choice: "cancel", label: LABELS.cancel },
    ],
  };
}

// Lock state. `phase` says where the cursor stands:
//   unknown  the cursor check has not answered yet, so everything stays locked
//   inside   the cursor was over the panel when the request appeared; it must leave and return
//   left     it has left; the next entry clears the lock
//   clear    only the time lock applies

/** A fresh lock for a request that just became current, with `lockedMs` of time lock left. */
export function newLock(nowMs, lockedMs) {
  return { until: nowMs + lockedMs, phase: "unknown" };
}

/** Records where the cursor was when the request appeared. Unreadable (null) counts as over it. */
export function resolveCursor(lock, inside) {
  if (lock.phase !== "unknown") return lock;
  return { ...lock, phase: inside === false ? "clear" : "inside" };
}

/** The cursor entered (true) or left (false) the panel. An unreadable cursor (null) changes nothing. */
export function pointerMoved(lock, inside) {
  if (inside === null || inside === undefined) return lock;
  if (lock.phase === "inside" && !inside) return { ...lock, phase: "left" };
  if (lock.phase === "left" && inside) return { ...lock, phase: "clear" };
  return lock;
}

export function isLocked(lock, nowMs) {
  return lock.phase !== "clear" || nowMs < lock.until;
}

/** Only an answer that can approve is locked: handing back to Claude Code and Cancel never are. */
export function isChoiceLocked(lock, choice, nowMs) {
  if (choice === "release" || choice === "cancel") return false;
  return isLocked(lock, nowMs);
}

/**
 * A click counts only if its press began while the button was unlocked, it is for the request
 * the button was drawn for, and the lock is still off now.
 */
export function clickAllowed({ downLocked, lockedNow, pinnedId, currentId }) {
  return downLocked === false && !lockedNow && pinnedId === currentId;
}

/** Seconds left now, counting down from what the snapshot said when it arrived. */
export function remainingNow(secsAtReceipt, receivedAtMs, nowMs) {
  return Math.max(0, secsAtReceipt - Math.floor(Math.max(0, nowMs - receivedAtMs) / 1000));
}

export function countdownLabel(secs) {
  const s = Math.max(0, Math.floor(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function blockNode(block) {
  const text = block.text === "" ? "(empty)" : block.text;
  switch (block.type) {
    case "path":
      return el("div", "ap-path", text);
    case "tool":
      return el("div", "ap-tool", text);
    case "other": {
      const wrap = el("div", "ap-other");
      wrap.append(el("div", "ap-diff-label", "Other inputs"), el("pre", "ap-code", text));
      return wrap;
    }
    case "removed":
    case "added": {
      const wrap = el("div", `ap-diff ap-${block.type}`);
      wrap.append(el("div", "ap-diff-label", block.type === "removed" ? "Removed" : "Added"), el("pre", "ap-code", text));
      return wrap;
    }
    case "code":
    case "json":
      return el("pre", "ap-code", text);
    default:
      return el("div", "ap-text", text);
  }
}

/**
 * Mounts the approval view into `root`, a child of `panelEl`.
 *  - onAnswer(id, choice): sends the answer; may reject
 *  - onRefresh(): asks for a fresh snapshot after a refused answer
 *  - cursorInWindow(): resolves to [x, y] in CSS pixels from the window's top-left
 *  - prepare(): resolves once the panel is open and laid out, before the cursor is checked
 * Returns { render(snapshot), setOpen(open) }.
 */
export function mountApproval(root, panelEl, { onAnswer, onRefresh, cursorInWindow, prepare }) {
  const now = () => performance.now();
  let cur = null; // the request on show
  let open = false;
  let ticker = null;
  let unlockTimer = null;

  function stopTimers() {
    clearInterval(ticker);
    clearTimeout(unlockTimer);
    ticker = unlockTimer = null;
  }

  function refreshLock() {
    clearTimeout(unlockTimer);
    unlockTimer = null;
    if (!cur) return;
    const t = now();
    for (const b of root.querySelectorAll("button[data-choice]")) {
      b.disabled = cur.busy || isChoiceLocked(cur.lock, b.dataset.choice, t);
    }
    // When only the clock is left, wake up exactly when it runs out.
    if (cur.lock.phase === "clear" && t < cur.lock.until) {
      unlockTimer = setTimeout(refreshLock, cur.lock.until - t + 5);
    }
  }

  function tick() {
    if (!cur) return;
    const label = countdownLabel(remainingNow(cur.remaining, cur.receivedAt, now()));
    if (cur.timer.textContent !== label) cur.timer.textContent = label;
    refreshLock();
  }

  function startTicker() {
    clearInterval(ticker);
    ticker = null;
    if (cur && open) {
      tick();
      ticker = setInterval(tick, 1000);
    }
  }

  async function press(choice, pinnedId, downLocked) {
    if (!cur || cur.busy) return;
    const lockedNow = isChoiceLocked(cur.lock, choice, now());
    if (!clickAllowed({ downLocked, lockedNow, pinnedId, currentId: cur.id })) return;
    if (cur.step === "main" && choice === "always_allow") {
      cur.step = "confirm";
      // Confirm gets its own short time lock, so it cannot be hit by the same click run.
      cur.lock = resolveCursor(newLock(now(), LOCK_MS), false);
      drawActions();
      return;
    }
    if (choice === "cancel") {
      cur.step = "main";
      // Back on the main row, the buttons lock again for a moment.
      cur.lock = resolveCursor(newLock(now(), LOCK_MS), false);
      drawActions();
      return;
    }
    const id = cur.id;
    cur.busy = true;
    refreshLock();
    try {
      await onAnswer(id, choice);
    } catch {
      // Refused (locked, expired, already gone): show what is true now.
      if (cur && cur.id === id) cur.busy = false;
      refreshLock();
      onRefresh();
    }
  }

  function button(choice, label, className) {
    const b = el("button", className, label);
    b.type = "button";
    b.dataset.choice = choice;
    // Pinned to the request it was drawn for.
    const pinnedId = cur.id;
    b.dataset.requestId = pinnedId;
    // Whether the press began while locked. A disabled button gets no pointerdown at all, so a
    // press that began locked and ends after the unlock never has a recorded unlocked start.
    let downLocked;
    b.addEventListener("pointerdown", () => {
      downLocked = !cur || isChoiceLocked(cur.lock, choice, now());
    });
    b.addEventListener("click", () => {
      const was = downLocked;
      downLocked = undefined;
      press(choice, pinnedId, was);
    });
    return b;
  }

  /** Redraws only the rule line, buttons and link; the content above is left alone. */
  function drawActions() {
    const { req } = cur;
    const nodes = [];
    if (cur.step === "confirm") {
      const c = confirmView(req);
      const box = el("div", "ap-confirm");
      box.append(el("div", "ap-confirm-title", "Always allow this rule?"), el("div", "ap-rule-text", c.ruleText));
      if (c.warning) box.append(el("div", "ap-broad", c.warning));
      const row = el("div", "ap-row ap-row-confirm");
      row.append(...c.buttons.map((b) => button(b.choice, b.label, `ap-btn ap-${b.choice}`)));
      nodes.push(box, row);
    } else {
      const { buttons, link } = buttonsFor(req);
      const rule = ruleLine(req);
      if (rule) nodes.push(el("div", "ap-rule", rule));
      const row = el("div", "ap-row");
      row.append(...buttons.map((b) => button(b.choice, b.label, `ap-btn ap-${b.choice}`)));
      const linkBtn = button(link.choice, link.label, "ap-link");
      nodes.push(row, linkBtn);
    }
    cur.actions.replaceChildren(...nodes);
    refreshLock();
  }

  function build(snap) {
    const req = snap.request;
    const view = requestView(req, snap.request_count);
    const header = el("div", "ap-header");
    const timer = el("span", "ap-timer");
    header.append(
      el("span", "ap-project", view.project),
      el("span", "ap-source", view.source),
      ...(view.tag ? [el("span", "ap-tag", `#${view.tag}`)] : []),
      el("span", "ap-pos", view.position),
      timer,
    );
    const nodes = [header];
    if (view.description) nodes.push(el("div", "ap-desc", view.description));
    const content = el("div", "ap-content");
    content.append(...view.blocks.map(blockNode));
    const actions = el("div", "ap-actions");
    nodes.push(content, actions);
    root.replaceChildren(...nodes);
    return { header, timer, actions, pos: header.querySelector(".ap-pos") };
  }

  /** Whether the cursor is over the panel right now, or null if it cannot be told. */
  async function cursorOverPanel() {
    try {
      const [x, y] = await cursorInWindow();
      const r = panelEl.getBoundingClientRect();
      return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    } catch (e) {
      console.error("cursor check failed", e);
      return null;
    }
  }

  async function checkCursor(id) {
    await prepare();
    await new Promise((r) => requestAnimationFrame(() => r()));
    const inside = await cursorOverPanel(); // null counts as over the panel
    if (cur && cur.id === id) {
      cur.lock = resolveCursor(cur.lock, inside);
      refreshLock();
    }
  }

  function render(snap) {
    const req = snap?.request;
    if (!req) {
      cur = null;
      stopTimers();
      root.replaceChildren();
      return;
    }
    if (cur && cur.id === req.id) {
      // The same request: change numbers in place and leave every button node alone.
      cur.remaining = req.remaining_secs;
      cur.receivedAt = now();
      const pos = `1 of ${Math.max(1, snap.request_count)}`;
      if (cur.pos.textContent !== pos) cur.pos.textContent = pos;
      tick();
      return;
    }
    // A new request is current: start over, locked.
    stopTimers();
    cur = {
      id: req.id,
      req,
      step: "main",
      busy: false,
      lock: newLock(now(), req.locked_ms),
      remaining: req.remaining_secs,
      receivedAt: now(),
      ...build(snap),
    };
    drawActions();
    startTicker();
    checkCursor(req.id);
  }

  // Enter and leave events can fire for a cursor that never moved (the window resizes under
  // it), so each one is checked against where the cursor really is.
  async function pointerEvent() {
    if (!cur) return;
    const id = cur.id;
    // An unreadable cursor changes nothing, so the state stays locked.
    const inside = await cursorOverPanel();
    if (cur && cur.id === id) {
      cur.lock = pointerMoved(cur.lock, inside);
      refreshLock();
    }
  }
  panelEl.addEventListener("mouseenter", () => pointerEvent());
  panelEl.addEventListener("mouseleave", () => pointerEvent());

  function setOpen(next) {
    const reopened = next && !open;
    open = next;
    if (!open) {
      clearInterval(ticker);
      ticker = null;
      return;
    }
    // A panel that was closed and opens again under the cursor counts as a new appearance.
    if (reopened && cur) {
      cur.lock = newLock(now(), LOCK_MS);
      refreshLock();
      checkCursor(cur.id);
    }
    startTicker();
  }

  return { render, setOpen };
}
