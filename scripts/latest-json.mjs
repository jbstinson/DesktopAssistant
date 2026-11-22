// Builds latest.json, the manifest the in-app updater reads from the newest published GitHub
// release. release.yml runs it after building and signing the installer:
//   node scripts/latest-json.mjs <version> <setup.exe.sig> <setup download url> <out file>
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** The manifest for one Windows x64 installer. Throws if any field is empty. */
export function latestJson({ version, notes, pubDate, signature, url }) {
  const fields = { version, notes, pubDate, signature, url };
  for (const [name, value] of Object.entries(fields)) {
    if (typeof value !== "string" || value.trim() === "") throw new Error(`latest.json: ${name} is empty`);
  }
  return {
    version: version.replace(/^v/, ""),
    notes,
    pub_date: pubDate,
    platforms: { "windows-x86_64": { signature: signature.trim(), url } },
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, sigFile, url, out] = process.argv.slice(2);
  if (!out) {
    console.error("usage: node scripts/latest-json.mjs <version> <sig file> <url> <out file>");
    process.exit(2);
  }
  const manifest = latestJson({
    version,
    notes: "See the release page.",
    pubDate: new Date().toISOString(),
    signature: readFileSync(sigFile, "utf8"),
    url,
  });
  writeFileSync(out, JSON.stringify(manifest, null, 2));
}
