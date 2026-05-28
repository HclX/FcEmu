#!/bin/bash
# =====================================================
#          FcEmu Emulator - Master Test Runner
# =====================================================
set -e

# Define text colors for gorgeous summaries!
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=====================================================
          FcEmu Master Test Suite Execution
=====================================================${NC}"

# Step 1: Run Cargo unit tests
echo -e "\n${YELLOW}[Step 1/6] Running Rust Core Unit Tests...${NC}"
cargo test

# Step 2: Ensure headless CLI test binary is compiled
echo -e "\n${YELLOW}[Step 2/6] Compiling Headless CLI Test Binary...${NC}"
cargo build --bin headless

# Step 3: Run Blargg Test Suite Verification Harness
echo -e "\n${YELLOW}[Step 3/6] Executing Blargg Automated Test Suite...${NC}"

# Auto-download check for missing official blargg and stress test ROMs
if [ ! -f "tests/roms/instr_official_only.nes" ] || [ ! -f "tests/roms/cpu_dummy_writes.nes" ] || [ ! -f "tests/roms/branch_timing.nes" ] || [ ! -f "tests/roms/nestress.nes" ]; then
    echo -e "${YELLOW}Official test ROM files are missing. Running tests/download_test_roms.sh automatically...${NC}"
    chmod +x tests/download_test_roms.sh
    ./tests/download_test_roms.sh
fi

python3 tests/verify_blargg_runner.py

# Step 4: Run Unified Golden Image Verification Harness
echo -e "\n${YELLOW}[Step 4/6] Executing Unified Golden Image Verification Harness...${NC}"
python3 tests/verify_golden_images.py

# Step 5: Run E2E Test Runner Harness
echo -e "\n${YELLOW}[Step 5/6] Executing E2E Test Runner Harness...${NC}"
python3 tests/e2e_runner.py

# Step 6: Run Parallel Compatibility Explorer (Unified Blargg + Checksum PAL APU)
echo -e "\n${YELLOW}[Step 6/6] Executing Parallel Compatibility Explorer...${NC}"
python3 tests/run_all_external_tests.py

echo -e "\n${GREEN}=====================================================
 🎉 SUCCESS: ALL FCEMU MASTER 6-STEP TEST SUITES PASSED!
=====================================================${NC}"
