# FC/NES Emulator Development Plan: `TODO.md`

This document lists all tasks required to build a fully working, premium, test-driven Rust Famicom emulator. Tasks are structured chronologically and categorized by component tracks to facilitate parallel contributions.

---

## Phase 1: Foundation & Cartridge Loader
*Goal: Initialize project, set up main data traits, and parse NES ROM files.*

- [ ] **1.1 Workspace Initialisation**
  - Setup dependency structure in `Cargo.toml` (e.g., `wasm-bindgen`, `image`, and `md5`).
- [ ] **1.2 Define Component Interfaces**
  - Add `CpuBus` trait in `src/core/bus.rs`.
  - Add `PpuBus` trait in `src/core/ppu/mod.rs`.
  - Add `Mapper` trait in `src/core/cartridge/mapper.rs`.
- [ ] **1.3 iNES Rom Parser**
  - Create `Cartridge` structural parser in `src/core/cartridge/mod.rs`.
  - Extract PRG-ROM, CHR-ROM size, mirroring modes, mapper ID, and battery-backed RAM flags from the 16-byte iNES header.
  - Implement basic unit tests asserting correct header parsing with dummy/mock files (fully headless, runnable in remote environment).

---

## Phase 2: CPU Core (2A03) & Verification (TDD Track)
*Goal: Implement a fully compliant MOS 6502 CPU, validated through the `nestest` ROM.*

- [ ] **2.1 CPU Registry & Memory State**
  - Define the `Cpu` struct containing registers (`A`, `X`, `Y`, `PC`, `SP`, `Status`) and cycle counters in `src/cpu/mod.rs`.
  - Implement read/write memory access wrappers utilizing the `CpuBus` trait.
- [ ] **2.2 Addressing Modes**
  - Implement 12 official 6502 addressing modes (Immediate, Zero Page, Absolute, Indirect, Indexed, etc.) in `src/cpu/mod.rs`.
- [ ] **2.3 CPU Opcodes Implementation**
  - Implement execution logic for 56 primary instructions (ADC, SBC, LDA, STA, jumps, stack ops, status flag modifications) in `src/cpu/opcodes.rs`.
  - Set up correct cycle counting logic (including extra cycles on page boundary crossing).
- [ ] **2.4 Nestest TDD Execution Runner**
  - Write an integration test that loads `nestest.nes`, sets `PC` to `0xC000`, runs cycles step-by-step, and dumps state:
    `PC  OP A:xx X:xx Y:xx P:xx SP:xx CYC:xxxx`
  - Match CPU execution logs against the reference `nestest.log` line-by-line up to instruction ~9000.
  - Fix flag calculations (especially Decimal flags, negative flags, overflow conditions) and opcode cycles until 100% match is achieved.

---

## Phase 3: PPU (2C02) Video Engine
*Goal: Write the pixel renderer, low-level scroll, and sprite handling systems.*

- [ ] **3.1 PPU Register Interface & Mirroring**
  - Implement CPU register mapping (`0x2000 - 0x2007`) inside the PPU module.
  - Implement internal Palette RAM memory and mirroring rules (e.g., `0x3F10` mirroring `0x3F00`).
  - Setup horizontal and vertical VRAM mirroring based on cartridge settings.
- [ ] **3.2 Loopy Scrolling Registers**
  - Code the low-level `v` (current address), `t` (temp address), `x` (fine X scroll), and `w` (write toggle) state logic inside `src/ppu/registers.rs`.
  - Correctly update `v` and `t` on writes to `0x2005` (PPUSCROLL) and `0x2006` (PPUADDR).
- [ ] **3.3 Background Tile Pipeline Rendering**
  - Implement the cycle-by-cycle shift registers to fetch Nametable tiles, Attribute bytes, and low/high Pattern table bytes.
  - Feed the fetched tile index and attributes into color selection.
  - Output pixel buffers to the 256x240 display grid.
