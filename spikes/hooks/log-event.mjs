// Spike: records which Claude Code hook events fire. Always exits 0 so it can't affect a session.
import { appendFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const logPath = join(dirname(fileURLToPath(import.meta.url)), "events.jsonl");

let raw = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => (raw += chunk));
process.stdin.on("end", () => {
  try {
    const input = JSON.parse(raw);
    appendFileSync(
      logPath,
      JSON.stringify({
        at: new Date().toISOString(),
        event: input.hook_event_name,
        session: input.session_id,
        tool: input.tool_name,
        notificationType: input.notification_type,
        source: input.source,
        entrypoint: process.env.CLAUDE_CODE_ENTRYPOINT,
        inputKeys: Object.keys(input),
      }) + "\n",
    );
  } catch (error) {
    appendFileSync(logPath, JSON.stringify({ at: new Date().toISOString(), error: String(error) }) + "\n");
  }
  process.exit(0);
});
