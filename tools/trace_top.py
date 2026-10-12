#!/usr/bin/env python3
"""Sums a Chrome trace (`just profile`, Bevy's `trace_chrome`) per span: where each frame's time goes.

    python3 tools/trace_top.py target/trace.json [rows] [filter]

Prints, most expensive first: total ms, calls, ms per call, span name. `filter` keeps the spans whose
name contains it (e.g. `game_client` for our own systems).
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


def totals(events):
    """Total microseconds and calls per span label, from begin/end pairs per thread and complete events."""
    started = collections.defaultdict(list)
    total, calls = collections.Counter(), collections.Counter()
    for e in events:
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
    rows = int(sys.argv[2]) if len(sys.argv) > 2 else 40
    keep = sys.argv[3] if len(sys.argv) > 3 else ""
    total, calls = totals(load(sys.argv[1]))
    shown = [(k, us) for k, us in total.most_common() if keep in k][:rows]
    for name, us in shown:
        n = calls[name]
        print(f"{us / 1000:10.1f} ms {n:7d}x {us / max(n, 1) / 1000:9.3f} ms/call  {name[:150]}")


if __name__ == "__main__":
    main()
