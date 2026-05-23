#!/usr/bin/env python3
"""
Automated Golden Image Verification Harness for Super Mario Bros. (FcEmu Phase 8)
"""
import os
import sys
import re
import subprocess

GOLDEN_CHECKPOINTS = {
    60: "2c647c745659fc29ffdf432cc0fd751b",
    180: "b86b3af0e08e15fad021f41eca68a617",
    240: "502a438cdb801e8697a43a2746129c62",
    300: "f8761835aba05541309814479d32e39c"
}

INPUT_SEQUENCE = "61:0x08,180-240:0x80,241-260:0x81,261-300:0x80"

def run_headless(headless_bin, rom_path, frames):
    cmd = [
        headless_bin,
        "--rom", rom_path,
        "--frames", str(frames),
        "--checksum",
        "--inputs", INPUT_SEQUENCE
    ]
    print(f"Running: {' '.join(cmd)}")
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=45)
        if res.returncode != 0:
            print(f"[ERROR] Headless execution failed (exit code {res.returncode})")
            print(f"Stdout: {res.stdout}")
            print(f"Stderr: {res.stderr}")
            return None
        
        # Extract MD5
        match = re.search(r"Frame MD5:\s*([a-fA-F0-9]{32})", res.stdout)
        if not match:
            print(f"[ERROR] MD5 checksum not found in output.")
            print(f"Stdout: {res.stdout}")
            return None
        
        return match.group(1).lower()
    except subprocess.TimeoutExpired:
        print(f"[ERROR] Headless execution timed out after 45 seconds")
        return None

def main():
    headless_bin = "./target/debug/headless"
    rom_path = "roms/super_mario_bro.nes"
    
    if not os.path.exists(headless_bin):
        print(f"[ERROR] Headless binary not found at {headless_bin}. Please compile it first using `cargo build --bin headless`.")
        sys.exit(1)
        
    if not os.path.exists(rom_path):
        print(f"[ERROR] Super Mario Bros. ROM not found at {rom_path}.")
        sys.exit(1)
        
    print("==================================================")
    print("Automated Golden Image Verification Harness")
    print("==================================================")
    
    success = True
    for frames, expected_md5 in sorted(GOLDEN_CHECKPOINTS.items()):
        print(f"\n--- Verifying checkpoint at frame {frames} ---")
        observed_md5 = run_headless(headless_bin, rom_path, frames)
        
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

    print("\n==================================================")
    if success:
        print("ALL GOLDEN IMAGE VERIFICATION CHECKS PASSED!")
        sys.exit(0)
    else:
        print("SOME GOLDEN IMAGE VERIFICATION CHECKS FAILED!")
        sys.exit(1)

if __name__ == "__main__":
    main()
