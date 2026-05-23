#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status,
# if an undefined variable is referenced, or if any pipeline fails.
set -euo pipefail

# Determine the directory of this script to handle relative paths robustly
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "====================================================="
echo "           FcEmu Static Web Build Script             "
echo "====================================================="

# Step 1: Compile the WASM dynamic library using wasm-pack
echo "Step 1: Compiling WASM core via wasm-pack..."
if ! command -v wasm-pack &> /dev/null; then
  echo "Error: wasm-pack is not installed." >&2
  echo "Please install it from: https://rustwasm.github.io/wasm-pack/installer/" >&2
  exit 1
fi
wasm-pack build --target web

# Step 2: Clean the dist/ directory
echo "Step 2: Cleaning old build outputs (dist/ directory)..."
rm -rf dist/

# Step 3: Bundle and minify using Vite
echo "Step 3: Checking Node.js and npm..."
if ! command -v npm &> /dev/null; then
  echo "Error: npm (Node Package Manager) is required to run Vite but is not installed." >&2
  echo "Please install Node.js and npm from https://nodejs.org/" >&2
  exit 1
fi

echo "Installing package dependencies..."
npm install

echo "Bundling and minifying web app via Vite..."
npm run build

# Step 4: Post-build copy hook to unify ROM paths
echo "Step 4: Copying static public assets to dist/public..."
mkdir -p dist/public
cp -r static/public/* dist/public/

echo "====================================================="
echo " 🎉 FcEmu Web Build Completed successfully!"
echo " Output assets are located in: ./dist"
echo "====================================================="
