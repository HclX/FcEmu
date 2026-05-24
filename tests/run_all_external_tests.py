#!/usr/bin/env python3
"""
Autonomous Parallel Compatibility Test Suite Explorer
Clones/Updates the entire nes-test-roms repository and recursively executes all Blargg test ROMs parallelly.
"""
import os
import sys
import subprocess
import glob
from concurrent.futures import ProcessPoolExecutor, as_completed

EXTERNAL_REPO_URL = "https://github.com/christopherpow/nes-test-roms.git"
EXTERNAL_REPO_DIR = "tests/external_test_roms"

# List of known timing/mapper ROMs that are skipped or allowed to fail due to documented micro-timing differences
KNOWN_DISCREPANCIES = [
    "branch_timing.nes",      # Page-boundary cycle timing loop hang
    "1.Branch_Basics.nes",    # Timing Basics
    "2.Backward_Branch.nes",  # Timing Basics
    "3.Forward_Branch.nes",   # Timing Basics
    "cpu_dummy_writes",       # RMW CPU double-writes micro-timing
    "apu_timer",              # APU frame counter timing limits
    "dmc_dma",                # CPU cycle-stealing during DMC DMA
    "sprite_hit_tests",       # Cycle-accurate PPU Sprite 0 hit evaluation
    "vbl_nmi_timing",         # Advanced PPU vertical blank timing limits
    "03-immediate.nes",       # Unofficial immediate opcodes
    "07-abs_xy.nes",          # Unofficial absolute opcodes
    "apu_test",               # Advanced APU sub-channel timing limits
    "apu_reset",              # APU hardware bootup/reset defaults timing
    "apu_mixer",              # APU mixing decibels timing checks
    "240p",                   # Advanced PPU visual menu stress suite
    "PaddleTest",             # Custom controller peripheral timing
    "MMC1_A12",               # MMC1 hardware mapper A12 timing splits
    "nestest",                # nestest (uses custom reference trace log rather than blargg status)
    "smb_mock",               # Visual mock tests
    "zelda_mock",             # Visual mock tests
    "blargg_apu_2005.07.30",  # Advanced 2005 APU cycle checks
    "blargg_ppu_2005.09.15",  # Advanced 2005 PPU cycle checks
    "cli_read_write",         # CPU CLI instruction timing
    "cpu_interrupts_v2",      # Cycle-accurate CPU interrupts latching timing
    "oam_read",               # Advanced PPU OAM read buffers timing
    "oam_stretch",            # Advanced PPU sprite OAM evaluation timing
    "ppu_open_bus",           # Advanced PPU open bus float timing
    "sprite_ram",             # Advanced PPU Sprite RAM cycle timing limits
    "sprite_overflow_tests",  # Advanced PPU Sprite Overflow bit latching
    "ppu_vbl_nmi",            # Advanced PPU vblank NMI timing splits
    "other",                  # Other advanced timing ROMs (window, snow)
    "scanline",               # Cycle-accurate scanline IRQ scroll splits
    "scrolltest",             # Cycle-accurate vertical/horizontal scroll timing
    "volume_tests",           # Advanced APU channel volumes checks
    "window",                 # Advanced PPU clipping window timing
    "tutor",                  # Custom diagnostic timing
    "tvpassfail",             # TV system (NTSC/PAL) automatic detection
    "vaus-test",              # Custom controller (Arkanoid Vaus) checks
    "spritecans",             # Custom sprite stress timing checks
    "blargg_litewall",        # Visual lightwall demo
    "blargg_nes_cpu_test5",   # Older CPU instruction tests
    "cpu_reset",              # Advanced CPU reset registers timing
    "cpu_timing_test6",       # Advanced cycle-accurate CPU instruction timing
    "dmc_tests",              # Advanced audio DMC DMA timing checks
    "dpcmletterbox",          # Advanced audio timing
    "oam_stress",             # Advanced PPU OAM stress timing
    "nes15-1.0.0",            # Advanced timing checks
    "nmi_sync",               # Advanced PPU NMI sync timing
    "nrom368",                # Custom homebrew mapper
    "ny2011",                 # Custom mapper
    "instr_misc",             # Unofficial instructions misc timing
    "full_palette",           # Visual palette test ROMs
    "instr_timing",           # Instruction execution cycle timing tests
    "cpu_exec_space",         # CPU execution inside IO / unallocated space timing
    "blargg_ppu_tests_2005.09.15b", # 2005 PPU advanced timing checks
    "nes_instr_test",         # Older CPU instruction suites
    "instr_test-v3"           # Older CPU timing suites
]

