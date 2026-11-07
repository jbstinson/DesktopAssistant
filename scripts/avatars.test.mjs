// Run: node --test scripts/avatars.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { AVATARS, avatarFor } from "../ui/avatars/index.js";
import { MARKS } from "../ui/character.svg.js";

const REQUIRED = ["head", "hat", "keys", "book", "book-page", "bell", "eyes"];
const ids = (markup) => [...markup.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
const count = (text, part) => text.split(part).length - 1;

test("the avatars, in menu order", () => {
  assert.deepEqual(Object.keys(AVATARS), ["robot", "dog", "cat", "paperclip"]);
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
