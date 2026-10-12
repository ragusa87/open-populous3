#!/usr/bin/env python3
"""Sums a Chrome trace (`just profile`, Bevy's `trace_chrome`) per span: where each frame's time goes.

    python3 tools/trace_top.py target/trace.json [rows] [filter] [--all]

Prints, most expensive first: total ms, calls, ms per call, span name. `filter` keeps the spans whose
name contains it (e.g. `game_client` for our own systems). Only what follows the game's `ready` span (the end of
loading, `boot::advance`) is counted, unless `--all` (the boot included) or the trace has no such span.
"""
import collections
import json
import sys


def load(path):
    raw = open(path).read().rstrip().rstrip(",")
    return json.loads(raw if raw.endswith("]") else raw + "]")


def label(event):
    name = (event.get("args") or {}).get("name")
    return f"{event.get('name', '')}: {name}" if name else event.get("name", "")


def ready_at(events):
    """When loading ended: the start of the first `ready` span, None without one."""
    return next((e["ts"] for e in events if e.get("name") == "ready" and e.get("ph") in ("B", "X")), None)


def totals(events, since=None):
    """Total microseconds and calls per span label, from begin/end pairs per thread and complete events, counting
    only spans starting at `since` or later."""
    started = collections.defaultdict(list)
    total, calls = collections.Counter(), collections.Counter()
    for e in events:
        if since is not None and e.get("ts", 0) < since and e.get("ph") in ("B", "X"):
            continue
        key = (e.get("tid"), label(e))
        if e.get("ph") == "B":
            started[key].append(e["ts"])
        elif e.get("ph") == "E" and started[key]:
            total[key[1]] += e["ts"] - started[key].pop()
            calls[key[1]] += 1
        elif e.get("ph") == "X":
            total[key[1]] += e.get("dur", 0)
            calls[key[1]] += 1
    return total, calls


def main():
    args = [a for a in sys.argv[1:] if a != "--all"]
    rows = int(args[1]) if len(args) > 1 else 40
    keep = args[2] if len(args) > 2 else ""
    events = load(args[0])
    since = None if "--all" in sys.argv else ready_at(events)
    total, calls = totals(events, since)
    shown = [(k, us) for k, us in total.most_common() if keep in k][:rows]
    for name, us in shown:
        n = calls[name]
        print(f"{us / 1000:10.1f} ms {n:7d}x {us / max(n, 1) / 1000:9.3f} ms/call  {name[:150]}")


if __name__ == "__main__":
    main()
