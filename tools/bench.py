#!/usr/bin/env python3
"""Measure binary size and headless export speed.

    python3 tools/bench.py <binary> [project.json] [runs]

Reports the binary size and the median / min wall time of
`--export <project> <tmp> --no-subdir` (all formats) and `--render <project> 1`.
"""
import os
import statistics
import subprocess
import sys
import tempfile
import time

binary = sys.argv[1]
project = sys.argv[2] if len(sys.argv) > 2 else "examples/sample_project.json"
runs = int(sys.argv[3]) if len(sys.argv) > 3 else 9


def timed(args):
    t = []
    for _ in range(runs):
        with tempfile.TemporaryDirectory() as d:
            a = [x.replace("{tmp}", d) for x in args]
            t0 = time.perf_counter()
            subprocess.run([binary, *a], check=True, stdout=subprocess.DEVNULL)
            t.append(time.perf_counter() - t0)
    return statistics.median(t), min(t)


size = os.path.getsize(binary)
ex = timed(["--export", project, "{tmp}", "--no-subdir"])
rd = timed(["--render", project, "1", "{tmp}/s.png"])
print(f"binary      {size:,} bytes ({size / 1048576:.2f} MiB)")
print(f"export all  median {ex[0] * 1000:.0f} ms  min {ex[1] * 1000:.0f} ms")
print(f"render 1    median {rd[0] * 1000:.0f} ms  min {rd[1] * 1000:.0f} ms")
