// Every avatar, in tray order. Each has `art` (SVG markup with the ids the poses and hats
// need) and `hatTransform` (an SVG transform for #hat, or "" to wear hats at full size).

import robot from "./robot.js";
import dog from "./dog.js";

// Until each avatar has its own art, it borrows the robot's.
const placeholder = (id) => ({ ...robot, id });

export const AVATARS = { robot, dog, cat: placeholder("cat"), paperclip: placeholder("paperclip") };

/** The avatar with this id, or the robot for anything unknown. */
export function avatarFor(id) {
  return typeof id === "string" && Object.hasOwn(AVATARS, id) ? AVATARS[id] : AVATARS.robot;
}
