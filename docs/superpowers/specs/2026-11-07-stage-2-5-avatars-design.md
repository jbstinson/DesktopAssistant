# DesktopAssistant Stage 2.5: Avatars

The on-screen character can take one of four forms instead of only the robot. Stage 3 (typing and voice) stays on hold.

## Why

The user wants the companion to have more than one look, picked to suit their mood. The robot stays the default, so nothing changes for anyone who doesn't choose another.

## Goals

- Four avatars: **Robot** (today's, unchanged), **Dog**, **Cat**, **Paperclip**.
- Picked from an **Avatar** submenu in the tray, beside Voice and Holidays, with a tick on the current one. The change happens at once and is remembered after a restart.
- Every avatar acts out every character state, so the meaning of each state is the same whatever the look: asleep, typing, reading, thinking, needs-you (waving, with the count badge when more than one), done (ringing a bell) and error (alarmed, with the alarm mark).
- Every avatar wears the monthly holiday hats when Holidays is on.

## Art

The approved first sketches are in `.superpowers/brainstorm/1377-1794086840/content/avatars.html` (git-ignored, on disk). Summarised so this document stands alone:

| Avatar | Look |
|---|---|
| Robot | Unchanged. |
| Dog | Brown, floppy dark ears, tan muzzle and belly, black nose, pink tongue, red collar with a gold tag, tail. |
| Cat | Grey tabby, pointed ears with pink insides, forehead stripes, green eyes with slit pupils, pink nose, white whiskers, white chest and paws, curled tail. |
| Paperclip | A silver bent-wire paperclip with two big round eyes and a smile. (The first version stood on a sheet of lined paper; removed at the user's request as too close to Clippy.) |

All art is original. The paperclip is not Microsoft's Clippy: no eyebrows, and its own eyes and smile. The dog and cat are generic.

## Non-goals

- The giant combining robot and the menacing chrome robot (sketched, then dropped by the user).
- Characters from existing franchises (Voltron, Ultron, Clippy).
- Different sounds or voices per avatar.
- Mixing parts between avatars, custom colours, or user-made avatars.
- A settings screen.

## Anti-goals

- An avatar whose state can't be told apart from another state. A glance must still say "needs you", "done" or "error".
- Any rise in idle CPU or GPU. Avatars use the same low-cost stepped animation (transform and opacity only, about 7 updates a second, nothing while hidden).
- Any change to focus, click-through, the window size, the panel or approvals.
- A hat that floats off an avatar's head or hides meaningful marks.

## Decided

- **Default.** Robot, for anyone without a saved choice, or with an unreadable one.
- **Saved choice.** In `%APPDATA%\DesktopAssistant\`, beside `voice.txt` and `hats.json`. An unknown name means Robot.
- **Hats on every avatar.** Robot, dog and cat wear the hats as they are; the cat's ears tuck under the hat. The paperclip wears a smaller version of each hat, scaled to sit on top of its clip. *(User's choice.)*
- **Antenna rule generalised.** Anything on an avatar's head that a hat would clash with (the robot's antenna) hides while a hat shows. The cat's ears don't need hiding; the hat covers them.
- **Size.** Each avatar fits the robot's 140×140 box, head top at about y=22, so the window, the 30 px hat room, the click circle and the panel stay exactly as they are. *(Claude's call.)*
- **Marks stay shared.** The thinking dots, "zzz", alarm, count badge and the warning marker stay where they are for every avatar, drawn above the hat. *(Claude's call.)*
- **Props per state.** Each avatar uses the same props as the robot: a keyboard for typing, an open book for reading, a hand bell for done. How each holds them is up to the art. *(Claude's call.)*

## Left to the builder

- How each avatar acts out each state. Suggestions, not requirements:
  - Dog and cat: paws on the keys for typing, paws on the book for reading, a paw raised and waving for needs-you, the bell held in a paw or on the collar for done, ears and tail up for error, curled up with eyes shut for asleep, and the tail swishing as the stepped animation.
  - Paperclip: leaning over the keyboard for typing, the book open in front for reading, bending its top loop to wave for needs-you, the bell beside it for done, a startled wobble for error.
- How the code is organised. Today `character.svg.js` holds the art and `character.css` targets ids inside it, and `main.js` only sets `body[data-state]`. A reasonable shape is one art module per avatar, pose CSS scoped by `body[data-avatar]`, and the shared marks kept common.
- How the paperclip's hats are scaled and placed, and whether that's a transform or separate art.
- The saved file's name and format.
- Tests: the saved choice loading and defaulting, the tray ticks, and a visual check of every avatar in every state, with and without a hat, extending `scripts/hats-preview.html`.
