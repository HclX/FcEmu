pub mod mapper;

use super::bus::MirroringMode;
use mapper::{Mapper, Mapper0, Mapper1, Mapper2, Mapper227};

pub struct Cartridge {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub prg_ram: Vec<u8>,
    pub chr_ram: Vec<u8>,
    pub mapper_id: u8,
    pub mirroring: MirroringMode,
    pub has_battery: bool,
    pub mapper: Box<dyn Mapper>,
}

impl Cartridge {
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        if data.len() < 16 {
            return Err("ROM too small".to_string());
        }

        if data[0..4] != [0x4E, 0x45, 0x53, 0x1A] {
            return Err("Invalid iNES magic number".to_string());
        }

        let prg_banks = data[4];
        let chr_banks = data[5];
        let control_1 = data[6];
        let control_2 = data[7];

        let mapper_id = (control_2 & 0xF0) | (control_1 >> 4);

        let mirroring = if (control_1 & 0x08) != 0 {
            MirroringMode::FourScreen
        } else if (control_1 & 0x01) != 0 {
            MirroringMode::Vertical
        } else {
            MirroringMode::Horizontal
        };

        let has_battery = (control_1 & 0x02) != 0;
        let has_trainer = (control_1 & 0x04) != 0;

        let prg_size = prg_banks as usize * 16384;
        let chr_size = chr_banks as usize * 8192;

        let header_offset = 16;
        let trainer_offset = if has_trainer { 512 } else { 0 };
        let prg_start = header_offset + trainer_offset;
        let chr_start = prg_start + prg_size;

        if data.len() < chr_start + chr_size {
            return Err("ROM data truncated".to_string());
        }

        let prg_rom = data[prg_start..prg_start + prg_size].to_vec();
        let chr_rom = if chr_size > 0 {
            data[chr_start..chr_start + chr_size].to_vec()
        } else {
            Vec::new()
        };

        let chr_ram = if chr_size == 0 {
            vec![0; 8192] // default 8KB CHR RAM if no CHR ROM
        } else {
            Vec::new()
        };

        let mapper: Box<dyn Mapper> = match mapper_id {
            0 => Box::new(Mapper0::new(prg_banks, chr_banks)),
            1 => Box::new(Mapper1::new(prg_banks, chr_banks)),
            2 => Box::new(Mapper2::new(prg_banks, chr_banks)),
            227 => Box::new(Mapper227::new(prg_banks, chr_banks)),
            _ => return Err(format!("Unsupported mapper: {}", mapper_id)),
        };

        Ok(Self {
            prg_rom,
            chr_rom,
            prg_ram: vec![0; 8192],
            chr_ram,
            mapper_id,
            mirroring,
            has_battery,
            mapper,
        })
    }

    pub fn read_cpu(&self, addr: u16) -> u8 {
        if let Some(offset) = self.mapper.map_cpu_read(addr) {
            if addr >= 0x8000 {
                if offset < self.prg_rom.len() {
                    self.prg_rom[offset]
                } else {
                    0
                }
            } else if (0x6000..0x8000).contains(&addr) {
                if offset < self.prg_ram.len() {
                    self.prg_ram[offset]
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        }
    }

    pub fn write_cpu(&mut self, addr: u16, val: u8) {
        if let Some(offset) = self.mapper.map_cpu_write(addr, val) {
            if addr >= 0x8000 {
                // PRG ROM is read-only
            } else if (0x6000..0x8000).contains(&addr) && offset < self.prg_ram.len() {
                self.prg_ram[offset] = val;
            }
        }
    }

    pub fn read_ppu(&self, addr: u16) -> u8 {
        if let Some(offset) = self.mapper.map_ppu_read(addr) {
            if offset < self.chr_rom.len() {
                self.chr_rom[offset]
            } else if offset < self.chr_ram.len() {
                self.chr_ram[offset]
            } else {
                0
            }
        } else {
            0
        }
    }

    pub fn write_ppu(&mut self, addr: u16, val: u8) {
        if let Some(offset) = self.mapper.map_ppu_write(addr, val) {
            if self.chr_rom.is_empty() && offset < self.chr_ram.len() {
                self.chr_ram[offset] = val;
            }
        }
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut state = Vec::with_capacity(8192 * 2 + 32);
        
        // Write PRG RAM (always 8KB in our core)
        state.extend_from_slice(&self.prg_ram);
        
        // Write CHR RAM length then data
        state.extend_from_slice(&(self.chr_ram.len() as u32).to_le_bytes());
        state.extend_from_slice(&self.chr_ram);
        
        // Write Mapper state length then data
        let mapper_state = self.mapper.save_state();
        state.extend_from_slice(&(mapper_state.len() as u32).to_le_bytes());
        state.extend_from_slice(&mapper_state);
        
        state
    }

    pub fn load_state(&mut self, state: &[u8]) -> Result<usize, String> {
        if state.len() < 8192 + 8 {
            return Err("State too small for Cartridge".to_string());
        }
        let mut idx = 0;
        
        // Restore PRG RAM
        self.prg_ram.copy_from_slice(&state[idx..idx + 8192]);
        idx += 8192;
        
        // Restore CHR RAM
        let chr_ram_len = u32::from_le_bytes(state[idx..idx+4].try_into().unwrap()) as usize;
        idx += 4;
        if chr_ram_len > 0 {
            if state.len() < idx + chr_ram_len {
                return Err("State truncated for CHR RAM".to_string());
            }
            if self.chr_ram.len() != chr_ram_len {
                self.chr_ram = vec![0; chr_ram_len];
            }
            self.chr_ram.copy_from_slice(&state[idx..idx + chr_ram_len]);
            idx += chr_ram_len;
        }
        
        // Restore Mapper State
        if state.len() < idx + 4 {
            return Err("State truncated for Mapper length".to_string());
        }
        let mapper_state_len = u32::from_le_bytes(state[idx..idx+4].try_into().unwrap()) as usize;
        idx += 4;
        if mapper_state_len > 0 {
            if state.len() < idx + mapper_state_len {
                return Err("State truncated for Mapper state".to_string());
            }
            self.mapper.load_state(&state[idx..idx + mapper_state_len]);
            idx += mapper_state_len;
        }
        
        Ok(idx)
    }
}
