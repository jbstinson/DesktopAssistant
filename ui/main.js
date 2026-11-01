import { mountCharacter } from "./character.svg.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const robot = document.getElementById("robot");
const parts = mountCharacter(robot);
robot.addEventListener("mousedown", () => invoke("clicked"));

let gotEvent = false;

function apply(snap) {
  const body = document.body;
  body.dataset.state = snap.state;
  body.dataset.multi = String(snap.waiting > 1);
  parts.badgeCount.textContent = snap.waiting > 1 ? String(snap.waiting) : "";
  robot.title = snap.warning || "";
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