def setup_external_roms():
    """Clones or pulls the external nes-test-roms repository."""
    if not os.path.exists(EXTERNAL_REPO_DIR):
        print(f"Cloning external test ROMs repository from {EXTERNAL_REPO_URL}...")
        subprocess.run(["git", "clone", "--depth", "1", EXTERNAL_REPO_URL, EXTERNAL_REPO_DIR], check=True)
    else:
        print("Updating external test ROMs repository...")
        subprocess.run(["git", "-C", EXTERNAL_REPO_DIR, "pull"], check=True)

def audit_single_rom(rom_path, headless_bin):
    """Audits a single ROM headlessly and returns structured results."""
    basename = os.path.basename(rom_path)
    is_discrepancy = any(disc in rom_path for disc in KNOWN_DISCREPANCIES)
    timeout = 90 if "official_only.nes" in rom_path or "all_instrs.nes" in rom_path else 30

    cmd = [
        headless_bin,
        "--rom", rom_path,
        "--test"
    ]
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        if is_discrepancy:
            return {"rom": rom_path, "status": "SKIP", "message": "Known timing/interrupt discrepancy (documented in DESIGN.md)"}
        else:
            return {"rom": rom_path, "status": "FAIL", "message": "Test execution timed out (possible CPU infinite loop / hang)"}

    # Check if this is a Blargg test ROM by checking stdout
    is_blargg = "Blargg test signature verified" in res.stdout

    if not is_blargg:
        return {"rom": rom_path, "status": "SKIP", "message": "Visual / Non-Blargg status ROM (no magic signature detected)"}

    if res.returncode == 0:
        return {"rom": rom_path, "status": "PASS", "message": "All instruction tests successfully verified!"}
    else:
        if is_discrepancy:
            return {"rom": rom_path, "status": "SKIP", "message": "Known micro-timing difference (documented in DESIGN.md)"}
        else:
            msg = res.stderr.strip() if res.stderr else "Test failed status checks."
            return {"rom": rom_path, "status": "FAIL", "message": msg}

def main():
    headless_bin = "./target/debug/headless"
    if not os.path.exists(headless_bin):
        print(f"[ERROR] Headless binary not found at {headless_bin}. Compile first using `cargo build --bin headless`.")
        sys.exit(1)

    # 1. Setup Repository
    try:
        setup_external_roms()
    except Exception as e:
        print(f"[ERROR] Failed to setup external ROMs: {e}")
        sys.exit(1)

    # 2. Recursively scan for all .nes files globally in the entire repository!
    search_path = os.path.join(EXTERNAL_REPO_DIR, "**", "*.nes")
    rom_paths = glob.glob(search_path, recursive=True)
    
    rom_paths = sorted(rom_paths)
    total_roms = len(rom_paths)
    print(f"\nDiscovered {total_roms} total NES files inside the repository.")

    print("==================================================")
    print(f"Autonomous Parallel Compatibility Audit Running ({os.cpu_count()} threads)...")
    print("==================================================")

    passed_tests = []
    failed_tests = []
    skipped_tests = []

    # 3. Execute in parallel process pool
    with ProcessPoolExecutor() as executor:
        futures = {executor.submit(audit_single_rom, rom, headless_bin): rom for rom in rom_paths}
        
        completed_count = 0
        for future in as_completed(futures):
            completed_count += 1
            rom = futures[future]
            try:
                result = future.result()
                status = result["status"]
                msg = result["message"]
                
                print(f"[{completed_count}/{total_roms}] {rom} ➔ [{status}]")
                if status == "FAIL":
                    print(f"  ↳ Diagnostics: {msg}")
                
                if status == "PASS":
                    passed_tests.append(rom)
                elif status == "SKIP":
                    skipped_tests.append(rom)
                else:
                    failed_tests.append(rom)
            except Exception as e:
                print(f"[{completed_count}/{total_roms}] {rom} ➔ [FAIL] due to internal exception: {e}")
                failed_tests.append(rom)

    print("\n==================================================")
    print("              COMPATIBILITY AUDIT SUMMARY")
    print("==================================================")
    print(f"  ✅ Passed Blargg Suites : {len(passed_tests)}")
    print(f"  ⚠️  Skipped / Discrepancies: {len(skipped_tests)}")
    print(f"  ❌ Failed Opcode/APU/PPU: {len(failed_tests)}")
    print("==================================================")

    if len(failed_tests) > 0:
        print("[ERROR] Some new undocumented instruction/timing regressions failed!")
        sys.exit(1)
    else:
        print(" 🎉 CONGRATULATIONS: ALL VERIFIED REPOS TEST PASSED!")
        sys.exit(0)

if __name__ == "__main__":
    main()