- [ ] **3.4 OAM & Sprite Rendering**
  - Implement 256-byte OAM memory storage.
  - Support CPU OAM Direct Memory Access (`0x4014` DMA transfer).
  - Render sprites (8x8 and 8x16 options) with correct background/foreground priority layering and sprite zero hit detection (`PPUSTATUS` bit 6).

---

## Phase 4: Cartridge Mappers Support
*Goal: Implement dynamic banking mappers to support large games.*

- [ ] **4.1 Mapper 0 (NROM)**
  - Write mapper mappings mapping PRG banks static to 16KB/32KB and CHR banks static to 8KB.
  - Verify with standard CPU instruction loading tests.
- [ ] **4.2 Mapper 1 (MMC1)**
  - Implement serial shift write registers (writing 5 times to `0x8000 - 0xFFFF`).
  - Support switching between 16KB PRG banks and 4KB/8KB CHR banks.
  - Support dynamic runtime switching of mirroring mode (Horizontal, Vertical, Single Screen).
- [ ] **4.3 Mapper 2 (UxROM)**
  - Map CPU writes to cartridge range to select variable 16KB PRG ROM bank in lower address space, while fixing upper space to the last bank.
- [ ] **4.4 Mapper 4 (MMC3)**
  - Support bank switching patterns for both PRG and CHR segments.
  - Implement scanline timing counter tracking PPU A12 line toggles. Trigger CPU IRQ interrupt when counter counts down to zero (required for split-screen scrolling).

---

## Phase 5: Joypad, Synchronization, & Visual Frontend
*Goal: Assemble system components into a unified main loop and build the visual GUI.*

- [ ] **5.1 Synchronized Emulator Master Loop**
  - Build master emulator execution loop in `src/core/mod.rs`.
  - Step PPU exactly 3 cycles for every 1 CPU cycle.
  - Implement NMI interrupt propagation from PPU to CPU.
- [ ] **5.2 Headless Integration CLI (`src/bin/headless.rs`)**
  - Create a CLI client that runs the emulator engine without window environments.
  - Support flags to execute a ROM for exactly `N` frames, and dump the final RGB24 frame buffer to disk (e.g. as raw PPM/PNG) or verify its MD5 checksum.
- [ ] **5.3 Desktop GUI Wrapper (Optional, Feature-gated)**
  - Mount standard local renderer (`sdl2` or `pixels`) to mount the core frame buffer behind a cargo feature flag `desktop` (only built when display environments are present).

---

## Phase 6: APU (Audio Processing Unit)
*Goal: Implement audio rendering channels and sound mixer.*

- [ ] **6.1 Pulse Wave Channels**
  - Implement Pulse 1 and Pulse 2 square wave channels with custom duty cycle sweeps and volume envelopes.
- [ ] **6.2 Triangle & Noise Generators**
  - Implement linear frequency triangle wave generator for low-bass signals.
  - Implement random shift-register noise generator for sound effects.
- [ ] **6.3 Frame Counter & Output Sink**
  - Wire Frame Counter (240Hz) to trigger channel envelopes.
  - Mix pulse, triangle, and noise signals. Connect mixer to an audio output sink (e.g., `cpal` or `sdl2`).

---

## Phase 7: Verification & Integration Testing
*Goal: Test and verify emulator against 2 popular game ROMs.*

- [ ] **7.1 ROM 1 Verification: Super Mario Bros (NROM Mapper 0)**
  - Run Super Mario Bros ROM (either via WebAssembly (WASM) Player or Headless Frame Checks).
  - Verify: Perfect colors, smooth scroll updates, responsive controller inputs, accurate physics, correct collision (verified via sprite zero hit).
- [ ] **7.2 ROM 2 Verification: The Legend of Zelda (MMC1 Mapper 1)**
  - Run The Legend of Zelda ROM.
  - Verify: Horizontal/vertical scrolling transitions, screen-wipe rendering transitions, battery-backed game-save persistence to RAM files.
