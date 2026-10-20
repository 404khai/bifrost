#!/usr/bin/env python3
"""Build benchmark executables and preserve raw measurements and environment data."""
import argparse
import hashlib
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import tempfile
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def executables(build_root):
    build = subprocess.run(
        ["cargo", "bench", "--no-run", "--message-format=json"],
        cwd=build_root, text=True, stdout=subprocess.PIPE, check=True,
    )
    result = {}
    for line in build.stdout.splitlines():
        item = json.loads(line)
        if item.get("reason") == "compiler-artifact" and item.get("executable"):
            result[item["target"]["name"]] = item["executable"]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True, help="unique result filename stem")
    parser.add_argument("--samples", type=int, default=31)
    parser.add_argument("--filter", help="latency fixture name substring")
    parser.add_argument("--allocations", action="store_true", help="also measure each case in a fresh DHAT process")
    parser.add_argument("--profile-case", help="collect a DHAT call-stack profile for one stream case")
    parser.add_argument("--source-ref", help="benchmark source from a Git revision in an isolated temporary build")
    args = parser.parse_args()
    if args.samples < 5:
        parser.error("at least five samples are required")
    if Path(args.label).name != args.label:
        parser.error("label must be a simple filename stem")
    if args.source_ref:
        revision = command("git", "rev-parse", "--verify", f"{args.source_ref}^{{commit}}")
        with tempfile.TemporaryDirectory(prefix="bifrost-bench-") as directory:
            build_root = Path(directory)
            for relative in command("git", "ls-tree", "-r", "--name-only", revision, "src").splitlines():
                path = build_root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                content = subprocess.check_output(["git", "show", f"{revision}:{relative}"], cwd=ROOT)
                path.write_bytes(content)
            for name in ("Cargo.toml", "Cargo.lock"):
                shutil.copy2(ROOT / name, build_root / name)
            shutil.copytree(ROOT / "benches", build_root / "benches")
            benchmark(args, build_root, revision)
    else:
        benchmark(args, ROOT, command("git", "rev-parse", "HEAD"))


def benchmark(args, build_root, revision):
    os.chdir(ROOT)
    output = ROOT / "docs/benchmark-results"
    output.mkdir(exist_ok=True)
    path = output / f"{args.label}.json"
    if path.exists():
        raise SystemExit(f"refusing to overwrite {path}; choose a new label")
    binaries = executables(build_root)
    metadata = {
        "command": sys.argv,
        "utc": datetime.now(timezone.utc).isoformat(),
        "harness_sha256": hashlib.sha256((ROOT / "benches/translator.rs").read_bytes()).hexdigest(),
        "revision": revision,
        "streaming_sha256": hashlib.sha256((build_root / "src/streaming.rs").read_bytes()).hexdigest(),
        "fixture_sha256": hashlib.sha256((ROOT / "benches/support/mod.rs").read_bytes()).hexdigest(),
        "rustc": command("rustc", "--version"),
        "cargo": command("cargo", "--version"),
        "os": platform.platform(),
        "cpu_count": os.cpu_count(),
        "flags": {name: os.environ.get(name, "") for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
    }
    if sys.platform == "darwin":
        metadata["cpu"] = command("sysctl", "-n", "machdep.cpu.brand_string")
        metadata["ram_bytes"] = int(command("sysctl", "-n", "hw.memsize"))
        metadata["os_version"] = command("sw_vers")
    cmd = [binaries["translator"], "--samples", str(args.samples)]
    if args.filter:
        cmd.extend(["--filter", args.filter])
    data = json.loads(command(*cmd))
    data["environment"] = metadata
    allocations = []
    if args.allocations:
        names = list(dict.fromkeys(row["case"] for row in data["rows"] if row["method"] == "stream_e2e"))
        for name in names:
            for method in ("value_e2e", "stream_e2e"):
                print(f"allocations {name} {method}", file=sys.stderr, flush=True)
                allocations.append(json.loads(command(binaries["allocations"], "--case", name, "--method", method)))
    data["allocations"] = allocations
    if args.profile_case:
        profile = ROOT / "target" / f"{args.label}-heap.json"
        subprocess.run([binaries["allocations"], "--case", args.profile_case, "--method", "stream_e2e",
                        "--heap-profile", str(profile)], cwd=ROOT, check=True)
        heap = json.loads(profile.read_text())
        points = sorted(heap["pps"], key=lambda point: point["tb"], reverse=True)
        data["heap_profile"] = {
            "case": args.profile_case,
            "points": [{"bytes": point["tb"], "blocks": point["tbk"],
                        "stack": [heap["ftbl"][frame] for frame in point["fs"]]}
                       for point in points[:12]],
        }
    path.write_text(json.dumps(data, indent=2) + "\n")
    print(path.relative_to(ROOT))


if __name__ == "__main__":
    main()
