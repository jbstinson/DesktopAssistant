// Decides which clip of the 3D skeleton plays. Entering a state plays its "<state>-in"
// transition first when it has one, then the state's loop. While idle, every 8 to 20 seconds it
// plays a random "fidget-*" clip, then goes back to idle. Clips that share the state's name and
// play once (done, error) simply hold their last frame.
//
// `show(name)` puts a clip on screen; the caller reports finished one-off clips with `ended`.
// Timers and randomness are passed in so tests can drive them.

export const FIDGET_MIN_MS = 8000;
export const FIDGET_SPREAD_MS = 12000;

export function createPlayer(clips, show, timers = globalThis, random = Math.random) {
  const fidgets = clips.filter((c) => c.startsWith("fidget-"));
  let state = null;
  let clip = null;
  let timer = null;

  const play = (name) => {
    clip = name;
    show(name);
  };

  function schedule() {
    timers.clearTimeout(timer);
    timer = null;
    if (state !== "idle" || fidgets.length === 0) return;
    timer = timers.setTimeout(() => {
      timer = null;
      if (state === "idle" && clip === "idle") play(fidgets[Math.floor(random() * fidgets.length)]);
    }, FIDGET_MIN_MS + random() * FIDGET_SPREAD_MS);
  }

  return {
    setState(next) {
      if (next === state) return;
      state = next;
      play(clips.includes(`${next}-in`) ? `${next}-in` : next);
      schedule();
    },
    /** A clip that plays once has finished: a transition or fidget gives way to the state's loop. */
    ended(name) {
      if (name !== clip || name === state) return;
      play(state);
      schedule();
    },
    stop() {
      timers.clearTimeout(timer);
      timer = null;
    },
  };
}