- [ ] **7.3 Diagnostic Test ROMs Verification**
  - Verify timing correctness against popular tests: `vbl_nmi_timing.nes` and `sprite_hit_tests.nes` by Blargg.
- [ ] **7.4 Headless CI / Automated Regression Testing**
  - Set up an automated script in `tests/` to run standard ROMs headlessly for 1000 frames and assert that the generated visual frame matches known-correct MD5 checksums.
- [ ] **7.5 Code Optimization**
  - Run cargo profiling benchmarks.
  - Ensure warm memory dispatch paths are inlined for maximum speed.

---

## Phase 8: Headless Golden Image Testing Harness
*Goal: Build automated player simulators to assert visual accuracy and controller inputs without display servers.*

- [ ] **8.1 Automated Script Development (`tests/verify_mario.py`)**
  - Create a Python validation script that runs `./target/debug/headless` on `roms/super_mario_bro.nes`.
  - Support precise frame-based input injection (simulating waiting, pressing `START`, loading, walking `RIGHT`, and jumping `RIGHT + A`).
- [ ] **8.2 Golden MD5 Registry Setup**
  - Establish official Visual MD5 Checksum references for standard game positions: Title Screen (Frame 60), Level Start (Frame 180), Scrolling (Frame 240), and Jumping (Frame 300).
- [ ] **8.3 Bounded-Window Tolerant Matching**
  - Implement frame-window checking (±3 frames) to survive minor clock cycle drifts between runs.

---

## Phase 9: Input Register & Key-Mapping Remediation (Dev A Track)
*Goal: Audit controller shift registers and resolve the non-responsive joypad inputs.*

- [ ] **9.1 Latch Audit (`0x4016` & `0x4017`)**
  - Verify that setting the controller strobe high continuously latches the key state, and setting it low locks the state and begins serial shifting.
  - Ensure sequential CPU reads properly shift out one button state bit at a time (A, B, Select, Start, Up, Down, Left, Right).
- [ ] **9.2 Inputs Latch Integration**
  - Ensure the latch integration works perfectly with standard input interfaces.

---

## Phase 10: PPU Scrolling, Mirroring & Sprite 0 Hit Remediation (Dev B Track)
*Goal: Rectify visual tearing, nametable errors, and game hanging at load screens.*

- [ ] **10.1 Scroll Registers Audit (`registers.rs`)**
  - Audit fine X/Y scrolling registers and coarse X/Y shifts.
  - Verify scroll register increments and wraps during visible rendering scanline steps (cycles 0 to 256).
- [ ] **10.2 Nametable Vertical Mirroring Fix**
  - Correct the VRAM nametable address mapping in `SimplePpuBus` for vertical mirroring modes, ensuring side-by-side screens wrap cleanly instead of copying garbage.
- [ ] **10.3 Sprite 0 Hit Cycle-Accurate Intersect**
  - Audit and correct the Sprite 0 Hit pixel check: trigger immediately when non-transparent pixels of sprite 0 and background overlap.
  - Prevent hanging during game booting by asserting that the Sprite 0 Hit flag resets correctly at the pre-render scanline (scanline 261).

---

## Phase 11: Playability Verification & Advanced Mappers (Dev C & D Track)
*Goal: Accomplish successful runs of Super Mario Bros and Zelda.*

- [ ] **11.1 Super Mario Bros. (Milestone Complete Gate)**
  - Verify that the title screen renders correctly, inputs trigger play, level loads, Mario walks right, jumps over obstacles, and screen scrolls horizontally without artifacts.
- [ ] **11.2 The Legend of Zelda. (MMC1 Complete Gate)**
  - Verify horizontal and vertical camera shifts.
  - Confirm that battery-backed RAM files persist saved states safely to disk.

---

## Phase 12: PPU Timing Alignment & Pixel Shifting Fix (Dev B Track)
*Goal: Correct the 1-pixel horizontal visual column tears near coarse X boundaries.*

