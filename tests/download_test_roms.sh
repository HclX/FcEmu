#!/bin/bash
# Automated Diagnostic Test ROMs Downloader
set -e

mkdir -p tests/roms

echo "Downloading Blargg's CPU Instruction Tests (Official Only)..."
curl -fL -o tests/roms/instr_official_only.nes "https://github.com/christopherpow/nes-test-roms/raw/master/instr_test-v5/official_only.nes"

echo "Downloading Blargg's CPU Dummy Writes Test (PPU Mem)..."
curl -fL -o tests/roms/cpu_dummy_writes.nes "https://github.com/christopherpow/nes-test-roms/raw/master/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"

echo "Downloading Blargg's Branch Timing Test (Branch Basics)..."
curl -fL -o tests/roms/branch_timing.nes "https://github.com/christopherpow/nes-test-roms/raw/master/branch_timing_tests/1.Branch_Basics.nes"

echo "Downloading J.Y. Company's NEStress Test ROM..."
curl -fL -o tests/roms/nestress.nes "https://github.com/christopherpow/nes-test-roms/raw/master/stress/NEStress.NES"

echo "Download complete!"
