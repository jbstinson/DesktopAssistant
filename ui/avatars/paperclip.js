// The paperclip. It has no arms, so its whole wire body is #head and every pose tilts, bobs or
// sways it from the base at (70, 118). Hats are shrunk to fit the clip's top loop (top at
// (70, 22), 32 units wide). Poses live in paperclip.css.

import { BELL, BOOK, KEYS } from "./props.js";

export default {
  id: "paperclip",
  // Moves a hat's base (70, 26) onto the clip's top at (70, 24), at 42% size.
  hatTransform: "translate(70 24) scale(0.42) translate(-70 -26)",
  art: `
${KEYS}
  <g id="paper">
    <rect x="36" y="100" width="68" height="24" rx="2" fill="#f8fafc"/>
    <path d="M44 108 H96 M44 114 H90 M44 120 H80" stroke="#cbd5e1" stroke-width="1.5"/>
  </g>
${BOOK}
${BELL}
  <g id="head">
    <path d="M62 100 V48 A8 8 0 0 1 78 48 V110 A12 12 0 0 1 54 110 V38 A16 16 0 0 1 86 38 V96" fill="none" stroke="#94a3b8" stroke-width="5" stroke-linecap="round"/>
    <g id="eyes">
      <g id="eyes-shape">
        <circle cx="58" cy="56" r="9" fill="#fff" stroke="#334155" stroke-width="1.5"/><circle cx="82" cy="56" r="9" fill="#fff" stroke="#334155" stroke-width="1.5"/>
        <circle cx="60" cy="58" r="3.5" fill="#111827"/><circle cx="84" cy="58" r="3.5" fill="#111827"/>
      </g>
    </g>
    <path d="M62 76 Q70 82 78 76" fill="none" stroke="#334155" stroke-width="2.5" stroke-linecap="round"/>
    <g id="hat"></g>
  </g>
`,
};
