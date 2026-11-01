import { mountCharacter } from "./character.svg.js";
import { mountPanel } from "./panel.js";
import { mountApproval } from "./approval.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const { getCurrentWindow } = window.__TAURI__.window;

const ROBOT = 140;
// A press held this long drags the window; a shorter one is a click.
const DRAG_MS = 150;
const PANEL_H = 332;

const robot = document.getElementById("robot");
const parts = mountCharacter(robot);
const warn = document.createElement("span");
warn.id = "warn";
warn.textContent = "!";
robot.append(warn);

const panelEl = document.getElementById("panel");
const panel = mountPanel(panelEl, {
  onDismiss: (id) => invoke("dismiss_session", { id }),
  onJump: (id) => invoke("jump_to", { id }),
});

let open = false;
let busy = false;
// Panel changes run one after another, so an automatic open and a click cannot interleave.
let chain = Promise.resolve();
// The request the panel last saw, and whether the panel opened itself for it.
let lastRequestId = null;
// The panel state last asked for, which `open` trails while a change is in flight.
let wantOpen = false;
let autoOpened = false;

const approval = mountApproval(document.getElementById("approval"), panelEl, {
  onAnswer: (id, choice) => invoke("answer_permission", { id, choice }),
  onRefresh: () => invoke("get_snapshot").then(apply),
  cursorInWindow: () => invoke("cursor_in_window"),
  // The cursor is checked only once the panel is open and has its size.
  prepare: () => chain,
});

// The window grows around the robot, so ask Rust to resize first and place things after.
async function applyPanel(want) {
  if (open === want) return;
  busy = true;
  try {
    const layout = await invoke("set_panel", { open: want });
    open = want;
    document.body.dataset.panel = open ? "open" : "closed";
    if (open) {
      robot.style.left = `${layout.robot_x}px`;
      robot.style.top = `${layout.robot_y}px`;
      // Robot at the top of the window: panel below it; otherwise panel above.
      panelEl.style.top = layout.robot_y > 0 ? "8px" : `${ROBOT + 8}px`;
      panelEl.style.height = `${PANEL_H}px`;
    } else {
      robot.style.left = robot.style.top = "";
    }
    panel.setOpen(open);
    approval.setOpen(open);
  } catch (e) {
    console.error("set_panel failed", e);
  } finally {
    busy = false;
  }
}

function setPanelOpen(want) {
  wantOpen = want;
  chain = chain.then(() => applyPanel(want));
  return chain;
}

function togglePanel() {
  if (busy) return;
  // Once the user chooses, the panel no longer closes itself.
  autoOpened = false;
  setPanelOpen(!wantOpen);
}

// A new request opens the panel without taking focus. A panel opened that way closes when
// the requests run out; one the user already had open stays and returns to the session list.
function followRequests(snap) {
  const id = snap.request ? snap.request.id : null;
  if (id !== null && id !== lastRequestId && !wantOpen) {
    autoOpened = true;
    setPanelOpen(true);
  } else if (id === null && lastRequestId !== null) {
    if (autoOpened) setPanelOpen(false);
    autoOpened = false;
  }
  lastRequestId = id;
}

let pressed = false;
let pressTimer = null;
robot.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  pressed = true;
  // Dragging is off while the panel is open: the window is the panel's size then.
  if (open) return;
  pressTimer = setTimeout(() => {
    pressTimer = null;
    // The system takes over the mouse during a drag, so no mouseup arrives.
    pressed = false;
    getCurrentWindow().startDragging().catch((err) => console.error("startDragging failed", err));
  }, DRAG_MS);
});
window.addEventListener("mouseup", (e) => {
  if (e.button !== 0 || !pressed) return;
  pressed = false;
  if (pressTimer !== null) clearTimeout(pressTimer);
  pressTimer = null;
  togglePanel();
});

// While hidden, a class stops every CSS animation.
listen("visibility", (e) => document.body.classList.toggle("paused", !e.payload));

let gotEvent = false;

function apply(snap) {
  const body = document.body;
  body.dataset.state = snap.state;
  body.dataset.multi = String(snap.waiting > 1);
  body.dataset.warning = String(Boolean(snap.warning));
  parts.badgeCount.textContent = snap.waiting > 1 ? String(snap.waiting) : "";
  robot.title = snap.warning || "";
  body.dataset.request = String(Boolean(snap.request));
  followRequests(snap);
  approval.render(snap);
  // A request takes over the view, so the session list waits until it ends.
  if (!snap.request) panel.render(snap);
}

// Listen first, then ask once for the current state so nothing emitted
// before the page was ready is missed.
listen("snapshot", (e) => {
  gotEvent = true;
  apply(e.payload);
});
invoke("get_snapshot").then((snap) => {
  if (!gotEvent) apply(snap);
});
