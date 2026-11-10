// Monthly hats, drawn in the robot's 140x140 viewBox with the head's top at y=22. Hats rise
// up to 30 units above the viewBox, into the room the window leaves above the robot. The art
// is the approved mockup; shapes used twice live in HAT_DEFS.

export const HAT_DEFS = `<defs>
  <path id="hat-heart" d="M0 -2 C -3 -7 -10 -5 -9 1 C -8 5 -3 8 0 11 C 3 8 8 5 9 1 C 10 -5 3 -7 0 -2 z"/>
  <polygon id="hat-star" points="0,-3 0.9,-0.9 3,-0.9 1.3,0.4 1.9,2.6 0,1.3 -1.9,2.6 -1.3,0.4 -3,-0.9 -0.9,-0.9"/>
  <path id="hat-drop" d="M0 -5 Q4 1 0 3 Q-4 1 0 -5 z"/>
  <path id="hat-flake" d="M0 -5 V5 M-4.3 -2.5 L4.3 2.5 M-4.3 2.5 L4.3 -2.5" stroke-width="1.6" stroke-linecap="round"/>
</defs>`;

const HATS = {
  // January: New Year party cone.
  1: `<polygon points="52,27 88,27 70,-16" fill="#a855f7"/>
    <path d="M60 4 L80 4 M57 14 L83 14" stroke="#fbbf24" stroke-width="3"/>
    <circle cx="64" cy="20" r="1.6" fill="#f0abfc"/><circle cx="76" cy="9" r="1.6" fill="#f0abfc"/>
    <circle cx="70" cy="-18" r="5.5" fill="#fbbf24"/>
    <g fill="#fbbf24"><use href="#hat-star" x="100" y="0"/><use href="#hat-star" x="36" y="-8"/><use href="#hat-star" x="108" y="-18"/></g>`,
  // February: bobbing heart headband.
  2: `<path d="M30 38 Q70 2 110 38" fill="none" stroke="#ec4899" stroke-width="4" stroke-linecap="round"/>
    <path d="M54 19 Q46 8 50 -2 M86 19 Q94 8 90 -2" fill="none" stroke="#9d174d" stroke-width="2"/>
    <g fill="#f43f5e"><use href="#hat-heart" x="49" y="-10"/><use href="#hat-heart" x="91" y="-10"/></g>`,
  // March: green top hat with a shamrock.
  3: `<rect x="48" y="-16" width="44" height="40" rx="2" fill="#16a34a"/>
    <rect x="48" y="12" width="44" height="7" fill="#052e16"/>
    <ellipse cx="70" cy="25" rx="34" ry="5" fill="#15803d"/>
    <g fill="#86efac"><circle cx="80" cy="-1" r="4"/><circle cx="86" cy="-1" r="4"/><circle cx="83" cy="-6" r="4"/></g>
    <path d="M83 0 L85 8" stroke="#86efac" stroke-width="1.5"/>`,
  // April: yellow rain hat in April showers.
  4: `<path d="M50 22 Q50 -2 70 -2 Q90 -2 90 22 z" fill="#facc15"/>
    <path d="M60 0 Q58 10 59 21 M80 0 Q82 10 81 21" fill="none" stroke="#ca8a04" stroke-width="1"/>
    <path d="M26 36 Q30 16 70 16 Q110 16 114 36 Q100 26 70 25 Q40 26 26 36 z" fill="#eab308"/>
    <path d="M30 33 Q36 22 70 21 Q104 22 110 33" fill="none" stroke="#ca8a04" stroke-width="1" stroke-dasharray="2 2"/>
    <g fill="#60a5fa"><use href="#hat-drop" x="34" y="-8"/><use href="#hat-drop" x="104" y="-2"/><use href="#hat-drop" x="116" y="-20"/><use href="#hat-drop" x="22" y="8"/><use href="#hat-drop" x="44" y="-22"/></g>`,
  // May: flower crown.
  5: `<path d="M32 31 Q70 13 108 31" fill="none" stroke="#16a34a" stroke-width="3.5" stroke-linecap="round"/>
    <circle cx="38" cy="26" r="6" fill="#f472b6"/><circle cx="54" cy="19" r="6" fill="#fde047"/><circle cx="70" cy="17" r="6.5" fill="#a78bfa"/><circle cx="86" cy="19" r="6" fill="#fb923c"/><circle cx="102" cy="26" r="6" fill="#f472b6"/>
    <g fill="#fef9c3"><circle cx="38" cy="26" r="2"/><circle cx="54" cy="19" r="2"/><circle cx="70" cy="17" r="2.2"/><circle cx="86" cy="19" r="2"/><circle cx="102" cy="26" r="2"/></g>
    <g fill="#22c55e"><ellipse cx="46" cy="24" rx="4" ry="2" transform="rotate(-25 46 24)"/><ellipse cx="94" cy="24" rx="4" ry="2" transform="rotate(25 94 24)"/></g>`,
  // June: straw sun hat.
  6: `<path d="M46 25 Q46 -4 70 -4 Q94 -4 94 25 z" fill="#fde68a"/>
    <rect x="46" y="14" width="48" height="7" fill="#f97316"/>
    <ellipse cx="70" cy="25" rx="50" ry="8" fill="#fcd34d"/>
    <path d="M28 25 Q70 31 112 25" fill="none" stroke="#d97706" stroke-width="1" stroke-dasharray="3 3"/>`,
  // July: stars-and-stripes top hat.
  7: `<rect x="50" y="-20" width="40" height="44" fill="#f8fafc"/>
    <g fill="#dc2626"><rect x="50" y="-20" width="40" height="5"/><rect x="50" y="-10" width="40" height="5"/><rect x="50" y="0" width="40" height="5"/></g>
    <rect x="50" y="10" width="40" height="10" fill="#1d4ed8"/>
    <g fill="#f8fafc"><use href="#hat-star" x="57" y="15"/><use href="#hat-star" x="65" y="15"/><use href="#hat-star" x="75" y="15"/><use href="#hat-star" x="83" y="15"/></g>
    <ellipse cx="70" cy="25" rx="34" ry="5" fill="#1e3a8a"/>`,
  // August: beach bucket hat.
  8: `<path d="M48 20 L52 -2 Q70 -8 88 -2 L92 20 z" fill="#38bdf8"/>
    <rect x="48" y="14" width="44" height="6" fill="#0369a1"/>
    <path d="M36 28 L48 18 L92 18 L104 28 Q70 34 36 28 z" fill="#7dd3fc"/>
    <path d="M42 26 Q70 31 98 26" fill="none" stroke="#0369a1" stroke-width="1" stroke-dasharray="2 2"/>`,
  // September: knit beanie with a falling leaf.
  9: `<path d="M32 30 Q32 -6 70 -6 Q108 -6 108 30 z" fill="#c2410c"/>
    <path d="M50 2 V22 M60 -3 V22 M70 -5 V22 M80 -3 V22 M90 2 V22" stroke="#9a3412" stroke-width="1.5"/>
    <rect x="28" y="20" width="84" height="11" rx="5.5" fill="#9a3412"/>
    <circle cx="70" cy="-10" r="7.5" fill="#fdba74"/>
    <g transform="translate(118 -4) rotate(30)"><path d="M0 -7 Q6 -2 0 7 Q-6 -2 0 -7 z" fill="#ea580c"/><path d="M0 -6 V9" stroke="#7c2d12" stroke-width="1"/></g>`,
  // October: witch hat.
  10: `<path d="M48 24 Q60 -2 74 -26 Q88 -22 96 -12 Q84 -14 82 -8 Q86 8 92 24 z" fill="#312e81"/>
    <path d="M49 14 L91 14 L92 21 L48 21 z" fill="#a855f7"/>
    <rect x="65" y="13" width="10" height="9" rx="1.5" fill="none" stroke="#fbbf24" stroke-width="2"/>
    <ellipse cx="70" cy="25" rx="46" ry="6" fill="#1e1b4b"/>`,
  // November: pilgrim hat with a buckle.
  11: `<path d="M52 24 L56 -14 L84 -14 L88 24 z" fill="#292524"/>
    <rect x="53" y="10" width="34" height="9" fill="#0c0a09"/>
    <rect x="64" y="9" width="12" height="11" rx="1" fill="none" stroke="#fbbf24" stroke-width="2.2"/>
    <ellipse cx="70" cy="25" rx="36" ry="5" fill="#1c1917"/>`,
  // December: fluffy earmuffs in falling snow.
  12: `<path d="M34 34 Q34 -6 70 -6 Q106 -6 106 34" fill="none" stroke="#64748b" stroke-width="4" stroke-linecap="round"/>
    <g fill="#f8fafc" stroke="#cbd5e1" stroke-width="1"><circle cx="32" cy="34" r="11"/><circle cx="108" cy="34" r="11"/></g>
    <g fill="#bae6fd"><circle cx="32" cy="34" r="5.5"/><circle cx="108" cy="34" r="5.5"/></g>
    <g stroke="#bae6fd"><use href="#hat-flake" x="50" y="-14"/><use href="#hat-flake" x="92" y="-20"/><use href="#hat-flake" x="118" y="2"/><use href="#hat-flake" x="22" y="-4"/><use href="#hat-flake" x="70" y="-24"/></g>`,
};

/** The hat for `month` (1-12) as SVG markup, or "" for none. */
export function hatMarkup(month) {
  return Number.isInteger(month) && Object.hasOwn(HATS, month) ? HATS[month] : "";
}
