// Every avatar, in tray order. Each has `art` (SVG markup with the ids the poses and hats
// need) and `hatTransform` (an SVG transform for #hat, or "" to wear hats at full size).

import robot from "./robot.js";
import dog from "./dog.js";
import cat from "./cat.js";
import paperclip from "./paperclip.js";
import skeleton from "./skeleton.js";
import skeleton3d from "./skeleton3d.js";

export const AVATARS = { robot, dog, cat, paperclip, skeleton, skeleton3d };

/** The avatar with this id, or the robot for anything unknown. */
export function avatarFor(id) {
  return typeof id === "string" && Object.hasOwn(AVATARS, id) ? AVATARS[id] : AVATARS.robot;
}
