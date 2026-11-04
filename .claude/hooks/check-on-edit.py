"""PostToolUse hook: after Claude edits a Rust or TypeScript file, run the matching
compiler check and feed any errors straight back, so breakages in callers are seen
immediately instead of at the end of a task (see .claude/skills/build/impact-analysis.md).
"""

import json
import os
import subprocess
import sys

MAX_LINES = 40


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0
    tool_input = payload.get("tool_input") or {}
    path = (tool_input.get("file_path") or "").replace("\\", "/")
    # Check the checkout the edited file lives in (an agent worktree under .claude/worktrees/
    # has its own code, .env and DB), not always the main project dir.
    marker = next((m for m in ("/apps/api/", "/apps/web/", "/packages/ui/") if m in path), None)
    root = path[: path.index(marker)] if marker else (os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd())

    if path.endswith(".rs") and "/apps/api/" in path:
        cmd = ["cargo", "check", "--workspace", "--all-targets", "--quiet", "--message-format", "short"]
        cwd = os.path.join(root, "apps", "api")
        label = "cargo check"
    elif path.endswith((".ts", ".tsx")) and ("/apps/web/" in path or "/packages/ui/" in path):
        cmd = ["pnpm", "--filter", "web", "exec", "tsc", "--noEmit", "--pretty", "false"]
        cwd = root
        label = "tsc (web)"
    else:
        return 0

    try:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=180, shell=(os.name == "nt"))
    except (OSError, subprocess.TimeoutExpired) as exc:
        print(json.dumps({"systemMessage": f"{label} hook skipped: {exc}"}))
        return 0

    if result.returncode == 0:
        return 0
    output = ((result.stdout or "") + (result.stderr or "")).strip().splitlines()
    errors = [line for line in output if "error" in line.lower() and "hard linking" not in line] or output
    snippet = "\n".join(errors[:MAX_LINES])
    print(json.dumps({
        "decision": "block",
        "reason": f"{label} failed after editing {path}. Fix these before continuing "
                  f"(check every caller, per impact analysis):\n{snippet}",
    }))
    return 0


if __name__ == "__main__":
    sys.exit(main())
