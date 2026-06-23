#!/usr/bin/env python3
"""Audit Keeper tool usage from chat logs.

The Keeper persists every chat as append-only JSONL under
`<World>/.ck/chats/*.jsonl`. This walks those, counts how often each tool is
called and how often it errors, and flags image bloat — the data behind a
"do we still need this tool / is the new one getting used" review.

Usage:
    scripts/keeper_tool_usage.py [WORLD_OR_DATA_ROOT ...]

With no args it scans the default data root (~/Documents/Chronicle Keeper).
Point it at a single world folder or a parent holding several.
"""
import collections
import glob
import json
import os
import sys

DEFAULT_ROOT = os.path.expanduser("~/Documents/Chronicle Keeper")


def chat_files(root):
    # Accept either a world folder (has .ck/) or a parent of world folders.
    direct = glob.glob(os.path.join(root, ".ck", "chats", "*.jsonl"))
    if direct:
        return direct
    return glob.glob(os.path.join(root, "*", ".ck", "chats", "*.jsonl"))


def main(roots):
    calls = collections.Counter()
    errors = collections.Counter()
    result_bytes = collections.Counter()
    turn_errors = []
    image_bytes = 0
    files = 0

    for root in roots:
        for f in chat_files(root):
            files += 1
            for line in open(f, encoding="utf-8", errors="replace"):
                line = line.strip()
                if not line:
                    continue
                try:
                    e = json.loads(line)
                except json.JSONDecodeError:
                    continue
                t = e.get("type")
                if t == "assistant":
                    for tc in e.get("tool_calls", []):
                        calls[tc.get("name", "?")] += 1
                elif t == "tool_result":
                    name = e.get("name", "?")
                    result_bytes[name] += len(e.get("content", "") or "")
                    if e.get("is_error"):
                        errors[name] += 1
                elif t == "user" and e.get("images"):
                    image_bytes += len(json.dumps(e["images"]))
                elif t == "error":
                    turn_errors.append((e.get("message", "") or "")[:140])

    if not files:
        print(f"No chat logs found under: {', '.join(roots)}")
        return 1

    width = max((len(n) for n in calls), default=12)
    print(f"# Keeper tool usage — {files} chat file(s)\n")
    print(f"{'tool':<{width}}  calls  errors  result_bytes")
    for name, n in calls.most_common():
        print(f"{name:<{width}}  {n:>5}  {errors[name]:>6}  {result_bytes[name]:>12}")

    if turn_errors:
        print(f"\n## Turn-level errors ({len(turn_errors)})")
        for m in turn_errors:
            print(f"- {m}")

    if image_bytes:
        print(f"\nInline image bytes in chats: {image_bytes:,} "
              f"({image_bytes / 1_048_576:.1f} MB)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:] or [DEFAULT_ROOT]))
