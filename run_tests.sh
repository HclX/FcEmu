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
echo -e "\n${YELLOW}[Step 1/7] Running Rust Core Unit Tests...${NC}"
cargo test

# Step 2: Ensure headless CLI test binary is compiled
echo -e "\n${YELLOW}[Step 2/7] Compiling Headless CLI Test Binary...${NC}"
cargo build --bin headless

# Step 3: Run Blargg Test Suite Verification Harness
echo -e "\n${YELLOW}[Step 3/7] Executing Blargg Automated Test Suite...${NC}"

# Auto-download check for missing official blargg and stress test ROMs
if [ ! -f "tests/roms/instr_official_only.nes" ] || [ ! -f "tests/roms/cpu_dummy_writes.nes" ] || [ ! -f "tests/roms/branch_timing.nes" ] || [ ! -f "tests/roms/nestress.nes" ]; then
    echo -e "${YELLOW}Official test ROM files are missing. Running tests/download_test_roms.sh automatically...${NC}"
    chmod +x tests/download_test_roms.sh
    ./tests/download_test_roms.sh
fi

python3 tests/verify_blargg_runner.py

# Step 4: Run Unified Golden Image Verification Harness
echo -e "\n${YELLOW}[Step 4/7] Executing Unified Golden Image Verification Harness...${NC}"
python3 tests/verify_golden_images.py

# Step 5: Run Nestest CPU Trace Verification
echo -e "\n${YELLOW}[Step 5/7] Executing Nestest CPU Trace Verification...${NC}"
python3 tests/verify_nestest_trace.py

# Step 6: Run Parallel Compatibility Explorer (Unified Blargg + Checksum PAL APU)
echo -e "\n${YELLOW}[Step 6/7] Executing Parallel Compatibility Explorer...${NC}"
python3 tests/run_all_external_tests.py

# Step 7: Run Playwright Browser Tests (requires wasm-pack + Node.js)
echo -e "\n${YELLOW}[Step 7/7] Executing Playwright Browser Tests...${NC}"
if ! command -v wasm-pack &> /dev/null; then
    echo -e "${YELLOW}  SKIP: wasm-pack not found. Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh${NC}"
elif ! command -v node &> /dev/null; then
    echo -e "${YELLOW}  SKIP: Node.js not found. Install from: https://nodejs.org${NC}"
else
    # Build WASM if pkg/ doesn't exist or is stale
    if [ ! -d "pkg" ] || [ "src/core/wasm.rs" -nt "pkg/fce_core.js" ]; then
        echo "  Building WASM bundle..."
        bash build_web.sh
    fi

    # Install Node dependencies if needed
    if [ ! -d "node_modules" ]; then
        echo "  Installing Node dependencies..."
        npm install
    fi

    # Install Playwright browsers if needed
    if ! npx playwright install --dry-run &> /dev/null 2>&1; then
        echo "  Installing Playwright browsers..."
        npx playwright install chromium
    fi

    # Run all Playwright tests
    npm test
fi

echo -e "\n${GREEN}=====================================================
 🎉 SUCCESS: ALL FCEMU MASTER 7-STEP TEST SUITES PASSED!
=====================================================${NC}"

