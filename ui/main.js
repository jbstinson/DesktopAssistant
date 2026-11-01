import { mountCharacter } from "./character.svg.js";
import { mountPanel } from "./panel.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const ROBOT = 140;
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

// The window grows around the robot, so ask Rust to resize first and place things after.
robot.addEventListener("mousedown", async () => {
  if (busy) return;
  busy = true;
  try {
    const layout = await invoke("set_panel", { open: !open });
    open = !open;
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
  } catch (e) {
    console.error("set_panel failed", e);
  } finally {
    busy = false;
  }
});

let gotEvent = false;

function apply(snap) {
  const body = document.body;
  body.dataset.state = snap.state;
  body.dataset.multi = String(snap.waiting > 1);
  body.dataset.warning = String(Boolean(snap.warning));
  parts.badgeCount.textContent = snap.waiting > 1 ? String(snap.waiting) : "";
  robot.title = snap.warning || "";
  panel.render(snap);
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
