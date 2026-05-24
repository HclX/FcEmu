# FcEmu — WebAssembly NES Emulator

> **Live Playable Demo**: 🎮 **[Play Free Homebrew Games Live on GitHub Pages!](https://HclX.github.io/FcEmu/)**

---

## 🚀 A Test of "Vibe Coding" with AI

This repository is a highly polished experimental test of **"Vibe Coding"** — a modern software development methodology where a human researcher pair-programs with a collaborative swarm of agentic AIs to build and optimize software rapidly.

The project began as a cloud-streamed emulator backend (written in Rust with Axum and WebSockets) streaming visual frames to a canvas. Together with the AI swarm, we successfully:
1.  **Re-architected** the entire engine into a **purely client-side standard web application** compiled to WebAssembly (WASM).
2.  **Purged** obsolete native servers and heavy dependencies, yielding an extremely lightweight compile-and-run footprint.
3.  **Implemented zero-copy browser memory sharing**, allowing JavaScript to cast canvas `ImageData` directly onto Rust WASM linear memory using a high-speed `Uint32Array` (32-bit accelerated writes).
4.  **Engineered clock-locked dynamic Web Audio queues** with snaps-on-jitter safety logic to eradicate static crackling/noises.
5.  **Added local SRAM battery persistence** using browser **IndexedDB** keyed on Web Crypto SHA-256 ROM hashes to save game states locally.
6.  **Configured Vite bundling** and portable relative pathing (`base: "./"`) so the static game hosts out-of-the-box on subdirectory servers.
7.  **Automated Release CI/CD** via GitHub Actions to build and deploy to GitHub Pages on every push.

---

## 🎮 Features

*   **Pure Client-Side WASM**: Executes 100% locally inside your browser sandbox. Zero server CPU or bandwidth usage.
*   **Instant Play & Local Load**: Drag-and-drop your own `.nes` files or click **⚡ Load Default: Pong 1K** to fetch and play instantly.
*   **Crisp Rendering & Ratios**: Features sharp nearest-neighbor pixel scaling with Native (8:7) and CRT (4:3) aspect ratio togglers.
*   **Local Saves (SRAM)**: Automatic 5-second dirty-checking auto-save and visible tab change saves persisted to browser `IndexedDB`.
*   **Full Keyboard Controls**: 
    *   **D-Pad**: Arrow Keys or `W` `A` `S` `D`
    *   **Button A**: `Z` or `J`
    *   **Button B**: `X` or `K`
    *   **Select**: `Right Shift` or `Space`
    *   **Start**: `Enter`

---

## 🛠️ Development & Verification

To verify or build the static release bundle locally:

1.  **Compile WASM & Vite Bundle**:
    Run the automated release script:
    ```bash
    ./build_web.sh
    ```
    *(Requires `wasm-pack` and `npm` installed locally).*
    
2.  **Local Preview**:
    Serve the resulting static assets inside `/dist` using any simple HTTP server:
    ```bash
    npx serve dist/
    # OR
    python3 -m http.server -d dist/ 8080
    ```

For detailed technical specifications, timing sync reviews, and architecture, see the **[DESIGN.md](DESIGN.md)** guide. For extensive local verification and automated deploy configs, see the **[RELEASE.md](RELEASE.md)** manual.

---

## ⚖️ Legal Disclaimer

**FcEmu** is an independent, open-source, educational emulator project. It is not affiliated with, authorized, sponsored, or endorsed by Nintendo Co., Ltd., its subsidiaries, or affiliates in any way. 

* "Nintendo Entertainment System", "NES", "Famicom", and all associated console designs and game titles are registered trademarks of Nintendo Co., Ltd. All trademarks and copyrights belong to their respective owners.
* No proprietary or copyrighted Nintendo console BIOS files are included or required by this project.
* All default ROMs included in the library are free, open-source homebrew games distributed legally with permission from their respective owners.

### 🎮 Homebrew Game Acknowledgements

We would like to express our deep gratitude to the creative developers in the NES homebrew community who created and freely distributed the legal games included in our default library:

*   **Nova the Squirrel**
    *   **Developer**: NovaSquirrel
    *   **Official Repository**: [Nova the Squirrel on GitHub](https://github.com/NovaSquirrel/NovaTheSquirrel)
    *   **License**: Open-source software, GPL-v3 licensed.
*   **Flappy Bird**
    *   **Developer**: jwarby
    *   **Official Repository**: [jwarby/flappy-bird-nes](https://github.com/jwarby/flappy-bird-nes)
    *   **License**: Open-source free homebrew software.

All homebrew ROM binaries are fetched from the community-maintained **[Retrobrews NES Games Collection](https://github.com/retrobrews/nes-games)** and original open-source developer repositories. We encourage players to visit these developers' websites, support their projects, and explore the rich world of modern NES homebrew development!
