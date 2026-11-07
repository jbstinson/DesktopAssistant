// Run: node --test scripts/hats.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { HAT_DEFS, hatMarkup } from "../ui/hats.js";

const MONTHS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

test("no hat for null or anything outside 1-12", () => {
  for (const m of [null, undefined, 0, 13, -1, 1.5, "10", NaN]) {
    assert.equal(hatMarkup(m), "", String(m));
  }
});

test("every month has its own hat", () => {
  const all = MONTHS.map(hatMarkup);
  for (const [i, markup] of all.entries()) assert.ok(markup.length > 0, `month ${i + 1}`);
  assert.equal(new Set(all).size, 12);
});

test("hats define no ids, so they can be swapped freely", () => {
  for (const m of MONTHS) assert.ok(!/\bid=/.test(hatMarkup(m)), `month ${m}`);
});

test("shared shapes are hat- ids, and every reference resolves", () => {
  const defined = [...HAT_DEFS.matchAll(/\bid="([^"]+)"/g)].map((x) => x[1]);
  assert.ok(defined.length > 0);
  for (const id of defined) assert.ok(id.startsWith("hat-"), id);
  for (const m of MONTHS) {
    for (const [, ref] of hatMarkup(m).matchAll(/href="#([^"]+)"/g)) {
      assert.ok(defined.includes(ref), `month ${m} uses #${ref}`);
    }
  }
});
