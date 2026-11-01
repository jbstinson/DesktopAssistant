// The session panel. Pure helpers are exported for tests (scripts/panel.test.mjs); the DOM
// code only runs when mountPanel is called. Every string from a session goes in via
// textContent, because project names come from folder names.

const SOURCES = { "claude-desktop": "Desktop", cli: "Terminal", "sdk-cli": "Background" };
const STATES = {
  "needs-you": "Needs you",
  error: "Error",
  typing: "Typing",
  reading: "Reading",
  thinking: "Thinking",
  done: "Done",
  asleep: "Idle",
};

/** Elapsed time refreshes this often while the panel is open, and never otherwise. */
export const REFRESH_MS = 30000;

export function sourceLabel(source) {
  return Object.hasOwn(SOURCES, source) ? SOURCES[source] : "Unknown";
}

export function stateLabel(state) {
  return Object.hasOwn(STATES, state) ? STATES[state] : state;
}

/** "<1m" under a minute, otherwise whole minutes: "3m", "75m". */
export function elapsedLabel(sinceSecs, nowSecs) {
  const minutes = Math.floor(Math.max(0, nowSecs - sinceSecs) / 60);
  return minutes < 1 ? "<1m" : `${minutes}m`;
}

/** Short text for a failed jump; Rust errors arrive as plain strings. */
export function jumpNotice(err) {
  const why = typeof err === "string" ? err : err?.message;
  return why ? `Could not jump: ${why}` : "Could not jump";
}

/** How long a notice stays in the warning area. */
const NOTICE_MS = 4000;

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/**
 * Mounts the panel into `root`. Returns { render(snapshot), setOpen(open) }.
 * `onDismiss(id)` and `onJump(id)` are called from row buttons.
 */
export function mountPanel(root, { onDismiss, onJump }) {
  const warning = el("div", "warning");
  warning.hidden = true;
  const list = el("ul", "sessions");
  root.append(warning, list);

  let snap = null;
  let timer = null;
  let notice = "";
  let noticeTimer = null;
  let isOpen = false;
  const nowSecs = () => Math.floor(Date.now() / 1000);

  function row(s, now) {
    const li = el("li", "session");
    li.dataset.state = s.state;
    const main = el("button", "session-main");
    main.type = "button";
    main.append(
      el("span", "project", s.project || "(unnamed)"),
      el("span", "meta", `${sourceLabel(s.source)} · ${stateLabel(s.state)} · ${elapsedLabel(s.since, now)}`),
    );
    main.classList.toggle("jumpable", Boolean(s.jumpable));
    if (s.jumpable) {
      main.addEventListener("click", async () => {
        try {
          await onJump(s.id);
        } catch (err) {
          showNotice(jumpNotice(err));
        }
      });
    }
    const dismiss = el("button", "dismiss", "×");
    dismiss.type = "button";
    dismiss.title = "Dismiss";
    dismiss.setAttribute("aria-label", `Dismiss ${s.project || "session"}`);
    dismiss.addEventListener("click", () => onDismiss(s.id));
    li.append(main, dismiss);
    return li;
  }

  function showNotice(text) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => {
      notice = "";
      render();
    }, NOTICE_MS);
    render();
  }

  function render(next) {
    if (next) snap = next;
    if (!snap || !isOpen) return; // closed: keep the data, skip the DOM work
    const text = [snap.warning, notice].filter(Boolean).join("; ");
    warning.hidden = !text;
    warning.textContent = text;
    const now = nowSecs();
    if (snap.sessions.length === 0) {
      list.replaceChildren(el("li", "empty", "No active sessions"));
    } else {
      list.replaceChildren(...snap.sessions.map((s) => row(s, now)));
    }
  }

  function setOpen(open) {
    clearInterval(timer);
    timer = null;
    isOpen = open;
    if (open) {
      render();
      timer = setInterval(() => render(), REFRESH_MS);
    }
  }

  return { render, setOpen };
}
