#!/usr/bin/env python3
"""
Automated Verification Harness for Headless Blargg Test Runner
"""
import os
import sys
import subprocess

def run_headless_test(headless_bin, rom_path):
    cmd = [
        headless_bin,
        "--rom", rom_path,
        "--test"
    ]
    print(f"Running: {' '.join(cmd)}")
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        return res
    except subprocess.TimeoutExpired:
        print(f"[ERROR] Headless execution timed out after 10 seconds")
        return None

def main():
    headless_bin = "./target/debug/headless"
    
    if not os.path.exists(headless_bin):
        print(f"[ERROR] Headless binary not found at {headless_bin}. Please compile it first.")
        sys.exit(1)
        
    # Ensure mock ROMs are generated
    # We can just run the generator script
    generator_script = "tests/generate_mock_roms.py"
    if os.path.exists(generator_script):
        print("Generating mock ROMs...")
        subprocess.run([sys.executable, generator_script], check=True)
    else:
        print(f"[ERROR] Generator script not found at {generator_script}")
        sys.exit(1)
        
    tests = [
        {
            "rom": "tests/roms/blargg_mock_pass.nes",
            "expected_exit": 0,
            "expected_stdout": "Test PASSED!",
            "expected_stderr": ""
        },
        {
            "rom": "tests/roms/blargg_mock_fail.nes",
            "expected_exit": 1,
            "expected_stdout": "",
            "expected_stderr": "Diagnostics:\nFail"
        },
        {
            "rom": "tests/roms/blargg_mock_reset.nes",
            "expected_exit": 0,
            "expected_stdout": "Test PASSED!",
            "expected_stderr": ""
        }
    ]
    
    print("==================================================")
    print("Verifying Headless Blargg Test Runner")
    print("==================================================")
    
    success = True
    for t in tests:
        rom = t["rom"]
        print(f"\n--- Testing ROM: {rom} ---")
        res = run_headless_test(headless_bin, rom)
        
        if res is None:
            print("FAIL: Execution failed or timed out")
            success = False
            continue
            
        print(f"Exit code: {res.returncode}")
        print(f"Stdout:\n{res.stdout.strip()}")
        if res.stderr:
            print(f"Stderr:\n{res.stderr.strip()}")
            
        # Verify exit code
        if res.returncode != t["expected_exit"]:
            print(f"FAIL: Exit code mismatch. Expected {t['expected_exit']}, got {res.returncode}")
            success = False
            continue
            
        # Verify expected output in stdout
        if t["expected_stdout"] and t["expected_stdout"] not in res.stdout:
            print(f"FAIL: Expected stdout '{t['expected_stdout']}' not found in output")
            success = False
            continue
            
        # Verify expected output in stderr
        if t["expected_stderr"] and t["expected_stderr"] not in res.stderr:
            print(f"FAIL: Expected stderr '{t['expected_stderr']}' not found in output")
            success = False
            continue
            
        print("PASS")
        
    print("\n==================================================")
    if success:
        print("ALL BLARGG RUNNER VERIFICATION CHECKS PASSED!")
        sys.exit(0)
    else:
        print("SOME BLARGG RUNNER VERIFICATION CHECKS FAILED!")
        sys.exit(1)

if __name__ == "__main__":
    main()
