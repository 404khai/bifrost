#!/usr/bin/env python3
"""Summarize three baseline and three optimized runs; keep raw samples separately."""
import json
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "docs/benchmark-results"


def main():
    groups = {name: [json.loads((RESULTS / f"{name}-{i}.json").read_text()) for i in range(1, 4)]
              for name in ("baseline", "optimized")}
    for group in groups.values():
        assert len({run["environment"]["streaming_sha256"] for run in group}) == 1
        assert len({run["environment"]["harness_sha256"] for run in group}) == 1
    assert groups["baseline"][0]["environment"]["harness_sha256"] == groups["optimized"][0]["environment"]["harness_sha256"]
    assert groups["baseline"][0]["environment"]["fixture_sha256"] == groups["optimized"][0]["environment"]["fixture_sha256"]
    measurements = {}
    for name, runs in groups.items():
        keys = [(row["case"], row["method"]) for row in runs[0]["rows"]]
        measurements[name] = {}
        for case, method in keys:
            rows = [next(row for row in run["rows"] if row["case"] == case and row["method"] == method) for run in runs]
            medians = [row["median_ns"] for row in rows]
            measurements[name][(case, method)] = {
                "median_ns": median(medians), "run_medians_ns": medians,
                "min_run_median_ns": min(medians), "max_run_median_ns": max(medians),
                "representative_p05_ns": rows[0]["p05_ns"], "representative_p95_ns": rows[0]["p95_ns"],
                "bytes": rows[0].get("bytes"),
            }
    summary = []
    for (case, method), before in measurements["baseline"].items():
        after = measurements["optimized"][(case, method)]
        summary.append({"case": case, "method": method, "baseline": before, "optimized": after,
                        "latency_change_percent": (after["median_ns"] / before["median_ns"] - 1) * 100})
    output = {"definition": "median of three run medians; p05/p95 are batch-mean spread from run 1, not request tails",
              "measurements": summary, "allocations": {name: runs[0]["allocations"] for name, runs in groups.items()}}
    (RESULTS / "summary.json").write_text(json.dumps(output, indent=2) + "\n")
    for row in summary:
        if row["method"] == "stream_e2e":
            value = measurements["optimized"][(row["case"], "value_e2e")]["median_ns"]
            after = row["optimized"]
            print(f'{row["case"]:24} value={value/1000:9.3f} us stream_before={row["baseline"]["median_ns"]/1000:9.3f} us '
                  f'after={after["median_ns"]/1000:9.3f} us change={row["latency_change_percent"]:6.1f}% '
                  f'range=[{after["min_run_median_ns"]/1000:.3f}, {after["max_run_median_ns"]/1000:.3f}]')


if __name__ == "__main__":
    main()
