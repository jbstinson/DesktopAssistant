// The skeleton, from the ribs up. Its arms are built like the robot's (shoulders at (36.5, 92)
// and (103.5, 92), elbows 15 below, three finger bones per hand) so it types, reads, waves and
// rings the bell with the robot's moves. The glowing pupils move inside fixed eye sockets.
// Poses live in skeleton.css. Bones carry a dark outline so they read on light wallpapers.

import { BELL, BOOK, KEYS } from "./props.js";

const BONE = "#f5f5f4";
const LINE = "#57534e";
const DARK = "#1c1917";

/** One arm: upper arm, then the forearm group that bends at the elbow, ending in finger bones. */
function arm(side, x) {
  const s = side[0];
  const fingers = [-4.2, -1.2, 1.8]
    .map((dx, i) => `<rect id="finger-${s}${i + 1}" class="finger" x="${x + dx}" y="124" width="2.4" height="6" rx="1.2" fill="${BONE}" stroke="${LINE}" stroke-width="0.8"/>`)
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
      <ellipse cx="${x}" cy="122" rx="4.5" ry="3.5" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
    </g>
  </g>`;
}

/** A rib pair curving out from the spine at height y, `w` wide on each side. */
const rib = (y, w) =>
  `<path d="M70 ${y} Q${70 - w} ${y - 3} ${70 - w - 3} ${y + 9} M70 ${y} Q${70 + w} ${y - 3} ${70 + w + 3} ${y + 9}"/>`;

const RIBS = [rib(95, 20), rib(103, 19), rib(111, 16)].join("");

export default {
  id: "skeleton",
  hatTransform: "",
  art: `
${KEYS}
  <ellipse cx="70" cy="104" rx="27" ry="19" fill="${DARK}" opacity="0.35"/>
  <g fill="none" stroke-linecap="round">
    <g stroke="${LINE}" stroke-width="5.5">${RIBS}<path d="M40 90 Q70 83 100 90"/></g>
    <g stroke="${BONE}" stroke-width="3.2">${RIBS}<path d="M40 90 Q70 83 100 90"/></g>
  </g>
  <rect x="66.5" y="80" width="7" height="42" rx="3.5" fill="${BONE}" stroke="${LINE}" stroke-width="1.2"/>
  <path d="M66.5 88 H73.5 M66.5 96 H73.5 M66.5 104 H73.5 M66.5 112 H73.5" stroke="${LINE}" stroke-width="1"/>
${BOOK}
${arm("left", 36.5)}
${arm("right", 103.5)}
${BELL}
  <g id="head">
    <rect x="44" y="60" width="52" height="22" rx="10" fill="${BONE}" stroke="${LINE}" stroke-width="1.5"/>
    <rect x="30" y="20" width="80" height="52" rx="26" fill="${BONE}" stroke="${LINE}" stroke-width="1.5"/>
    <path d="M88 23 l-3 6 l4 4 l-2 5" fill="none" stroke="${LINE}" stroke-width="1.2" stroke-linecap="round"/>
    <ellipse cx="54" cy="48" rx="10.5" ry="11" fill="${DARK}"/>
    <ellipse cx="86" cy="48" rx="10.5" ry="11" fill="${DARK}"/>
    <g id="eyes">
      <g id="eyes-shape">
        <circle cx="54" cy="49" r="3.6" fill="#fdba74"/>
        <circle cx="86" cy="49" r="3.6" fill="#fdba74"/>
        <circle cx="55" cy="48" r="1.2" fill="#fff7ed"/>
        <circle cx="87" cy="48" r="1.2" fill="#fff7ed"/>
      </g>
    </g>
    <path d="M70 57 L65.5 65 H74.5 Z" fill="${DARK}"/>
    <rect x="52" y="69" width="36" height="9" rx="2.5" fill="#fafaf9" stroke="${LINE}" stroke-width="1.2"/>
    <path d="M52 73.5 H88 M58 69 V78 M64 69 V78 M70 69 V78 M76 69 V78 M82 69 V78" stroke="${LINE}" stroke-width="1"/>
    <g id="hat"></g>
  </g>
`,
};
