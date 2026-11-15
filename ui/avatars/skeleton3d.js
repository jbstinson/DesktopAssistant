// The 3D skeleton: an original skeleton bust modelled and rendered in Blender
// (art/skeleton3d/build.py). Each clip is one strip of pre-rendered frames side by side;
// skeleton3d.css steps through the strip on screen with transform only. A player picks the clip:
// a transition into each state, the state's loop, and now and then a fidget while idle.
// It wears no hats yet, and the ids at the end exist only so the shared poses and marks find them.

import strips from "./skeleton3d/strips.js";
import { createPlayer } from "./skeleton3d-player.js";

const url = (clip) => new URL(`./skeleton3d/${clip}.webp`, import.meta.url).href;

/** One clip's strip in a 140x140 window that shows a single frame at a time. */
function strip(clip, { frames, seconds, loop }) {
  // A repeating strip steps through every frame; one that plays once stops on its last.
  const steps = loop ? frames : frames - 1;
  return `
  <svg id="s3-${clip}" class="s3" width="140" height="140" viewBox="0 0 140 140">
    <image class="s3-strip" width="${140 * frames}" height="140" href="${url(clip)}" preserveAspectRatio="none"
      style="--steps:${steps};--dur:${seconds}s;--iter:${loop ? "infinite" : 1}"/>
  </svg>`;
}

/** Drives the clips inside a mounted character `svg`. Returns the player (setState, stop). */
function player(svg) {
  const byName = new Map([...svg.querySelectorAll(".s3")].map((el) => [el.id.slice(3), el]));
  // Marks the character as player-driven, so the CSS shows only the clip marked "on".
  svg.dataset.clip = "";
  let shown = null;
  const p = createPlayer([...byName.keys()], (name) => {
    shown?.classList.remove("on");
    shown = byName.get(name) ?? null;
    shown?.classList.add("on");
  });
  svg.addEventListener("animationend", (e) => {
    const el = e.target.closest?.(".s3");
    if (el) p.ended(el.id.slice(3));
  });
  return p;
}

export default {
  id: "skeleton3d",
  hatTransform: "",
  player,
  art: `${Object.entries(strips).map(([clip, s]) => strip(clip, s)).join("")}
  <g id="keys"></g><g id="book"><g id="book-page"></g></g><g id="bell"></g>
  <g id="head"><g id="eyes"></g><g id="hat"></g></g>
`,
};
