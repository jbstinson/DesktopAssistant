// Run: node --test scripts/skeleton3d-player.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { createPlayer, FIDGET_MIN_MS, FIDGET_SPREAD_MS } from "../ui/avatars/skeleton3d-player.js";

const CLIPS = ["idle", "typing", "typing-in", "done", "error", "fidget-look", "fidget-jaw"];

/** A player with fake timers; `shown` lists every clip put on screen. */
function rig(random = () => 0) {
  const shown = [];
  let pending = null;
  const timers = {
    setTimeout: (fn, ms) => (pending = { fn, ms }),
    clearTimeout: (t) => { if (t === pending) pending = null; },
  };
  const p = createPlayer(CLIPS, (name) => shown.push(name), timers, random);
  return { p, shown, fire: () => { const t = pending; pending = null; t.fn(); }, pending: () => pending };
}

test("a state with a transition plays it first, then its loop", () => {
  const { p, shown } = rig();
  p.setState("typing");
  assert.deepEqual(shown, ["typing-in"]);
  p.ended("typing-in");
  assert.deepEqual(shown, ["typing-in", "typing"]);
});

test("a state without a transition goes straight to its clip, and a one-off state holds", () => {
  const { p, shown } = rig();
  p.setState("done");
  p.ended("done");
  assert.deepEqual(shown, ["done"]);
});

test("the same state again changes nothing", () => {
  const { p, shown } = rig();
  p.setState("typing");
  p.setState("typing");
  assert.deepEqual(shown, ["typing-in"]);
});

test("a stale end from an earlier clip is ignored", () => {
  const { p, shown } = rig();
  p.setState("typing");
  p.setState("error");
  p.ended("typing-in");
  assert.deepEqual(shown, ["typing-in", "error"]);
});

test("idle fidgets after 8 to 20 seconds, then returns to idle", () => {
  const { p, shown, fire, pending } = rig(() => 0.5);
  p.setState("idle");
  assert.equal(pending().ms, FIDGET_MIN_MS + 0.5 * FIDGET_SPREAD_MS);
  fire();
  assert.deepEqual(shown, ["idle", "fidget-jaw"]);
  p.ended("fidget-jaw");
  assert.deepEqual(shown, ["idle", "fidget-jaw", "idle"]);
  assert.ok(pending(), "the next fidget is scheduled");
});

test("leaving idle cancels the fidget, and stop cancels it too", () => {
  const a = rig();
  a.p.setState("idle");
  a.p.setState("typing");
  assert.equal(a.pending(), null);
  const b = rig();
  b.p.setState("idle");
  b.p.stop();
  assert.equal(b.pending(), null);
});
