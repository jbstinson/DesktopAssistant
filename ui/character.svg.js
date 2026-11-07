// Builds the character SVG: the chosen avatar's art (ui/avatars/), then the marks every
// avatar shares (thinking dots, zzz, alarm, count badge), drawn on top. Code elsewhere only
// sets body[data-state] and body[data-avatar]; poses live in CSS and target the ids. The
// month's hat goes in #hat, inside each avatar's #head.

import { HAT_DEFS } from "./hats.js";
import { avatarFor } from "./avatars/index.js";

export const MARKS = `
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
`;

/** Puts the character for avatar `id` into `container`, replacing only an earlier character
 * SVG so siblings such as the warning marker survive, and returns the elements the app updates. */
export function mountCharacter(container, id) {
  const avatar = avatarFor(id);
  const markup = `<svg id="character" viewBox="0 0 140 140" width="140" height="140" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  ${HAT_DEFS}${avatar.art}${MARKS}</svg>`;
  const old = container.querySelector("svg#character");
  if (old) old.outerHTML = markup;
  else container.insertAdjacentHTML("afterbegin", markup);
  const hat = container.querySelector("#hat");
  if (avatar.hatTransform) hat.setAttribute("transform", avatar.hatTransform);
  return { badgeCount: container.querySelector("#badge-count"), hat };
}
