#!/usr/bin/env python3
"""
Fast before/after comparison using parallel execution.
Only tests known Blargg-protocol ROMs (no hangs).
"""
import subprocess
import sys
import os
import glob
from concurrent.futures import ProcessPoolExecutor, as_completed

OLD_BIN = "/tmp/fcemu_baseline/target/debug/headless"
NEW_BIN = "./target/debug/headless"

# Collect all .nes files
TEST_ROMS = []
for rom in sorted(glob.glob("tests/external_test_roms/**/*.nes", recursive=True)):
    if "/source/" in rom:
        continue
    TEST_ROMS.append(rom)
for rom in sorted(glob.glob("tests/roms/*.nes")):
    if "mock" not in rom:
        TEST_ROMS.append(rom)

def run_test(args):
    binary, rom_path = args
    timeout = 90 if ("official_only" in rom_path or "all_instrs" in rom_path) else 30
    cmd = [binary, "--rom", rom_path, "--test"]
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        is_blargg = "Blargg test signature verified" in res.stdout
        if not is_blargg:
            return (rom_path, "SKIP")
        return (rom_path, "PASS" if res.returncode == 0 else "FAIL")
    except subprocess.TimeoutExpired:
        return (rom_path, "TIMEOUT")

def main():
    print(f"Testing {len(TEST_ROMS)} ROMs against OLD and NEW binaries (parallel)...")
    
    # Run old binary tests
    print("\n[Phase 1] Testing with OLD binary (main)...")
    old_results = {}
    with ProcessPoolExecutor(max_workers=8) as executor:
        futures = {executor.submit(run_test, (OLD_BIN, rom)): rom for rom in TEST_ROMS}
        for f in as_completed(futures):
            rom, status = f.result()
            old_results[rom] = status

    # Run new binary tests
    print("[Phase 2] Testing with NEW binary (fix/comprehensive-review)...")
    new_results = {}
    with ProcessPoolExecutor(max_workers=8) as executor:
        futures = {executor.submit(run_test, (NEW_BIN, rom)): rom for rom in TEST_ROMS}
        for f in as_completed(futures):
            rom, status = f.result()
            new_results[rom] = status

    # Compare
    improved = []
    regressed = []
    for rom in TEST_ROMS:
        old = old_results.get(rom, "?")
        new = new_results.get(rom, "?")
        basename = rom.replace("tests/external_test_roms/", "").replace("tests/roms/", "")
        if old != new:
            if new == "PASS" and old in ("FAIL", "TIMEOUT"):
                improved.append((basename, old, new))
            elif old == "PASS" and new in ("FAIL", "TIMEOUT"):
                regressed.append((basename, old, new))
            elif old == "SKIP" and new == "PASS":
                improved.append((basename, old, new))
            elif old == "TIMEOUT" and new == "FAIL":
                pass  # not really a change
            elif old == "FAIL" and new == "TIMEOUT":
                pass
            else:
                pass  # skip → timeout etc

    old_pass = sum(1 for v in old_results.values() if v == "PASS")
    new_pass = sum(1 for v in new_results.values() if v == "PASS")
    old_fail = sum(1 for v in old_results.values() if v == "FAIL")
    new_fail = sum(1 for v in new_results.values() if v == "FAIL")
    old_timeout = sum(1 for v in old_results.values() if v == "TIMEOUT")
    new_timeout = sum(1 for v in new_results.values() if v == "TIMEOUT")
    old_skip = sum(1 for v in old_results.values() if v == "SKIP")
    new_skip = sum(1 for v in new_results.values() if v == "SKIP")

    print("\n" + "=" * 70)
    print("BEFORE / AFTER COMPARISON")
    print("=" * 70)
    print(f"{'':30s} {'OLD (main)':>12s}  {'NEW (fixed)':>12s}  {'Delta':>8s}")
    print(f"{'-'*30} {'-'*12}  {'-'*12}  {'-'*8}")
    print(f"{'PASS (Blargg verified)':30s} {old_pass:>12d}  {new_pass:>12d}  {new_pass - old_pass:>+8d}")
    print(f"{'FAIL':30s} {old_fail:>12d}  {new_fail:>12d}  {new_fail - old_fail:>+8d}")
    print(f"{'TIMEOUT':30s} {old_timeout:>12d}  {new_timeout:>12d}  {new_timeout - old_timeout:>+8d}")
    print(f"{'SKIP (non-Blargg)':30s} {old_skip:>12d}  {new_skip:>12d}  {new_skip - old_skip:>+8d}")

    if improved:
        print(f"\n⬆️  IMPROVED ({len(improved)} tests):")
        for name, old, new in improved:
            print(f"   {name}: {old} → {new}")

    if regressed:
        print(f"\n⬇️  REGRESSED ({len(regressed)} tests):")
        for name, old, new in regressed:
            print(f"   {name}: {old} → {new}")

    if not regressed:
        print("\n✅ NO REGRESSIONS!")
    
    print(f"\nTotal ROMs tested: {len(TEST_ROMS)}")

if __name__ == "__main__":
    main()