- [ ] **12.1 Scroll Increment Timing Alignment**
  - Relocate the PPU internal scroll register updates (`increment_coarse_x`, `increment_y`, `transfer_x`) inside `src/core/ppu/render.rs` to execute **before** `self.cycle += 1`.
  - Ensure that coarse X is shifted exactly at the end of cycle 8 (after the 8th pixel column of the tile is scanned), rather than pre-shifting at the end of cycle 7.
- [ ] **12.2 Visual Boundary Validation**
  - Verify that background visual tiles align perfectly without any left/right offset jumps at border columns.

---

## Phase 16: Automated Formatting, Linting & Warnings Purge
*Goal: Enforce strict style guidelines and eliminate clippy static analysis warnings.*

- [ ] **16.1 Style Formatting Compliance (`cargo fmt`)**
  - Run `cargo fmt --all -- --check` to ensure all Rust source files are formatted identically to standard guidelines. Fix any format infractions.
- [ ] **16.2 Static Linting Remediation (`cargo clippy`)**
  - Run `cargo clippy --all-targets -- -D warnings` to catch and eliminate un-idiomatic patterns, redundant borrows, or redundant array allocations. Fix any clippy errors to make the compiler output pristine.
- [ ] **16.3 Purge Dead Code & Unused Imports**
  - Clean up any unused functions, modules, or variables. Ensure no stale mock modules or debug helpers remain.

---

## Phase 17: Idiomatic Refactoring & Zero-Regression Verification (Dev A, B & E2E)
*Goal: Restructure system loops, array indexing, and address ranges using idiomatic Rust patterns.*

- [ ] **17.1 SimpleBus Address Range Cleanups (`src/core/bus.rs`)**
  - Audit address boundaries inside `SimpleBus::read` and `SimpleBus::write`. Replace manual logic with idiomatic Rust `match` and closed range bounds (e.g., `0x2000..=0x3FFF`).
- [ ] **17.2 High-Speed OAM DMA Block Copying**
  - Refactor DMA transfers (`0x4014` write in `SimpleBus::write`). Replace manual element-by-element loops with optimized slice operations:
    `self.ppu.write_oam_dma(&self.mem[page_addr as usize..(page_addr + 256) as usize])`
  - Verify that slice operations compile cleanly and maintain the same speed metrics.
- [ ] **17.3 Zero-Regression Execution checks**
  - Run the automated verifier `tests/verify_mario.py` and connection checks `tests/e2e_runner.py` after every single refactor commit to assert **zero functional deviations** from the golden visual MD5 baseline.

---

## Phase 18: Sprite 0 Hit Timing & Scroll-Split Remediation (Dev B Track)
*Goal: Eliminate the horizontal "pixel-dragging" defect right below the levels status text.*

- [ ] **18.1 Cycle-Accurate Sprite 0 Intersection check**
  - Audit PPU pixel rendering checks inside `src/core/ppu/render.rs`. Ensure Sprite 0 Hit flag (`PPUSTATUS` bit 6) is set on the exact cycle where sprite and background pixels intersect.
  - Factor in background clipping mask checks (`PPUSTATUS` register properties) to ensure correct triggers at leftmost 8 columns.
- [ ] **18.2 Immediate Scroll Register writes Propagation**
  - Audit `SimpleBus::write` address handlers. Ensure writes to `$2005` (PPUSCROLL) and `$2006` (PPUADDR) immediately update active scrolling registers during CPU steps mid-scanline.
- [ ] **18.3 Playability Visual Validation**
  - Verify that horizontal maps scroll smoothly without any residual pixel-tearing or dragging artifacts on the screen.

---

## Phase 19: Crisp Scaling, aspect-ratio Locks & Size Controls (Dev D & JS)
*Goal: Scale canvas visual displays beautifully without any blocky blurriness.*

- [ ] **19.1 Sharp nearest-neighbor scaling**
  - Add crisp image-rendering CSS styles to `nes-canvas` inside the web player interface (`image-rendering: pixelated` and `image-rendering: crisp-edges`).
