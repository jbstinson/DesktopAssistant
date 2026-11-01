// Run: node --test scripts/panel.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { elapsedLabel, sourceLabel, stateLabel, REFRESH_MS } from "../ui/panel.js";

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
