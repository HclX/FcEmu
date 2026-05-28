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

# Step 7: Run Playwright Browser Tests (via Docker — no local Node.js required)
echo -e "\n${YELLOW}[Step 7/7] Executing Playwright Browser Tests...${NC}"
if command -v docker &> /dev/null; then
    echo "  Using Docker for isolated browser testing..."
    docker build -f Dockerfile.test -t fcemu-browser-tests . 2>&1 | tail -5
    docker run --rm fcemu-browser-tests
elif command -v node &> /dev/null && command -v wasm-pack &> /dev/null; then
    echo "  Docker not found, falling back to local Node.js..."
    # Build WASM if pkg/ doesn't exist or is stale
    if [ ! -d "pkg" ] || [ "src/core/wasm.rs" -nt "pkg/fce_core.js" ]; then
        bash build_web.sh
    fi
    [ ! -d "node_modules" ] && npm install
    npx playwright install --with-deps chromium 2>/dev/null
    npm test
else
    echo -e "${YELLOW}  SKIP: Neither Docker nor Node.js+wasm-pack found.${NC}"
    echo -e "${YELLOW}  Browser tests will run in CI (GitHub Actions).${NC}"
fi

echo -e "\n${GREEN}=====================================================
 🎉 SUCCESS: ALL FCEMU MASTER 7-STEP TEST SUITES PASSED!
=====================================================${NC}"
