#!/usr/bin/env python3
"""
Unified Golden Image Verification Harness for FcEmu.

Runs all golden image tests in sequence and reports pass/fail per test.
Exits with nonzero if any test fails.
"""
import os
import sys
import re
import subprocess

# Input sequence for Flappy Bird PAL active gameplay reaching frame 451
GAMEPLAY_INPUTS = "420-427:0x8,438-444:0x1,448-453:0x1,457-463:0x1,468-473:0x1,479-483:0x1,501-508:0x1,512-517:0x1,522-527:0x1,536-541:0x1,545-548:0x1,561-566:0x1,570-576:0x1,581-587:0x1,597-602:0x1,607-611:0x1,620-625:0x1,632-635:0x1,641-646:0x1,656-661:0x1,667-671:0x1,697-704:0x1,708-715:0x1,723-729:0x1,733-739:0x1,744-748:0x1,773-779:0x1,783-788:0x1,794-800:0x1,823-830:0x1,835-842:0x1,868-874:0x1,878-885:0x1"

GOLDEN_TESTS = [
    {
        "name": "Nova the Squirrel",
        "rom": "static/public/roms/novathesquirrel.nes",
        "frames": {
            60: "43b732574ec3088c4dcbfcc5e52aa9ae",
            180: "43b732574ec3088c4dcbfcc5e52aa9ae",
            240: "43b732574ec3088c4dcbfcc5e52aa9ae",
            300: "43b732574ec3088c4dcbfcc5e52aa9ae",
        },
    },
    {
        "name": "NEStress",
        "rom": "tests/roms/nestress.nes",
        "frames": {
            300: "305a15fc90492390a0f9a0f5ad9de10b",
        },
    },
    {
        "name": "Flappy PAL",
        "rom": "static/public/roms/flappy-bird.nes",
        "frames": {
            451: "b753c77137c297b2a2a0c6b653df3326",
        },
        "region": "pal",
        "inputs": GAMEPLAY_INPUTS,
    },
]


def run_headless(headless_bin, rom_path, frames, region=None, inputs=None):
    """Run the headless emulator and return the MD5 checksum of the final frame."""
    cmd = [
        headless_bin,
        "--rom", rom_path,
        "--frames", str(frames),
        "--checksum",
    ]
    if region:
        cmd.extend(["--region", region])
    if inputs:
        cmd.extend(["--inputs", inputs])

    print(f"Running: {' '.join(cmd)}")
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=45)
        if res.returncode != 0:
            print(f"[ERROR] Headless execution failed (exit code {res.returncode})")
            print(f"Stdout: {res.stdout}")
            print(f"Stderr: {res.stderr}")
            return None

        match = re.search(r"Frame MD5:\s*([a-fA-F0-9]{32})", res.stdout)
        if not match:
            print(f"[ERROR] MD5 checksum not found in output.")
            print(f"Stdout: {res.stdout}")
            return None

        return match.group(1).lower()
    except subprocess.TimeoutExpired:
        print(f"[ERROR] Headless execution timed out after 45 seconds")
        return None


def run_test(headless_bin, test_config):
    """Run a single golden image test. Returns True if all checkpoints pass."""
    name = test_config["name"]
    rom_path = test_config["rom"]
    region = test_config.get("region")
    inputs = test_config.get("inputs")

    print(f"\n{'=' * 50}")
    print(f"Golden Image Verification - {name}")
    print(f"{'=' * 50}")

    if not os.path.exists(rom_path):
        print(f"[ERROR] ROM not found at {rom_path}.")
        return False

    success = True
    for frames, expected_md5 in sorted(test_config["frames"].items()):
        print(f"\n--- Verifying checkpoint at frame {frames} ---")
        observed_md5 = run_headless(headless_bin, rom_path, frames, region, inputs)

        if observed_md5 is None:
            print(f"FAIL: Could not run or parse output for frame {frames}")
            success = False
        elif observed_md5 == expected_md5:
            print(f"PASS: Frame {frames} MD5 is {observed_md5} (matches golden reference)")
        else:
            print(f"FAIL: Frame {frames} MD5 mismatch!")
            print(f"  Expected (Golden): {expected_md5}")
            print(f"  Observed (Actual): {observed_md5}")
            success = False

    return success


def main():
    headless_bin = "./target/debug/headless"

    if not os.path.exists(headless_bin):
        print(f"[ERROR] Headless binary not found at {headless_bin}. "
              f"Please compile it first using `cargo build --bin headless`.")
        sys.exit(1)

    print("==================================================")
    print("Unified Golden Image Verification Harness")
    print("==================================================")

    all_passed = True
    results = []

    for test_config in GOLDEN_TESTS:
        passed = run_test(headless_bin, test_config)
        results.append((test_config["name"], passed))
        if not passed:
            all_passed = False

    print(f"\n{'=' * 50}")
    print("Summary:")
    for name, passed in results:
        status = "PASS" if passed else "FAIL"
        print(f"  [{status}] {name}")
    print(f"{'=' * 50}")

    if all_passed:
        print("ALL GOLDEN IMAGE VERIFICATION CHECKS PASSED!")
        sys.exit(0)
    else:
        print("SOME GOLDEN IMAGE VERIFICATION CHECKS FAILED!")
        sys.exit(1)


if __name__ == "__main__":
    main()
