// Run: node --test scripts/avatars.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { AVATARS, avatarFor } from "../ui/avatars/index.js";
import { MARKS } from "../ui/character.svg.js";

const REQUIRED = ["head", "hat", "keys", "book", "book-page", "bell", "eyes"];
const ids = (markup) => [...markup.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
const count = (text, part) => text.split(part).length - 1;

test("the avatars, in menu order", () => {
  assert.deepEqual(Object.keys(AVATARS), ["robot", "dog", "cat", "paperclip", "skeleton", "skeleton3d"]);
});

test("anything unknown is the robot", () => {
  for (const id of ["nope", "", undefined, null, "Robot", "toString"]) {
    assert.equal(avatarFor(id), AVATARS.robot, String(id));
  }
  assert.equal(avatarFor("cat"), AVATARS.cat);
});

test("the shared marks", () => {
  for (const id of ["think", "think-1", "think-2", "think-3", "zzz", "alarm", "badge", "badge-count"]) {
    assert.ok(ids(MARKS).includes(id), id);
  }
});

for (const [name, avatar] of Object.entries(AVATARS)) {
  test(`${name} has every part the poses and hats need`, () => {
    for (const id of REQUIRED) assert.ok(ids(avatar.art).includes(id), `${name} lacks #${id}`);
    assert.ok(avatar.art.includes('<g id="hat"></g>'), `${name}: #hat must start empty`);
    assert.equal(typeof avatar.hatTransform, "string");
  });

  test(`${name} keeps the hat inside the head`, () => {
    const between = avatar.art.slice(avatar.art.indexOf('id="head"'), avatar.art.indexOf('id="hat"'));
    assert.ok(avatar.art.indexOf('id="head"') < avatar.art.indexOf('id="hat"'));
    assert.ok(count(between, "</g>") <= count(between, "<g"), `${name}: #head closes before #hat`);
  });

  test(`${name} and the marks share no id`, () => {
    const all = ids(avatar.art + MARKS);
    assert.equal(new Set(all).size, all.length, `${name}: ${all.filter((x, i) => all.indexOf(x) !== i)}`);
  });
}

test("the 3D skeleton has a loop for every state, and its other clips are transitions or fidgets", async () => {
  const { default: strips } = await import("../ui/avatars/skeleton3d/strips.js");
  const STATES = ["asleep", "idle", "typing", "reading", "thinking", "done", "needs-you", "error"];
  for (const state of STATES) assert.ok(Object.hasOwn(strips, state), state);
  for (const [clip, s] of Object.entries(strips)) {
    assert.ok(Number.isInteger(s.frames) && s.frames >= 2, `${clip} frames`);
    assert.ok(s.seconds > 0 && typeof s.loop === "boolean", clip);
    assert.ok(AVATARS.skeleton3d.art.includes(`id="s3-${clip}"`), clip);
    if (STATES.includes(clip)) continue;
    const intro = clip.endsWith("-in") && STATES.includes(clip.slice(0, -3));
    assert.ok(intro || clip.startsWith("fidget-"), `${clip} is neither a transition nor a fidget`);
    assert.equal(s.loop, false, `${clip} must play once so the player moves on`);
  }
});
