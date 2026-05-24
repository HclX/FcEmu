#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status,
# if an undefined variable is referenced, or if any pipeline fails.
set -euo pipefail

# Determine the directory of this script to handle relative paths robustly
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "====================================================="
echo "      FcEmu No-Build WASM Compilation Script         "
echo "====================================================="

# Step 1: Compile the WASM dynamic library using wasm-pack directly into static/pkg
echo "Step 1: Checking wasm-pack..."
if ! command -v wasm-pack &> /dev/null; then
  echo "Error: wasm-pack is not installed." >&2
  echo "Please install it from: https://rustwasm.github.io/wasm-pack/installer/" >&2
  exit 1
fi

echo "Step 2: Cleaning old WASM package (static/pkg)..."
rm -rf static/pkg/

echo "Step 3: Compiling WASM core directly to static/pkg..."
wasm-pack build --target web --out-dir static/pkg

# Step 4: Cleanup legacy directories if they exist (migration helper)
if [ -d "dist" ] || [ -d "pkg" ]; then
  echo "Step 4: Cleaning up legacy build directories (dist/ and pkg/)..."
  rm -rf dist/ pkg/
fi

echo "====================================================="
echo " 🎉 FcEmu WASM Compilation Completed successfully!"
echo " Web assets are ready in: ./static"
echo " You can now run: python3 server.py"
echo "====================================================="
