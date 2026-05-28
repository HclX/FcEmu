use crate::core::bus::SimpleBus;
use crate::core::cartridge::Cartridge;
use crate::core::cpu::Cpu;
use wasm_bindgen::prelude::*;

/// WebAssembly wrapper for the FcEmu Emulator Core.
#[wasm_bindgen]
pub struct WasmEmulator {
    cpu: Cpu,
    bus: SimpleBus,
}

impl Default for WasmEmulator {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]

impl WasmEmulator {
    /// Instantiates the emulator core and resets the CPU.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let mut cpu = Cpu::new();
        let mut bus = SimpleBus::new();
        cpu.reset(&mut bus);
        Self { cpu, bus }
    }

    /// Parses raw ROM bytes, loads the cartridge into the bus, and resets the CPU.
    pub fn load_rom(&mut self, data: &[u8]) -> bool {
        match Cartridge::from_rom(data) {
            Ok(cartridge) => {
                self.bus.load_cartridge(cartridge);
                self.cpu.reset(&mut self.bus);
                true
            }
            Err(_) => false,
        }
    }

    /// Resets the emulator core (CPU and PPU).
    pub fn reset(&mut self) {
        self.cpu.reset(&mut self.bus);
        self.bus.ppu.reset();
    }

    pub fn get_region(&self) -> u8 {
        match self.bus.timing.region {
            crate::core::region::EmulatorRegion::Ntsc => 0,
            crate::core::region::EmulatorRegion::Pal => 1,
        }
    }

    pub fn set_region(&mut self, region: u8) {
        let r = match region {
            0 => crate::core::region::EmulatorRegion::Ntsc,
            1 => crate::core::region::EmulatorRegion::Pal,
            _ => crate::core::region::EmulatorRegion::Ntsc,
        };
        self.bus.set_region(r);
    }

    pub fn get_cartridge_detected_region(&self) -> u8 {
        if let Some(ref cart) = self.bus.cartridge {
            match cart.region {
                crate::core::region::EmulatorRegion::Ntsc => 0,
                crate::core::region::EmulatorRegion::Pal => 1,
            }
        } else {
            0 // Default to NTSC
        }
    }

    /// Steps CPU instruction-by-instruction, syncs PPU, and ticks the APU until the frame is complete.
    pub fn step_frame(&mut self) {
        self.bus.ppu_frame_complete = false;
        while !self.bus.ppu_frame_complete {
            self.bus.ppu_ticked_cycles = 0;
            self.bus.cpu_cycles_spent_in_io = 0;

            let cycles = self.cpu.step(&mut self.bus);

            // Catch up PPU for idle CPU cycles
            if cycles > self.bus.cpu_cycles_spent_in_io {
                let idle_cycles = cycles - self.bus.cpu_cycles_spent_in_io;
                let catch_up_ppu = self.bus.accumulate_ppu_cycles(idle_cycles);
                self.bus.tick_ppu(catch_up_ppu);
            }

            self.bus.apu.tick(cycles);
        }
    }

    /// Exposes raw pointer to the 256x240x4 (RGBA32) screen frame buffer.
    pub fn frame_buffer_ptr(&self) -> *const u8 {
        self.bus.ppu.frame_buffer.as_ptr()
    }

    /// Exposes raw pointer to the dynamic float sample buffer.
    pub fn sample_buffer_ptr(&self) -> *const f32 {
        self.bus.apu.sample_buffer.as_ptr()
    }

    /// Returns the current count of float samples queued in the sample buffer.
    pub fn sample_buffer_len(&self) -> usize {
        self.bus.apu.sample_buffer.len()
    }

    /// Clears/drains the sample buffer after consumption.
    pub fn clear_sample_buffer(&mut self) {
        self.bus.apu.sample_buffer.clear();
    }

    /// Writes the joypad input button mask.
    pub fn write_controller(&mut self, mask: u8) {
        self.bus.controller_state = mask;
    }

    /// Writes the joypad 2 input button mask.
    pub fn write_controller2(&mut self, mask: u8) {
        self.bus.controller2_state = mask;
    }

    /// Returns true if a cartridge is loaded and has battery-backed SRAM.
    pub fn has_battery_backed_sram(&self) -> bool {
        self.bus
            .cartridge
            .as_ref()
            .map(|cart| cart.has_battery)
            .unwrap_or(false)
    }

    /// Returns a copy of the current SRAM (prg_ram) contents, if a cartridge is loaded.
    pub fn get_sram(&self) -> Option<Vec<u8>> {
        self.bus.cartridge.as_ref().map(|cart| cart.prg_ram.clone())
    }

    /// Overwrites the cartridge's SRAM with the provided data.
    /// Returns true if successful, false if no cartridge is loaded or data length is incorrect.
    pub fn set_sram(&mut self, data: &[u8]) -> bool {
        if let Some(ref mut cart) = self.bus.cartridge {
            if data.len() == cart.prg_ram.len() {
                cart.prg_ram.copy_from_slice(data);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut state = Vec::with_capacity(90000);

        // 1. CPU (13 bytes)
        state.push(self.cpu.a);
        state.push(self.cpu.x);
        state.push(self.cpu.y);
        state.push(self.cpu.status);
        state.push(self.cpu.sp);
        state.extend_from_slice(&self.cpu.pc.to_le_bytes());
        state.extend_from_slice(&self.cpu.cycles.to_le_bytes());

        // 2. SimpleBus (mem & vram & controllers) (67591 bytes)
        state.extend_from_slice(&self.bus.mem);
        state.extend_from_slice(&self.bus.vram);
        state.push(self.bus.controller_state);
        state.push(self.bus.controller_latch);
        state.push(self.bus.controller_shift);
        state.push(self.bus.controller2_state);
        state.push(self.bus.controller2_shift);

        // 3. PPU (312 bytes)
        state.extend_from_slice(&self.bus.ppu.v.to_le_bytes());
        state.extend_from_slice(&self.bus.ppu.t.to_le_bytes());
        state.push(self.bus.ppu.x);
        state.push(if self.bus.ppu.w { 1 } else { 0 });
        state.push(self.bus.ppu.ctrl);
        state.push(self.bus.ppu.mask);
        state.push(self.bus.ppu.status);
        state.push(self.bus.ppu.data_buffer);
        state.push(self.bus.ppu.oam_addr);
        state.extend_from_slice(&self.bus.ppu.oam_data);
        state.extend_from_slice(&self.bus.ppu.palette_ram);
        state.extend_from_slice(&self.bus.ppu.scanline.to_le_bytes());
        state.extend_from_slice(&self.bus.ppu.cycle.to_le_bytes());
        state.push(if self.bus.ppu.nmi_asserted { 1 } else { 0 });

        // 4. APU (Full Serialization) (68 bytes)
        // APU Mixer / Filter / Frame Counter (29 bytes)
        state.extend_from_slice(&self.bus.apu.prev_input.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.prev_output.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.prev_lpf_output.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.frame_counter_cycle.to_le_bytes());
        state.push(self.bus.apu.frame_counter_step);
        state.extend_from_slice(&self.bus.apu.cycle_accumulator.to_le_bytes());

        // Pulse 1 (9 bytes)
        state.push(if self.bus.apu.pulse1.enabled { 1 } else { 0 });
        state.push(self.bus.apu.pulse1.duty);
        state.push(if self.bus.apu.pulse1.constant_volume {
            1
        } else {
            0
        });
        state.push(self.bus.apu.pulse1.volume);
        state.extend_from_slice(&self.bus.apu.pulse1.timer_period.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.pulse1.timer.to_le_bytes());
        state.push(self.bus.apu.pulse1.duty_step);
        state.push(self.bus.apu.pulse1.length_counter);

        // Pulse 2 (9 bytes)
        state.push(if self.bus.apu.pulse2.enabled { 1 } else { 0 });
        state.push(self.bus.apu.pulse2.duty);
        state.push(if self.bus.apu.pulse2.constant_volume {
            1
        } else {
            0
        });
        state.push(self.bus.apu.pulse2.volume);
        state.extend_from_slice(&self.bus.apu.pulse2.timer_period.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.pulse2.timer.to_le_bytes());
        state.push(self.bus.apu.pulse2.duty_step);
        state.push(self.bus.apu.pulse2.length_counter);

        // Triangle (10 bytes)
        state.push(if self.bus.apu.triangle.enabled { 1 } else { 0 });
        state.push(if self.bus.apu.triangle.control_flag {
            1
        } else {
            0
        });
        state.push(self.bus.apu.triangle.linear_counter_reload);
        state.push(self.bus.apu.triangle.linear_counter);
        state.extend_from_slice(&self.bus.apu.triangle.timer_period.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.triangle.timer.to_le_bytes());
        state.push(self.bus.apu.triangle.step);
        state.push(self.bus.apu.triangle.length_counter);

        // Noise (11 bytes)
        state.push(if self.bus.apu.noise.enabled { 1 } else { 0 });
        state.push(if self.bus.apu.noise.constant_volume {
            1
        } else {
            0
        });
        state.push(self.bus.apu.noise.volume);
        state.push(if self.bus.apu.noise.loop_noise { 1 } else { 0 });
        state.extend_from_slice(&self.bus.apu.noise.timer_period.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.noise.timer.to_le_bytes());
        state.extend_from_slice(&self.bus.apu.noise.shift_register.to_le_bytes());
        state.push(self.bus.apu.noise.length_counter);

        // 5. Cartridge State
        if let Some(ref cart) = self.bus.cartridge {
            state.push(1); // Has cartridge
            let cart_state = cart.save_state();
            state.extend_from_slice(&(cart_state.len() as u32).to_le_bytes());
            state.extend_from_slice(&cart_state);
        } else {
            state.push(0); // No cartridge
        }

        state
    }

    pub fn load_state(&mut self, state: &[u8]) -> bool {
        // Helper macro for safe slice-to-array conversion
        macro_rules! safe_bytes {
            ($slice:expr, $len:ty) => {
                match $slice.try_into() {
                    Ok(bytes) => bytes,
                    Err(_) => return false,
                }
            };
        }
        // Minimum size check: CPU (15) + SimpleBus (67589) + PPU (305) + APU (65) + Cartridge Flag (1) = 67975
        if state.len() < 67975 {
            return false;
        }

        let mut idx = 0;

        // 1. CPU
        self.cpu.a = state[idx];
        idx += 1;
        self.cpu.x = state[idx];
        idx += 1;
        self.cpu.y = state[idx];
        idx += 1;
        self.cpu.status = state[idx];
        idx += 1;
        self.cpu.sp = state[idx];
        idx += 1;
        self.cpu.pc = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.cpu.cycles = u64::from_le_bytes(safe_bytes!(&state[idx..idx + 8], [u8; 8]));
        idx += 8;

        // 2. SimpleBus
        self.bus.mem.copy_from_slice(&state[idx..idx + 65536]);
        idx += 65536;
        self.bus.vram.copy_from_slice(&state[idx..idx + 2048]);
        idx += 2048;
        self.bus.controller_state = state[idx];
        idx += 1;
        self.bus.controller_latch = state[idx];
        idx += 1;
        self.bus.controller_shift = state[idx];
        idx += 1;
        self.bus.controller2_state = state[idx];
        idx += 1;
        self.bus.controller2_shift = state[idx];
        idx += 1;

        // 3. PPU
        self.bus.ppu.v = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.ppu.t = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.ppu.x = state[idx];
        idx += 1;
        self.bus.ppu.w = state[idx] == 1;
        idx += 1;
        self.bus.ppu.ctrl = state[idx];
        idx += 1;
        self.bus.ppu.mask = state[idx];
        idx += 1;
        self.bus.ppu.status = state[idx];
        idx += 1;
        self.bus.ppu.data_buffer = state[idx];
        idx += 1;
        self.bus.ppu.oam_addr = state[idx];
        idx += 1;
        self.bus
            .ppu
            .oam_data
            .copy_from_slice(&state[idx..idx + 256]);
        idx += 256;
        self.bus
            .ppu
            .palette_ram
            .copy_from_slice(&state[idx..idx + 32]);
        idx += 32;
        self.bus.ppu.scanline = i16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.ppu.cycle = i16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.ppu.nmi_asserted = state[idx] == 1;
        idx += 1;

        // 4. APU
        self.bus.apu.prev_input = f32::from_le_bytes(safe_bytes!(&state[idx..idx + 4], [u8; 4]));
        idx += 4;
        self.bus.apu.prev_output = f32::from_le_bytes(safe_bytes!(&state[idx..idx + 4], [u8; 4]));
        idx += 4;
        self.bus.apu.prev_lpf_output =
            f32::from_le_bytes(safe_bytes!(&state[idx..idx + 4], [u8; 4]));
        idx += 4;
        self.bus.apu.frame_counter_cycle =
            u32::from_le_bytes(safe_bytes!(&state[idx..idx + 4], [u8; 4]));
        idx += 4;
        self.bus.apu.frame_counter_step = state[idx];
        idx += 1;
        self.bus.apu.cycle_accumulator =
            f64::from_le_bytes(safe_bytes!(&state[idx..idx + 8], [u8; 8]));
        idx += 8;

        // Pulse 1
        self.bus.apu.pulse1.enabled = state[idx] == 1;
        idx += 1;
        self.bus.apu.pulse1.duty = state[idx];
        idx += 1;
        self.bus.apu.pulse1.constant_volume = state[idx] == 1;
        idx += 1;
        self.bus.apu.pulse1.volume = state[idx];
        idx += 1;
        self.bus.apu.pulse1.timer_period =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.pulse1.timer = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.pulse1.duty_step = state[idx];
        idx += 1;
        self.bus.apu.pulse1.length_counter = state[idx];
        idx += 1;

        // Pulse 2
        self.bus.apu.pulse2.enabled = state[idx] == 1;
        idx += 1;
        self.bus.apu.pulse2.duty = state[idx];
        idx += 1;
        self.bus.apu.pulse2.constant_volume = state[idx] == 1;
        idx += 1;
        self.bus.apu.pulse2.volume = state[idx];
        idx += 1;
        self.bus.apu.pulse2.timer_period =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.pulse2.timer = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.pulse2.duty_step = state[idx];
        idx += 1;
        self.bus.apu.pulse2.length_counter = state[idx];
        idx += 1;

        // Triangle
        self.bus.apu.triangle.enabled = state[idx] == 1;
        idx += 1;
        self.bus.apu.triangle.control_flag = state[idx] == 1;
        idx += 1;
        self.bus.apu.triangle.linear_counter_reload = state[idx];
        idx += 1;
        self.bus.apu.triangle.linear_counter = state[idx];
        idx += 1;
        self.bus.apu.triangle.timer_period =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.triangle.timer =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.triangle.step = state[idx];
        idx += 1;
        self.bus.apu.triangle.length_counter = state[idx];
        idx += 1;

        // Noise
        self.bus.apu.noise.enabled = state[idx] == 1;
        idx += 1;
        self.bus.apu.noise.constant_volume = state[idx] == 1;
        idx += 1;
        self.bus.apu.noise.volume = state[idx];
        idx += 1;
        self.bus.apu.noise.loop_noise = state[idx] == 1;
        idx += 1;
        self.bus.apu.noise.timer_period =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.noise.timer = u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.noise.shift_register =
            u16::from_le_bytes(safe_bytes!(&state[idx..idx + 2], [u8; 2]));
        idx += 2;
        self.bus.apu.noise.length_counter = state[idx];
        idx += 1;

        // 5. Cartridge State
        let has_cart = state[idx];
        idx += 1;
        if has_cart == 1 {
            if state.len() < idx + 4 {
                return false;
            }
            let cart_state_len =
                u32::from_le_bytes(safe_bytes!(&state[idx..idx + 4], [u8; 4])) as usize;
            idx += 4;
            if state.len() < idx + cart_state_len {
                return false;
            }
            if let Some(ref mut cart) = self.bus.cartridge {
                match cart.load_state(&state[idx..idx + cart_state_len]) {
                    Ok(read_bytes) => {
                        let _ = read_bytes;
                    }
                    Err(_) => return false,
                }
            } else {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_emulator_sram() {
        let mut emu = WasmEmulator::new();
        assert_eq!(emu.has_battery_backed_sram(), false);
        assert!(emu.get_sram().is_none());
        assert_eq!(emu.set_sram(&[0; 8192]), false);

        // Create a valid mock iNES ROM with battery-backed SRAM (NROM mapper 0)
        let mut rom = vec![0; 16 + 16384 + 8192];
        rom[0..4].copy_from_slice(&[0x4E, 0x45, 0x53, 0x1A]);
        rom[4] = 1;
        rom[5] = 1;
        rom[6] = 0x02; // Has battery

        assert!(emu.load_rom(&rom));
        assert_eq!(emu.has_battery_backed_sram(), true);

        let sram = emu.get_sram().unwrap();
        assert_eq!(sram.len(), 8192);
        assert_eq!(sram[0], 0);

        let mut new_sram = vec![0; 8192];
        new_sram[0] = 0xAA;
        new_sram[100] = 0x55;
        assert!(emu.set_sram(&new_sram));

        let retrieved = emu.get_sram().unwrap();
        assert_eq!(retrieved[0], 0xAA);
        assert_eq!(retrieved[100], 0x55);
    }

    #[test]
    fn test_wasm_emulator_controller2() {
        use crate::core::bus::CpuBus;
        let mut emu = WasmEmulator::new();

        // Controller 2 state: button A (0x01) and Select (0x04) pressed
        emu.write_controller2(0x05);

        // Strobe high then low to latch button states
        emu.bus.write(0x4016, 1);
        emu.bus.write(0x4016, 0);

        // Verify reading Controller 2 bits sequentially
        // A (1) -> B (0) -> Select (1) -> Start (0) -> Up (0) -> Down (0) -> Left (0) -> Right (0)
        assert_eq!(emu.bus.read(0x4017), 0x41); // A
        assert_eq!(emu.bus.read(0x4017), 0x40); // B
        assert_eq!(emu.bus.read(0x4017), 0x41); // Select
        assert_eq!(emu.bus.read(0x4017), 0x40); // Start
        assert_eq!(emu.bus.read(0x4017), 0x40); // Up
        assert_eq!(emu.bus.read(0x4017), 0x40); // Down
        assert_eq!(emu.bus.read(0x4017), 0x40); // Left
        assert_eq!(emu.bus.read(0x4017), 0x40); // Right

        // Subsequent reads should return 1 (0x41) because shift shifts in 1s (0x80)
        assert_eq!(emu.bus.read(0x4017), 0x41);
    }

    #[test]
    fn test_wasm_emulator_savestate() {
        let mut emu = WasmEmulator::new();

        // Modify some registers and RAM to check state preservation
        emu.cpu.a = 0xAA;
        emu.cpu.x = 0x55;
        emu.cpu.pc = 0xC000;
        emu.cpu.cycles = 12345;

        emu.bus.mem[0x100] = 0xBC;
        emu.bus.mem[0x500] = 0xDE;
        emu.bus.vram[0x20] = 0x34;
        emu.bus.controller_state = 0x80; // Right pressed

        emu.bus.ppu.v = 0x2000;
        emu.bus.ppu.scanline = 100;
        emu.bus.ppu.cycle = 200;
        emu.bus.ppu.palette_ram[5] = 0x3F;

        emu.bus.apu.triangle.linear_counter = 15;
        emu.bus.apu.noise.shift_register = 0x1234;

        // Save state
        let state = emu.save_state();
        assert!(state.len() >= 67975);

        // Instantiate a fresh new emulator
        let mut fresh_emu = WasmEmulator::new();
        assert_ne!(fresh_emu.cpu.a, 0xAA);
        assert_ne!(fresh_emu.bus.mem[0x100], 0xBC);

        // Load state
        assert!(fresh_emu.load_state(&state));

        // Verify all fields are perfectly restored!
        assert_eq!(fresh_emu.cpu.a, 0xAA);
        assert_eq!(fresh_emu.cpu.x, 0x55);
        assert_eq!(fresh_emu.cpu.pc, 0xC000);
        assert_eq!(fresh_emu.cpu.cycles, 12345);

        assert_eq!(fresh_emu.bus.mem[0x100], 0xBC);
        assert_eq!(fresh_emu.bus.mem[0x500], 0xDE);
        assert_eq!(fresh_emu.bus.vram[0x20], 0x34);
        assert_eq!(fresh_emu.bus.controller_state, 0x80);

        assert_eq!(fresh_emu.bus.ppu.v, 0x2000);
        assert_eq!(fresh_emu.bus.ppu.scanline, 100);
        assert_eq!(fresh_emu.bus.ppu.cycle, 200);
        assert_eq!(fresh_emu.bus.ppu.palette_ram[5], 0x3F);

        assert_eq!(fresh_emu.bus.apu.triangle.linear_counter, 15);
        assert_eq!(fresh_emu.bus.apu.noise.shift_register, 0x1234);
    }

    #[test]
    fn test_wasm_emulator_savestate_with_cartridge() {
        let mut emu = WasmEmulator::new();

        // Create a valid mock iNES ROM with battery-backed SRAM (NROM mapper 0)
        let mut rom = vec![0; 16 + 16384 + 8192];
        rom[0..4].copy_from_slice(&[0x4E, 0x45, 0x53, 0x1A]);
        rom[4] = 1; // 1 PRG bank
        rom[5] = 1; // 1 CHR bank
        rom[6] = 0x02; // Has battery

        assert!(emu.load_rom(&rom));
        assert_eq!(emu.has_battery_backed_sram(), true);

        // Modify SRAM and mapper registers
        if let Some(ref mut cart) = emu.bus.cartridge {
            cart.prg_ram[10] = 0xDE;
            cart.prg_ram[100] = 0xAD;
        } else {
            panic!("No cartridge loaded");
        }

        // Save state
        let state = emu.save_state();
        assert!(state.len() > 67975); // Should be larger because it has Cartridge state

        // Create another emulator and load same ROM
        let mut fresh_emu = WasmEmulator::new();
        assert!(fresh_emu.load_rom(&rom));

        // Verify fresh SRAM is empty
        if let Some(ref cart) = fresh_emu.bus.cartridge {
            assert_eq!(cart.prg_ram[10], 0);
            assert_eq!(cart.prg_ram[100], 0);
        }

        // Load state
        assert!(fresh_emu.load_state(&state));

        // Verify SRAM restored!
        if let Some(ref cart) = fresh_emu.bus.cartridge {
            assert_eq!(cart.prg_ram[10], 0xDE);
            assert_eq!(cart.prg_ram[100], 0xAD);
        }
    }
}
