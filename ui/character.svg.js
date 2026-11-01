// The robot's artwork. Code elsewhere only sets body[data-state]; poses and
// animations live in character.css and target the ids below. To swap the art,
// replace this file and character.css and keep the same ids.

const SVG = `
<svg id="character" viewBox="0 0 140 140" width="140" height="140" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  <g id="keys">
    <rect x="34" y="123" width="72" height="9" rx="3" fill="#475569"/>
    <g fill="#cbd5e1">
      <rect x="39" y="125" width="8" height="2.5" rx="1"/><rect x="50" y="125" width="8" height="2.5" rx="1"/>
      <rect x="61" y="125" width="8" height="2.5" rx="1"/><rect x="72" y="125" width="8" height="2.5" rx="1"/>
      <rect x="83" y="125" width="8" height="2.5" rx="1"/><rect x="94" y="125" width="8" height="2.5" rx="1"/>
    </g>
  </g>

  <rect x="43" y="82" width="54" height="40" rx="16" fill="#14b8a6"/>
  <circle cx="70" cy="102" r="8" fill="#ccfbf1"/>
  <circle cx="70" cy="102" r="3.5" fill="#f59e0b"/>

  <g id="arm-left">
    <rect x="31.0" y="90" width="11" height="20" rx="5.5" fill="#2dd4bf"/>
    <g id="forearm-left">
      <rect x="32.0" y="107" width="9" height="14" rx="4.5" fill="#2dd4bf"/>
      <circle cx="36.5" cy="107" r="5" fill="#0f766e"/>
        <rect id="finger-l1" class="finger" x="32.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
        <rect id="finger-l2" class="finger" x="35.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
        <rect id="finger-l3" class="finger" x="38.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
      <circle cx="36.5" cy="122" r="5" fill="#99f6e4"/>
    </g>
  </g>
  <g id="arm-right">
    <rect x="98.0" y="90" width="11" height="20" rx="5.5" fill="#2dd4bf"/>
    <g id="forearm-right">
      <rect x="99.0" y="107" width="9" height="14" rx="4.5" fill="#2dd4bf"/>
      <circle cx="103.5" cy="107" r="5" fill="#0f766e"/>
        <rect id="finger-r1" class="finger" x="99.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
        <rect id="finger-r2" class="finger" x="102.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
        <rect id="finger-r3" class="finger" x="105.3" y="124" width="2.4" height="6" rx="1.2" fill="#99f6e4"/>
      <circle cx="103.5" cy="122" r="5" fill="#99f6e4"/>
    </g>
  </g>

  <g id="head">
    <line x1="70" y1="24" x2="70" y2="12" stroke="#0f766e" stroke-width="3" stroke-linecap="round"/>
    <circle cx="70" cy="10" r="5" fill="#f59e0b"/>
    <rect x="26" y="22" width="88" height="60" rx="24" fill="#2dd4bf"/>
    <rect x="36" y="32" width="68" height="40" rx="16" fill="#0f172a"/>
    <g id="eyes">
      <g id="eyes-shape">
        <circle cx="56" cy="52" r="8" fill="#67e8f9"/>
        <circle cx="84" cy="52" r="8" fill="#67e8f9"/>
        <circle cx="58" cy="50" r="2.5" fill="#ecfeff"/>
        <circle cx="86" cy="50" r="2.5" fill="#ecfeff"/>
      </g>
    </g>
    <path d="M62 64 Q70 69 78 64" fill="none" stroke="#67e8f9" stroke-width="2.5" stroke-linecap="round"/>
  </g>

  <g id="bell">
    <path d="M122 92 q-9 0 -9 10 v5 h18 v-5 q0 -10 -9 -10 z" fill="#fbbf24"/>
    <rect x="111" y="107" width="22" height="3.5" rx="1.7" fill="#d97706"/>
    <circle cx="122" cy="113" r="2.8" fill="#d97706"/>
  </g>

  <g id="think" fill="#94a3b8">
    <circle id="think-1" cx="114" cy="40" r="3"/>
    <circle id="think-2" cx="120" cy="30" r="4"/>
    <circle id="think-3" cx="128" cy="17" r="5.5"/>
  </g>

  <g id="zzz" fill="#94a3b8" font-family="Segoe UI, sans-serif" font-weight="700">
    <text x="108" y="30" font-size="14">z</text>
    <text x="119" y="19" font-size="10">z</text>
  </g>

  <g id="alarm">
    <circle cx="116" cy="22" r="12" fill="#ef4444"/>
    <rect x="114" y="14" width="4" height="10" rx="2" fill="#fff"/>
    <circle cx="116" cy="29" r="2.2" fill="#fff"/>
  </g>

  <g id="badge">
    <circle cx="116" cy="22" r="13" fill="#ef4444" stroke="#fff" stroke-width="2"/>
    <text id="badge-count" x="116" y="27.5" text-anchor="middle" font-family="Segoe UI, sans-serif" font-size="15" font-weight="700" fill="#fff"></text>
  </g>
</svg>`;

/** Injects the character into `container` and returns the elements the app updates. */
export function mountCharacter(container) {
  container.innerHTML = SVG;
  return { badgeCount: container.querySelector("#badge-count") };
}
