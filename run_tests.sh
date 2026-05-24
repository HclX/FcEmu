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

echo -e "${YELLOW}====================================================="
echo "          FcEmu Master Test Suite Execution"
echo -e "=====================================================${NC}"

# Step 1: Run Cargo unit tests
echo -e "\n${YELLOW}[Step 1/5] Running Rust Core Unit Tests...${NC}"
cargo test

# Step 2: Ensure headless CLI test binary is compiled
echo -e "\n${YELLOW}[Step 2/5] Compiling Headless CLI Test Binary...${NC}"
cargo build --bin headless

# Step 3: Run Blargg Test Suite Verification Harness
echo -e "\n${YELLOW}[Step 3/5] Executing Blargg Automated Test Suite...${NC}"

# Auto-download check for missing official blargg and stress test ROMs
if [ ! -f "tests/roms/instr_official_only.nes" ] || [ ! -f "tests/roms/cpu_dummy_writes.nes" ] || [ ! -f "tests/roms/branch_timing.nes" ] || [ ! -f "tests/roms/nestress.nes" ]; then
    echo -e "${YELLOW}Official test ROM files are missing. Running tests/download_test_roms.sh automatically...${NC}"
    chmod +x tests/download_test_roms.sh
    ./tests/download_test_roms.sh
fi

python3 tests/verify_blargg_runner.py

# Step 4: Run Nova the Squirrel Visual Golden MD5 Verification Harness
echo -e "\n${YELLOW}[Step 4/5] Executing Nova the Squirrel Visual Golden Harness...${NC}"
python3 tests/verify_squirrel.py

# Step 5: Run NEStress Visual Golden MD5 Verification Harness
echo -e "\n${YELLOW}[Step 5/5] Executing NEStress Visual Golden Harness...${NC}"
python3 tests/verify_nestress.py

echo -e "\n${GREEN}====================================================="
echo " 🎉 SUCCESS: ALL FCEMU MASTER TEST SUITES PASSED!"
echo -e "=====================================================${NC}"
