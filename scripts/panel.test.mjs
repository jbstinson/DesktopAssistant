// Run: node --test scripts/panel.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { elapsedLabel, jumpNotice, panelBox, sourceLabel, stateLabel, REFRESH_MS } from "../ui/panel.js";

test("elapsed shows minutes, or <1m under a minute", () => {
  assert.equal(elapsedLabel(100, 100), "<1m");
  assert.equal(elapsedLabel(100, 159), "<1m");
  assert.equal(elapsedLabel(100, 160), "1m");
  assert.equal(elapsedLabel(0, 180), "3m");
  assert.equal(elapsedLabel(0, 4500), "75m");
  assert.equal(elapsedLabel(200, 100), "<1m"); // clock skew never goes negative
});

test("sources map to friendly names", () => {
  assert.equal(sourceLabel("claude-desktop"), "Desktop");
  assert.equal(sourceLabel("cli"), "Terminal");
  assert.equal(sourceLabel("sdk-cli"), "Background");
  assert.equal(sourceLabel(""), "Unknown");
  assert.equal(sourceLabel("toString"), "Unknown");
});

test("states have labels and unknown ones pass through", () => {
  assert.equal(stateLabel("needs-you"), "Needs you");
  assert.equal(stateLabel("asleep"), "Idle");
  assert.equal(stateLabel("odd"), "odd");
});

test("refresh is at most every 30 seconds", () => {
  assert.ok(REFRESH_MS >= 30000);
});

test("a failed jump becomes a short notice", () => {
  assert.equal(jumpNotice("window not found"), "Could not jump: window not found");
  assert.equal(jumpNotice(new Error("boom")), "Could not jump: boom");
  assert.equal(jumpNotice(undefined), "Could not jump");
});

test("panel fills the room below the robot, up to its full height", () => {
  assert.deepEqual(panelBox(30, 510, true), { top: 178, height: 332 });
});

test("panel above the robot stops short of the hat room", () => {
  assert.deepEqual(panelBox(370, 510, false), { top: 8, height: 332 });
});

test("on a short screen the panel shrinks instead of covering the hat", () => {
  // Window clamped to the top of a short work area, robot 260 px down, panel above.
  assert.deepEqual(panelBox(260, 510, false), { top: 8, height: 222 });
  // Robot 150 px down with the panel below: it ends at the window's bottom.
  assert.deepEqual(panelBox(150, 510, true), { top: 298, height: 212 });
});
