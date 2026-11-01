// Generates the alert sounds as 16-bit PCM mono 22050 Hz WAVs from sine tones.
// Run: node scripts/gen-sounds.mjs
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RATE = 22050;
const outDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "sounds");

// Mix a list of {freq, start, dur, amp, decay} partials into a Float32 buffer.
function render(totalSec, partials) {
  const n = Math.round(totalSec * RATE);
  const buf = new Float32Array(n);
  for (const p of partials) {
    const s0 = Math.round(p.start * RATE);
    const len = Math.min(Math.round(p.dur * RATE), n - s0);
    for (let i = 0; i < len; i++) {
      const t = i / RATE;
      const env = Math.exp(-p.decay * t);
      buf[s0 + i] += p.amp * env * Math.sin(2 * Math.PI * p.freq * t);
    }
  }
  // Quick fade in/out so there is no click at either end.
  const fade = Math.round(0.008 * RATE);
  for (let i = 0; i < fade; i++) {
    const g = i / fade;
    buf[i] *= g;
    buf[n - 1 - i] *= g;
  }
  return buf;
}

function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => {
    const c = Math.max(-1, Math.min(1, v));
    data.writeInt16LE(Math.round(c * 32767), i * 2);
  });
  const h = Buffer.alloc(44);
  h.write("RIFF", 0);
  h.writeUInt32LE(36 + data.length, 4);
  h.write("WAVEfmt ", 8);
  h.writeUInt32LE(16, 16);
  h.writeUInt16LE(1, 20); // PCM
  h.writeUInt16LE(1, 22); // mono
  h.writeUInt32LE(RATE, 24);
  h.writeUInt32LE(RATE * 2, 28);
  h.writeUInt16LE(2, 32);
  h.writeUInt16LE(16, 34);
  h.write("data", 36);
  h.writeUInt32LE(data.length, 40);
  return Buffer.concat([h, data]);
}

// alert: gentle two-note chime (E5 then A5), ~0.6 s.
const alert = render(0.6, [
  { freq: 659.25, start: 0.0, dur: 0.35, amp: 0.35, decay: 7 },
  { freq: 880.0, start: 0.17, dur: 0.43, amp: 0.35, decay: 7 },
]);

// bell: single decaying tone (C6) with a harmonic partial, ~1.0 s.
const bell = render(1.0, [
  { freq: 1046.5, start: 0, dur: 1.0, amp: 0.4, decay: 5 },
  { freq: 2093.0, start: 0, dur: 1.0, amp: 0.15, decay: 8 },
]);

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "alert.wav"), wav(alert));
writeFileSync(join(outDir, "bell.wav"), wav(bell));
console.log("wrote alert.wav and bell.wav to", outDir);
