#!/usr/bin/env python3
"""
Automated Golden Image Verification Harness for Flappy Bird PAL Mode (FcEmu Phase 10)
"""
import os
import sys
import re
import subprocess

# Frame 451 PAL Golden Checkpoint (Active Gameplay)
GOLDEN_MD5 = "221f40d4af4448bbcc1bc39dd3b8e87c"

# Input sequence to trigger gameplay and reach frame 451 in a realistic state
GAMEPLAY_INPUTS = "420-427:0x8,438-444:0x1,448-453:0x1,457-463:0x1,468-473:0x1,479-483:0x1,501-508:0x1,512-517:0x1,522-527:0x1,536-541:0x1,545-548:0x1,561-566:0x1,570-576:0x1,581-587:0x1,597-602:0x1,607-611:0x1,620-625:0x1,632-635:0x1,641-646:0x1,656-661:0x1,667-671:0x1,697-704:0x1,708-715:0x1,723-729:0x1,733-739:0x1,744-748:0x1,773-779:0x1,783-788:0x1,794-800:0x1,823-830:0x1,835-842:0x1,868-874:0x1,878-885:0x1"

def run_headless(headless_bin, rom_path, frames):
    cmd = [
        headless_bin,
        "--rom", rom_path,
        "--frames", str(frames),
        "--region", "pal",
        "--inputs", GAMEPLAY_INPUTS,
        "--checksum"
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
    rom_path = "static/public/roms/flappy-bird.nes"
    
    if not os.path.exists(headless_bin):
        print(f"[ERROR] Headless binary not found at {headless_bin}. Please compile it first using `cargo build --bin headless`.")
        sys.exit(1)
        
    if not os.path.exists(rom_path):
        print(f"[ERROR] Flappy Bird ROM not found at {rom_path}.")
        sys.exit(1)
        
    print("==================================================")
    print("Automated Golden Image Verification Harness - Flappy Bird PAL")
    print("==================================================")
    
    print(f"\n--- Verifying checkpoint at frame 451 ---")
    observed_md5 = run_headless(headless_bin, rom_path, 451)
    
    if observed_md5 is None:
        print(f"FAIL: Could not run or parse output for frame 451")
        sys.exit(1)
        
    if observed_md5 == GOLDEN_MD5:
        print(f"PASS: Frame 451 MD5 is {observed_md5} (matches golden reference)")
        sys.exit(0)
    else:
        print(f"FAIL: Frame 451 MD5 mismatch!")
        print(f"  Expected (Golden): {GOLDEN_MD5}")
        print(f"  Observed (Actual): {observed_md5}")
        sys.exit(1)

if __name__ == "__main__":
    main()
