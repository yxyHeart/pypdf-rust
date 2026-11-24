#!/usr/bin/env python3
"""Merge the dedicated layered-timing pass into the main diagnostics JSON.

The layered pass runs in its own interpreter (no monkeypatching, no other mode in
the same process), so its numbers are the cleanest wall-clock data in this
directory.  This script copies ``workloads.<W>.layers`` from the layers file into
the main file and records where it came from.
"""
import argparse
import json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--main", required=True)
    parser.add_argument("--layers", required=True)
    args = parser.parse_args()

    main = json.load(open(args.main, encoding="utf-8"))
    layers = json.load(open(args.layers, encoding="utf-8"))
    merged = []
    for wid, entry in layers["workloads"].items():
        if "layers" not in entry:
            continue
        main["workloads"].setdefault(wid, {})
        main["workloads"][wid]["layers"] = entry["layers"]
        main["workloads"][wid]["layers_source"] = {
            "file": args.layers,
            "repeat": len(next(iter(entry["layers"].values()))["samples_ms"]),
            "loadavg_start": layers["env"].get("loadavg_start"),
            "loadavg_end": layers["env"].get("loadavg_end"),
        }
        merged.append(wid)
    with open(args.main, "w", encoding="utf-8") as handle:
        json.dump(main, handle, indent=1)
    print("merged layers for", merged, "into", args.main)


if __name__ == "__main__":
    main()
