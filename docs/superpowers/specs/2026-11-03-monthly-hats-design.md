# DesktopAssistant: Monthly Hats

The robot wears a hat that matches the month's holiday or season, changing on its own. Each month's hat can be turned off. Stage 3 (typing and voice) is on hold while this is built.

## Why

The user wants the character to have more than one look. A hat adds a seasonal touch without changing the robot's shape, its poses, or what any state means. Some people will love one holiday and not the rest, so each month's hat is a separate choice.

## Goals

- On the 1st of each month the robot's hat changes to that month's hat with no action from the user.
- Twelve hats, one per month, as drawn in the approved mockup (`.superpowers/brainstorm/1729-1793704216/content/monthly-hats.html`; the hats are copied below so this document stands alone):

| Month | Theme | Hat |
|---|---|---|
| January | New Year | Purple party cone with gold stripes, gold pom-pom, a few gold sparkle stars |
| February | Valentine's Day | Pink headband with two red hearts on springs |
| March | St. Patrick's Day | Green top hat, dark band, light green shamrock |
| April | Easter and spring | White bunny ears, pink insides |
| May | Spring flowers | Green vine crown with five flowers |
| June | Start of summer | Straw sun hat, wide brim, orange band |
| July | Fourth of July | Red and white striped top hat, blue star band |
| August | Beach days | Light blue bucket hat |
| September | Fall | Rust knit beanie with pom-pom, one falling leaf |
| October | Halloween | Purple witch hat with bent tip and gold buckle |
| November | Thanksgiving | Black pilgrim hat with gold buckle |
| December | Christmas | Red Santa hat, white fur band and pom-pom |

- Each month's hat can be turned on or off on its own. In a month whose hat is off, the robot looks exactly as it does today.
- The choices survive restarts.

## Non-goals

- Other countries' holidays, exact holiday dates (Easter, Thanksgiving) or hats for single days. A hat lasts the whole calendar month.
- Outfits, colour schemes or other characters. Hats only.
- A settings screen.
- Letting people add their own hats.

## Anti-goals

- A hat that hides or crowds anything that carries meaning: the eyes and face, the alert badge with its count, the red alarm, the thinking dots, the "zzz", the bell, the book or the typing hands.
- A hat that floats in place while the head moves. The hat moves with the head in every pose.
- Any rise in idle CPU or GPU. The hat is still artwork, with no extra animation and no timer that wakes the app often.
- Taking focus or changing click-through behaviour in any way.

## Decided

- **Automatic by month.** The hat follows the PC's local date. It changes at local midnight on the 1st, including while the app is running and after the PC wakes from sleep.
- **Art.** The twelve hats above, in the same flat vector style as the robot. The mockup's SVG is the approved art: port it, changing shapes only where needed to clear the marks below.
- **Robot size and place.** The robot keeps its current size and does not move on screen. Room for hats is added above it, and saved positions keep the robot where it was. *(Claude's call.)*
- **Room is always there.** The extra headroom exists in every month, so the robot never jumps when a month without a hat begins. The headroom is click-through. *(Claude's call.)*
- **Clicks.** The hat is not clickable. The robot's hit area stays the circle on the body it is today. *(Claude's call.)*
- **Marks win.** The badge, alarm, thinking dots, "zzz" and bell are drawn above the hat. Where a hat would hide most of one, change that hat's art. *(Claude's call.)*
- **Head motion.** The headroom covers the hat's tip at the head's widest tilt and the startle animation, so no hat is ever clipped. *(Claude's call.)*
- **Month check.** One timer set for the next local midnight, re-checked when the PC wakes from sleep. No check runs more than once a minute. *(Claude's call.)*
- **Tray items.** Plain ticks, January to December, with no marker for the current month and no shortcuts. *(Claude's call.)*
- **Antenna.** Hidden while a hat is shown; back when there is no hat.
- **All poses.** The hat stays on in every state: asleep, idle, typing, reading, thinking, done, needs-you and error.
- **Switches.** All twelve are on by default. *(Claude's call, accepted by the user.)*
- **Where the switches live.** A **Hats** submenu in the tray, beside Voice, with one tick item per month, January to December. Changing a tick takes effect at once. *(Claude's call, accepted by the user.)*
- **Saved choice.** Stored in `%APPDATA%\DesktopAssistant\`, alongside `voice.txt` and `approvals.json`. A missing or unreadable file means all on.

## Left to the builder

- **Room above the head.** The artwork's viewBox is 140×140 with the head's top at y=22, and most hats rise 30–50 units above it. The builder sizes the taller canvas and window from the decisions above, and adjusts window placement, edge clamping and the panel's growth (`panel_rect`) to match, keeping the click-through and focus checks passing.
- **Exact art adjustments.** The badge and alarm (about 116, 22), thinking dots and "zzz" share the top right with the December pom-pom, September leaf, witch hat and sun hat brim.
- The saved file's name and format, following the pattern of `approvals.json`.
- How the month is detected without polling often, for example a timer set for the next midnight.
- Whether the hat art lives in `character.svg.js` or a file of its own, as long as `character.css` poses keep working. The hat belongs inside `#head` so it rotates with it.
- Tests: month-to-hat mapping, off months showing no hat, the saved file loading and defaulting, and a visual check of each hat in each pose.
