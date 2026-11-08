// The skeleton, from the ribs up, in a gothic memento mori style: aged bone, hollow sockets with
// faint crimson embers, worn teeth, and an iron cross on a chain. Its arms
// are built like the robot's (shoulders at (36.5, 92) and (103.5, 92), elbows 15 below, three
// finger bones per hand) so it types, reads, waves and rings the bell with the robot's moves.
// The embers move inside fixed eye sockets. Poses live in skeleton.css.

import { BELL, BOOK, KEYS } from "./props.js";

const BONE = "#e8dfc9";
const SHADE = "#bfb293";
const LINE = "#3b342b";
const VOID = "#0c0a09";

/** One arm: upper arm, then the forearm group that bends at the elbow, ending in finger bones. */
function arm(side, x) {
  const s = side[0];
  const fingers = [-4.2, -1.2, 1.8]
    .map((dx, i) => `<rect id="finger-${s}${i + 1}" class="finger" x="${x + dx}" y="124" width="2.2" height="6.5" rx="1.1" fill="${BONE}" stroke="${LINE}" stroke-width="0.8"/>`)
    .join("\n        ");
  return `
  <g id="arm-${side}">
    <rect x="${x - 2}" y="91" width="4" height="17" rx="2" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
    <circle cx="${x}" cy="92" r="4.5" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
    <g id="forearm-${side}">
      <rect x="${x - 3.5}" y="107" width="2.6" height="14" rx="1.3" fill="${BONE}" stroke="${LINE}" stroke-width="1"/>
      <rect x="${x + 0.9}" y="107" width="2.6" height="14" rx="1.3" fill="${BONE}" stroke="${LINE}" stroke-width="1"/>
      <circle cx="${x}" cy="107" r="3.8" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
        ${fingers}
      <ellipse cx="${x}" cy="122" rx="4.3" ry="3.4" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
    </g>
  </g>`;
}

/** A rib pair curving out from the spine at height y, `w` wide on each side. */
const rib = (y, w) =>
  `<path d="M70 ${y} Q${70 - w} ${y - 3} ${70 - w - 3} ${y + 9} M70 ${y} Q${70 + w} ${y - 3} ${70 + w + 3} ${y + 9}"/>`;

const RIBS = [rib(95, 17), rib(103, 16), rib(111, 13)].join("");

/** Teeth along the jaw, slightly uneven. */
const TEETH = [53, 58, 63, 68, 73, 78, 83]
  .map((x, i) => `<rect x="${x}" y="69.5" width="4.2" height="${i % 2 ? 7.5 : 7}" rx="1.2" fill="${BONE}" stroke="${LINE}" stroke-width="0.9"/>`)
  .join("");

export default {
  id: "skeleton",
  hatTransform: "",
  art: `
${KEYS}
  <ellipse cx="70" cy="105" rx="22" ry="17" fill="${VOID}" opacity="0.55"/>
  <g fill="none" stroke-linecap="round">
    <g stroke="${LINE}" stroke-width="5.5">${RIBS}<path d="M40 90 Q70 83 100 90"/></g>
    <g stroke="${BONE}" stroke-width="3.2">${RIBS}<path d="M40 90 Q70 83 100 90"/></g>
  </g>
  <rect x="66.5" y="80" width="7" height="42" rx="3.5" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
  <path d="M66.5 88 H73.5 M66.5 96 H73.5 M66.5 104 H73.5 M66.5 112 H73.5" stroke="${LINE}" stroke-width="1"/>
  <path d="M56 86 Q70 99 84 86" fill="none" stroke="#a1a1aa" stroke-width="1" stroke-dasharray="1.6 1.2"/>
  <g fill="#d4d4d8" stroke="#18181b" stroke-width="1">
    <rect x="68.2" y="96" width="3.6" height="15" rx="0.6"/>
    <rect x="64" y="99.5" width="12" height="3.6" rx="0.6"/>
  </g>
${BOOK}
${arm("left", 36.5)}
${arm("right", 103.5)}
${BELL}
  <g id="head">
    <path d="M45 62 Q44 80 56 82 H84 Q96 80 95 62 Z" fill="${BONE}" stroke="${LINE}" stroke-width="1.5"/>
    <path d="M70 18 C46 18 30 30 30 48 C30 60 36 66 44 70 H96 C104 66 110 60 110 48 C110 30 94 18 70 18 Z" fill="${BONE}" stroke="${LINE}" stroke-width="1.5"/>
    <path d="M34 50 Q36 62 46 66 M106 50 Q104 62 94 66" fill="none" stroke="${SHADE}" stroke-width="4" stroke-linecap="round"/>
    <path d="M42 40 Q54 33 66 40 M74 40 Q86 33 98 40" fill="none" stroke="${SHADE}" stroke-width="4" stroke-linecap="round"/>
    <path d="M70 19 V29 M88 22 l-4 7 l5 4 l-3 6 M48 25 l3 6 l-4 5" fill="none" stroke="${LINE}" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round"/>
    <path d="M43 44 Q44 37 55 38 Q66 40 65 48 Q65 60 55 60 Q43 59 43 44 Z" fill="${VOID}"/>
    <path d="M97 44 Q96 37 85 38 Q74 40 75 48 Q75 60 85 60 Q97 59 97 44 Z" fill="${VOID}"/>
    <g id="eyes">
      <g id="eyes-shape">
        <circle cx="54" cy="50" r="2.6" fill="#991b1b"/>
        <circle cx="86" cy="50" r="2.6" fill="#991b1b"/>
        <circle cx="54.6" cy="49.4" r="0.9" fill="#fca5a5"/>
        <circle cx="86.6" cy="49.4" r="0.9" fill="#fca5a5"/>
      </g>
    </g>
    <path d="M70 57 L66 66 Q70 68 74 66 Z" fill="${VOID}"/>
    <path d="M51 69.5 H89" stroke="${LINE}" stroke-width="1"/>
    ${TEETH}
    <g id="hat"></g>
  </g>
`,
};
