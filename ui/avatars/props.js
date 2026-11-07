// Props every animal avatar shares, identical to the robot's: the keyboard for typing, the
// open book for reading, and the hand bell for done.

export const KEYS = `
  <g id="keys">
    <rect x="34" y="123" width="72" height="9" rx="3" fill="#475569"/>
    <g fill="#cbd5e1">
      <rect x="39" y="125" width="8" height="2.5" rx="1"/><rect x="50" y="125" width="8" height="2.5" rx="1"/>
      <rect x="61" y="125" width="8" height="2.5" rx="1"/><rect x="72" y="125" width="8" height="2.5" rx="1"/>
      <rect x="83" y="125" width="8" height="2.5" rx="1"/><rect x="94" y="125" width="8" height="2.5" rx="1"/>
    </g>
  </g>`;

export const BOOK = `
  <g id="book">
    <rect x="42" y="99" width="56" height="27" rx="3" fill="#b45309"/>
    <rect x="45" y="101" width="24.5" height="23" rx="1.5" fill="#f8fafc"/>
    <rect x="70.5" y="101" width="24.5" height="23" rx="1.5" fill="#e2e8f0"/>
    <g fill="#94a3b8"><rect x="48.5" y="105" width="17" height="1.8" rx="0.9"/><rect x="48.5" y="110" width="18" height="1.8" rx="0.9"/><rect x="48.5" y="115" width="14" height="1.8" rx="0.9"/><rect x="74" y="105" width="17" height="1.8" rx="0.9"/><rect x="74" y="110" width="18" height="1.8" rx="0.9"/><rect x="74" y="115" width="14" height="1.8" rx="0.9"/></g>
    <rect id="book-page" x="70.5" y="101" width="24.5" height="23" rx="1.5" fill="#f8fafc"/>
    <rect x="69" y="99" width="2" height="27" fill="#78350f"/>
  </g>`;

export const BELL = `
  <g id="bell">
    <rect x="120.5" y="84" width="3" height="11" rx="1.5" fill="#d97706"/>
    <path d="M122 94 q-9 0 -9 10 v5 h18 v-5 q0 -10 -9 -10 z" fill="#fbbf24"/>
    <rect x="111" y="109" width="22" height="3.5" rx="1.7" fill="#d97706"/>
    <circle cx="122" cy="115" r="2.8" fill="#d97706"/>
  </g>`;
