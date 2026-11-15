// The 3D skeleton: an original skeleton bust modelled and rendered in Blender
// (art/skeleton3d/build.py). Each state is one strip of pre-rendered frames side by side;
// skeleton3d.css shows the current state's strip and steps through it with transform only.
// It wears no hats yet, and the ids below exist only so the shared poses and marks find them.

import strips from "./skeleton3d/strips.js";

const url = (state) => new URL(`./skeleton3d/${state}.webp`, import.meta.url).href;

/** One state's strip in a 140x140 window that shows a single frame at a time. */
function strip(state, { frames, seconds, loop }) {
  // A repeating strip steps through every frame; one that plays once stops on its last.
  const steps = loop ? frames : frames - 1;
  return `
  <svg id="s3-${state}" class="s3" width="140" height="140" viewBox="0 0 140 140">
    <image class="s3-strip" width="${140 * frames}" height="140" href="${url(state)}" preserveAspectRatio="none"
      style="--steps:${steps};--dur:${seconds}s;--iter:${loop ? "infinite" : 1}"/>
  </svg>`;
}

export default {
  id: "skeleton3d",
  hatTransform: "",
  art: `${Object.entries(strips).map(([state, s]) => strip(state, s)).join("")}
  <g id="keys"></g><g id="book"><g id="book-page"></g></g><g id="bell"></g>
  <g id="head"><g id="eyes"></g><g id="hat"></g></g>
`,
};
