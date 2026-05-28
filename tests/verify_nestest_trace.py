#!/usr/bin/env python3
"""
Nestest CPU Trace Verification for FcEmu.

Runs the nestest ROM in headless mode and compares the first 9000 lines
of CPU trace output against the known-good reference log. Verifies:
PC, A, X, Y, P (flags), SP, and cycle count for each instruction.
"""
import os
import re
import sys
import subprocess


def parse_trace_line(line):
    """Parse a CPU trace line into register values."""
    pc = line[:4].strip()

    a_match = re.search(r'A:([0-9A-F]{2})', line)
    x_match = re.search(r'X:([0-9A-F]{2})', line)
    y_match = re.search(r'Y:([0-9A-F]{2})', line)
    p_match = re.search(r'P:([0-9A-F]{2})', line)
    sp_match = re.search(r'SP:([0-9A-F]{2})', line)
    cyc_match = re.search(r'CYC:(\d+)', line)

    return {
        "PC": pc.upper() if pc else "",
        "A": a_match.group(1).upper() if a_match else "",
        "X": x_match.group(1).upper() if x_match else "",
        "Y": y_match.group(1).upper() if y_match else "",
        "P": p_match.group(1).upper() if p_match else "",
        "SP": sp_match.group(1).upper() if sp_match else "",
        "CYC": cyc_match.group(1) if cyc_match else ""
    }


def main():
    headless_bin = sys.argv[1] if len(sys.argv) > 1 else "./target/debug/headless"
    rom_path = "tests/roms/nestest.nes"
    ref_path = "tests/references/nestest.log"
    log_path = "nestest.log"
    lines_to_check = 9000

    print("=" * 60)
    print("Nestest CPU Trace Verification")
    print("=" * 60)

    # Verify prerequisites
    if not os.path.exists(headless_bin):
        print(f"ERROR: Headless binary not found at {headless_bin}")
        sys.exit(1)
    if not os.path.exists(rom_path):
        print(f"ERROR: nestest.nes ROM not found at {rom_path}")
        print("  Run: bash tests/download_test_roms.sh")
        sys.exit(1)
    if not os.path.exists(ref_path):
        print(f"ERROR: Reference log not found at {ref_path}")
        sys.exit(1)

    # Run headless emulator with trace logging
    print(f"Running: {headless_bin} --rom {rom_path} --log {log_path}")
    try:
        res = subprocess.run(
            [headless_bin, "--rom", rom_path, "--log", log_path],
            capture_output=True, text=True, timeout=60
        )
        if res.returncode != 0:
            print(f"ERROR: Headless execution failed (exit code {res.returncode})")
            if res.stderr:
                print(f"  stderr: {res.stderr[:500]}")
            sys.exit(1)
    except subprocess.TimeoutExpired:
        print("ERROR: Headless execution timed out (60s)")
        sys.exit(1)

    if not os.path.exists(log_path):
        print(f"ERROR: Generated trace log not found at {log_path}")
        sys.exit(1)

    # Compare trace output line-by-line
    print(f"Comparing first {lines_to_check} trace lines against reference...")
    with open(log_path, "r") as f_gen, open(ref_path, "r") as f_ref:
        lines_gen = f_gen.readlines()
        lines_ref = f_ref.readlines()

    limit = min(lines_to_check, len(lines_gen), len(lines_ref))
    if limit == 0:
        print("ERROR: One of the log files is empty.")
        sys.exit(1)

    if len(lines_gen) < lines_to_check:
        print(f"WARNING: Generated log has only {len(lines_gen)} lines (expected {lines_to_check})")

    for i in range(limit):
        gen_state = parse_trace_line(lines_gen[i].strip())
        ref_state = parse_trace_line(lines_ref[i].strip())

        for reg in ["PC", "A", "X", "Y", "P", "SP", "CYC"]:
            if gen_state[reg] != ref_state[reg]:
                print(f"MISMATCH at line {i+1}, register {reg}:")
                print(f"  Generated: {lines_gen[i].strip()}")
                print(f"    Parsed:  {gen_state}")
                print(f"  Reference: {lines_ref[i].strip()}")
                print(f"    Parsed:  {ref_state}")
                sys.exit(1)

    # Cleanup generated log
    os.remove(log_path)

    print(f"PASSED: {limit} trace lines match perfectly.")
    print("=" * 60)


if __name__ == "__main__":
    main()