- [ ] **19.2 Size Option Buttons**
  - Implement HTML/CSS styling and sizing container locks (preserving the exact NES 8:7 / stretched 4:3 television aspect ratios).
  - Add neat visual sizing control buttons to the interface: `1x` (original), `2x` (scaled), `3x` (scaled), and `Fit Window`.

---

## Phase 20: APU Channels Integration
*Goal: Complete the APU channels integration and dynamic sample buffers.*

- [ ] **20.1 APU Channels & Audio sample queuing**
  - Implement Pulse 1, Pulse 2, Triangle, and Noise wave channels ticking in the core loop.
  - Queue audio samples at a standard `44,100Hz` sampling rate, generating exactly `735` samples per visual frame step.

---

## Phase 21: PPU Pre-Fetch Scroll Offset & Sprites Shifting Fix (Dev B Track)
*Goal: Completely align background tiles and sprites horizontally, eliminating the 1-block offset.*

- [ ] **21.1 Deactivate Pre-Fetch coarse X Increments**
  - Modify the scroll increment block in `src/core/ppu/render.rs`. Completely disable the `self.cycle == 328` and `self.cycle == 336` pre-fetch increments.
  - This stops `self.v` from pre-incrementing by +2 tiles (16 pixels) during pre-fetches, preserving the correct starting horizontal scroll position at column `x = 0`.
- [ ] **21.2 Sprite Relative Alignment Validation**
  - Verify that sprites (e.g. Mario, Goombas, coins) are rendered at their exact screen coordinates relative to the scrolled backgrounds without any 1-block gaps.

---

## Phase 22: Sprite 0 Hit timing & Scroll Split Remediation (Dev B & CPU Track)
*Goal: Erase the dragging pixel lines on status bar boundaries.*

- [ ] **22.1 Cycle-Accurate overlap trigger**
  - Ensure Sprite 0 Hit overlap immediately sets `PPUSTATUS` bit 6 on the exact cycle background and sprite pixels meet.
  - Audit and verify `$2005`/`$2006` scroll parameters writes propagate immediately during execution step loops.

---

## Phase 24: Cycle-Accurate Bus-Ticked PPU Integration (Dev B & CPU Track)
*Goal: Eliminate instruction-level step jitter.*

- [ ] **24.1 SimpleBus dynamic PPU stepping (`src/core/bus.rs`)**
  - Add `pub ppu_frame_complete: bool` to `SimpleBus`.
  - Implement `tick_ppu(&mut self, cycles: u32)` stepping PPU loops and marking `ppu_frame_complete = true` on frame ticks.
  - Call `self.tick_ppu(3)` at the very beginning of `SimpleBus::read` and `SimpleBus::write` to lock cycle-accurate CPU/PPU timing sync.
- [ ] **24.2 Emulator master loop rewires**
  - Simplify the execution timing loops inside `src/core/mod.rs` (master loop) and `src/bin/headless.rs` to step the CPU and only check `bus.ppu_frame_complete` for visual frame boundary triggers, removing obsolete cycle multipliers.

---

## Phase 25: Sprite 1-Scanline Vertical Delay Fix (Dev B Track)
*Goal: Correct the 1-scanline vertical shift of sprites relative to backgrounds.*

- [ ] **25.1 OAM Y Coordinate scanline delay (`src/core/ppu/render.rs`)**
  - Adjust the active Y coordinate range check inside `render_pixel` to add `+1` scanline offset to all sprite Ys loaded from OAM:
    `let sprite_y = self.oam_data[oam_idx] as usize + 1;`
  - Confirm that character/enemy sprites align vertically on backgrounds without any offset shearing.

---

## Phase 26: Absolute Regression Validation
*Goal: Lock down Golden visual MD5 checksums.*

- [ ] **26.1 Absolute regression validations**
  - Re-run `tests/verify_mario.py` to lock down Golden visual MD5 checksums against the unshifted, cycle-accurate visual states.







