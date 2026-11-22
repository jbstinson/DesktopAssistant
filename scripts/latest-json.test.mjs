import test from "node:test";
import assert from "node:assert/strict";
import { latestJson } from "./latest-json.mjs";

const base = {
  version: "0.2.0",
  notes: "See the release page.",
  pubDate: "2026-11-22T12:00:00.000Z",
  signature: "dW50cnVzdGVkIGNvbW1lbnQ=\n",
  url: "https://github.com/jbstinson/DesktopAssistant/releases/download/v0.2.0/DesktopAssistant_0.2.0_x64-setup.exe",
};

test("builds the windows entry", () => {
  assert.deepEqual(latestJson(base), {
    version: "0.2.0",
    notes: "See the release page.",
    pub_date: "2026-11-22T12:00:00.000Z",
    platforms: { "windows-x86_64": { signature: "dW50cnVzdGVkIGNvbW1lbnQ=", url: base.url } },
  });
});

test("strips a leading v from the version", () => {
  assert.equal(latestJson({ ...base, version: "v0.2.0" }).version, "0.2.0");
});

test("refuses missing fields", () => {
  for (const field of ["version", "signature", "url"]) {
    assert.throws(() => latestJson({ ...base, [field]: "" }), new RegExp(field));
    assert.throws(() => latestJson({ ...base, [field]: undefined }), new RegExp(field));
  }
});
