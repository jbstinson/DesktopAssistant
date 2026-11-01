// Spike: a PermissionRequest hook that misbehaves on purpose. Mode comes from mode.txt beside the project.
import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.env.CLAUDE_PROJECT_DIR ?? process.cwd();
const mode = readFileSync(join(dir, "mode.txt"), "utf8").trim();
let raw = "";
process.stdin.on("data", (c) => (raw += c));
process.stdin.on("end", () => {
  const input = JSON.parse(raw);
  appendFileSync(
    join(dir, "events.jsonl"),
    JSON.stringify({ at: new Date().toISOString(), mode, event: input.hook_event_name, tool: input.tool_name, keys: Object.keys(input), command: input.tool_input?.command, suggestions: input.permission_suggestions }) + "\n",
  );
  const decide = (behavior) =>
    process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "PermissionRequest", decision: { behavior, ...(behavior === "deny" ? { message: "denied by spike" } : {}) } } }));
  switch (mode) {
    case "empty": return process.exit(0);
    case "allow": decide("allow"); return process.exit(0);
    case "deny": decide("deny"); return process.exit(0);
    case "crash": process.stderr.write("boom"); return process.exit(1);
    case "exit2": process.stderr.write("blocked"); return process.exit(2);
    case "badjson": process.stdout.write("{not json"); return process.exit(0);
    case "session":
      process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "PermissionRequest", decision: { behavior: "allow", updatedPermissions: [{ type: "addRules", rules: [{ toolName: "Bash", ruleContent: input.tool_input.command }], behavior: "allow", destination: "session" }] } } }));
      return process.exit(0);
    case "always":
      process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "PermissionRequest", decision: { behavior: "allow", updatedPermissions: input.permission_suggestions } } }));
      return process.exit(0);
    case "hang": return setTimeout(() => process.exit(0), 60000);
  }
});
