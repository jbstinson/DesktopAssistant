// The cat. Front paws hang from shoulders at (54, 90) and (86, 90); the tail swishes from
// (92, 116); the ears pivot at their bases. A hat covers the ears. Poses live in cat.css.

import { BELL, BOOK, KEYS } from "./props.js";

export default {
  id: "cat",
  hatTransform: "",
  art: `
${KEYS}
  <g id="tail"><path d="M92 116 Q124 112 116 86 Q112 74 121 68" fill="none" stroke="#64748b" stroke-width="7" stroke-linecap="round"/></g>
  <rect x="46" y="82" width="48" height="40" rx="18" fill="#94a3b8"/>
  <ellipse cx="70" cy="104" rx="12" ry="13" fill="#e2e8f0"/>
  <ellipse cx="61" cy="123" rx="7" ry="4" fill="#e2e8f0"/><ellipse cx="79" cy="123" rx="7" ry="4" fill="#e2e8f0"/>
${BOOK}
  <g id="paw-left">
    <rect x="49" y="88" width="11" height="29" rx="5.5" fill="#8794a8"/>
    <ellipse cx="54.5" cy="119" rx="7" ry="4.5" fill="#e2e8f0"/>
  </g>
  <g id="paw-right">
    <rect x="80" y="88" width="11" height="29" rx="5.5" fill="#8794a8"/>
    <ellipse cx="85.5" cy="119" rx="7" ry="4.5" fill="#e2e8f0"/>
  </g>
${BELL}
  <g id="head">
    <g id="ear-left"><polygon points="34,44 40,10 62,28" fill="#94a3b8"/><polygon points="40,36 43,18 54,28" fill="#f9a8d4"/></g>
    <g id="ear-right"><polygon points="106,44 100,10 78,28" fill="#94a3b8"/><polygon points="100,36 97,18 86,28" fill="#f9a8d4"/></g>
    <rect x="30" y="22" width="80" height="58" rx="28" fill="#94a3b8"/>
    <path d="M62 26 V34 M70 24 V34 M78 26 V34" stroke="#64748b" stroke-width="3" stroke-linecap="round"/>
    <g id="eyes">
      <g id="eyes-shape">
        <ellipse cx="54" cy="48" rx="8" ry="6.5" fill="#a3e635"/><ellipse cx="86" cy="48" rx="8" ry="6.5" fill="#a3e635"/>
        <ellipse cx="54" cy="48" rx="2" ry="5.5" fill="#111827"/><ellipse cx="86" cy="48" rx="2" ry="5.5" fill="#111827"/>
      </g>
    </g>
    <polygon points="66,58 74,58 70,63" fill="#f472b6"/>
    <path d="M70 63 Q66 69 61 66 M70 63 Q74 69 79 66" fill="none" stroke="#334155" stroke-width="1.8" stroke-linecap="round"/>
    <path d="M48 60 L32 57 M48 64 L32 66 M92 60 L108 57 M92 64 L108 66" stroke="#f8fafc" stroke-width="1.5" stroke-linecap="round"/>
    <g id="hat"></g>
  </g>
`,
};
