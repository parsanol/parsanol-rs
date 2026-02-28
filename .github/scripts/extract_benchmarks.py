#!/usr/bin/env python3
"""
Extract benchmark results from cargo bench output.
Generates a JSON file with timing data for the dashboard.
"""

import re
import json
import sys
from pathlib import Path

def extract_benchmarks(input_file: str, output_file: str) -> None:
    """Extract benchmark results from cargo bench output."""
    with open(input_file, 'r') as f:
        output = f.read()

    # Parse benchmark results
    # Format: "test bench_name ... bench: 1,234 ns/iter (+/- 56)"
    pattern = r'test\s+(\S+)\s+.*bench:\s+([0-9,]+)\s+ns/iter\s+\(\+/-\s+([0-9,]+)\)'
    matches = re.findall(pattern, output)

    results = []
    for name, time_ns, variance in matches:
        time = int(time_ns.replace(',', ''))
        var = int(variance.replace(',', ''))
        results.append({
            'name': name,
            'time_ns': time,
            'variance_ns': var,
            'time_us': time / 1000,
            'time_ms': time / 1_000_000,
        })

    # Sort by time
    results.sort(key=lambda x: x['time_ns'])

    # Save as JSON
    Path(output_file).parent.mkdir(parents=True, exist_ok=True)
    with open(output_file, 'w') as f:
        json.dump(results, f, indent=2)

    print(f"Extracted {len(results)} benchmark results")
    for r in results:
        print(f"  {r['name']}: {r['time_ns']} ns")

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print("Usage: extract_benchmarks.py <input_file> <output_file>")
        sys.exit(1)

    extract_benchmarks(sys.argv[1], sys.argv[2])
