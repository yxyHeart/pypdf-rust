#!/usr/bin/env python3
"""Merge the dedicated wall-phase pass (``--mode phases``) into the main JSON."""
import argparse
import json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--main", required=True)
    parser.add_argument("--phases", required=True)
    args = parser.parse_args()

    main = json.load(open(args.main, encoding="utf-8"))
    phases = json.load(open(args.phases, encoding="utf-8"))
    merged = []
    for wid, entry in phases["workloads"].items():
        if "wall_phases" not in entry:
            continue
        main["workloads"].setdefault(wid, {})
        main["workloads"][wid]["wall_phases"] = entry["wall_phases"]
        main["workloads"][wid]["wall_phases_source"] = {
            "file": args.phases,
            "repeat": entry["wall_phases"]["repeat"],
            "loadavg_start": phases["env"].get("loadavg_start"),
            "loadavg_end": phases["env"].get("loadavg_end"),
        }
        merged.append(wid)
    with open(args.main, "w", encoding="utf-8") as handle:
        json.dump(main, handle, indent=1)
    print("merged wall phases for", merged, "into", args.main)


if __name__ == "__main__":
    main()
