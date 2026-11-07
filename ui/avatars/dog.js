// The dog. Front paws hang from shoulders at (53, 90) and (87, 90) and rotate there; the tail
// wags from (94, 110); the ears swing from the top of the head. Poses live in dog.css.

import { BELL, BOOK, KEYS } from "./props.js";

export default {
  id: "dog",
  hatTransform: "",
  art: `
${KEYS}
  <g id="tail"><path d="M94 110 Q116 104 112 84" fill="none" stroke="#c2833f" stroke-width="7" stroke-linecap="round"/></g>
  <rect x="44" y="82" width="52" height="40" rx="18" fill="#c2833f"/>
  <ellipse cx="70" cy="104" rx="14" ry="12" fill="#f5deb3"/>
  <ellipse cx="60" cy="123" rx="8" ry="4" fill="#a16207"/><ellipse cx="80" cy="123" rx="8" ry="4" fill="#a16207"/>
  <rect x="48" y="80" width="44" height="6" rx="3" fill="#dc2626"/><circle cx="70" cy="89" r="3.5" fill="#fbbf24"/>
${BOOK}
  <g id="paw-left">
    <rect x="47" y="88" width="12" height="30" rx="6" fill="#b87333"/>
    <ellipse cx="53" cy="119" rx="8" ry="5" fill="#a16207"/>
  </g>
  <g id="paw-right">
    <rect x="81" y="88" width="12" height="30" rx="6" fill="#b87333"/>
    <ellipse cx="87" cy="119" rx="8" ry="5" fill="#a16207"/>
  </g>
${BELL}
  <g id="head">
    <g id="ear-left"><ellipse cx="33" cy="46" rx="10" ry="21" fill="#7c4a1e" transform="rotate(18 33 46)"/></g>
    <g id="ear-right"><ellipse cx="107" cy="46" rx="10" ry="21" fill="#7c4a1e" transform="rotate(-18 107 46)"/></g>
    <rect x="32" y="22" width="76" height="60" rx="28" fill="#c2833f"/>
    <ellipse cx="54" cy="38" rx="9" ry="7" fill="#a16207"/>
    <ellipse cx="70" cy="65" rx="21" ry="14" fill="#f5deb3"/>
    <g id="eyes">
      <g id="eyes-shape">
        <circle cx="55" cy="46" r="5.5" fill="#1f2937"/><circle cx="85" cy="46" r="5.5" fill="#1f2937"/>
        <circle cx="56.5" cy="44.5" r="1.8" fill="#fff"/><circle cx="86.5" cy="44.5" r="1.8" fill="#fff"/>
      </g>
    </g>
    <ellipse cx="70" cy="57" rx="6.5" ry="4.5" fill="#1f2937"/>
    <path d="M70 61 V65 M62 66 Q70 72 78 66" fill="none" stroke="#1f2937" stroke-width="2" stroke-linecap="round"/>
    <ellipse id="tongue" cx="73" cy="72" rx="4" ry="5" fill="#f472b6"/>
    <g id="hat"></g>
  </g>
`,
};
