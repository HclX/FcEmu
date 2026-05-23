use wasm_bindgen::prelude::*;
use crate::core::cpu::Cpu;
use crate::core::bus::SimpleBus;
use crate::core::cartridge::Cartridge;

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

    /// Steps CPU instruction-by-instruction, syncs PPU, and ticks the APU until the frame is complete.
    pub fn step_frame(&mut self) {
        self.bus.ppu_frame_complete = false;
        while !self.bus.ppu_frame_complete {
            self.bus.ppu_ticked_cycles = 0;
            let cycles = self.cpu.step(&mut self.bus);
            
            let expected_ppu_cycles = cycles * 3;
            if expected_ppu_cycles > self.bus.ppu_ticked_cycles {
                let catch_up = expected_ppu_cycles - self.bus.ppu_ticked_cycles;
                self.bus.tick_ppu(catch_up);
            }
            
            self.bus.apu.tick(cycles);
        }
    }

    /// Exposes raw pointer to the 256x240x3 (RGB24) screen frame buffer.
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
        self.bus
            .cartridge
            .as_ref()
            .map(|cart| cart.prg_ram.clone())
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
}
