use crate::core::bus::MirroringMode;

/// Mapper trait translates CPU and PPU addresses into actual offsets inside
/// PRG-ROM/RAM and CHR-ROM/RAM.
pub trait Mapper: Send {
    /// Map a CPU read address to cartridge memory offset.
    /// Returns `Some(offset)` if handled by mapper, or `None` if unmapped.
    fn map_cpu_read(&self, addr: u16) -> Option<usize>;

    /// Map a CPU write address to cartridge memory offset.
    /// Returns `Some(offset)` if handled by mapper, or `None`.
    /// Can also trigger bank switching or internal mapper configuration.
    fn map_cpu_write(&mut self, addr: u16, val: u8) -> Option<usize>;

    /// Map a PPU read address to cartridge CHR memory offset.
    fn map_ppu_read(&self, addr: u16) -> Option<usize>;

    /// Map a PPU write address to cartridge CHR memory offset.
    fn map_ppu_write(&mut self, addr: u16, val: u8) -> Option<usize>;

    /// Get dynamically selected mirroring mode (if supported by mapper).
    fn mirroring(&self) -> Option<MirroringMode> {
        None
    }

    /// Step scanline timing (used by mappers with scanline interrupts, e.g., MMC3).
    fn step_scanline(&mut self) -> bool {
        false
    }

    /// Serialize internal mapper state.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Deserialize internal mapper state.
    fn load_state(&mut self, _state: &[u8]) {}
}

/// Mapper0 (NROM) mapping logic.
/// PRG ROM: 16KB (mirrored at $C000-$FFFF) or 32KB.
/// CHR ROM: 8KB.
pub struct Mapper0 {
    prg_banks: u8,
    chr_banks: u8,
}

impl Mapper0 {
    pub fn new(prg_banks: u8, chr_banks: u8) -> Self {
        Self {
            prg_banks,
            chr_banks,
        }
    }
}

impl Mapper for Mapper0 {
    fn map_cpu_read(&self, addr: u16) -> Option<usize> {
        if addr >= 0x8000 {
            // $8000-$FFFF: PRG ROM
            // 16KB PRG ROM: mirrored to $C000-$FFFF.
            // 32KB PRG ROM: no mirroring.
            let mask = if self.prg_banks > 1 { 0x7FFF } else { 0x3FFF };
            Some((addr & mask) as usize)
        } else if (0x6000..=0x7FFF).contains(&addr) {
            Some((addr - 0x6000) as usize)
        } else {
            None
        }
    }

    fn map_cpu_write(&mut self, addr: u16, _val: u8) -> Option<usize> {
        if addr >= 0x8000 {
            let mask = if self.prg_banks > 1 { 0x7FFF } else { 0x3FFF };
            Some((addr & mask) as usize)
        } else if (0x6000..=0x7FFF).contains(&addr) {
            Some((addr - 0x6000) as usize)
        } else {
            None
        }
    }

    fn map_ppu_read(&self, addr: u16) -> Option<usize> {
        if addr < 0x2000 {
            // $0000-$1FFF: CHR ROM/RAM (8KB)
            Some(addr as usize)
        } else {
            None
        }
    }

    fn map_ppu_write(&mut self, addr: u16, _val: u8) -> Option<usize> {
        if addr < 0x2000 {
            if self.chr_banks == 0 {
                // CHR RAM
                Some(addr as usize)
            } else {
                // CHR ROM (read-only, but we return the offset so caller can handle it)
                Some(addr as usize)
            }
        } else {
            None
        }
    }
}

/// Mapper1 (MMC1) mapping logic.
pub struct Mapper1 {
    prg_banks: u8,
    chr_banks: u8,
    shift_reg: u8,
    write_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

impl Mapper1 {
    pub fn new(prg_banks: u8, chr_banks: u8) -> Self {
        Self {
            prg_banks,
            chr_banks,
            shift_reg: 0x10,
            write_count: 0,
            control: 0x0C, // Default: 16KB PRG swapping, horizontal mirroring
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }
}

impl Mapper for Mapper1 {
    fn map_cpu_read(&self, addr: u16) -> Option<usize> {
        match addr {
            0x6000..=0x7FFF => {
                Some((addr - 0x6000) as usize)
            }
            0x8000..=0xFFFF => {
                let prg_mode = (self.control >> 2) & 0x03;
                let bank_idx = match prg_mode {
                    0 | 1 => {
                        let base = (self.prg_bank & 0x0E) as usize;
                        if addr < 0xC000 {
                            base
                        } else {
                            base + 1
                        }
                    }
                    2 => {
                        if addr < 0xC000 {
                            0
                        } else {
                            self.prg_bank as usize
                        }
                    }
                    3 => {
                        if addr < 0xC000 {
                            self.prg_bank as usize
                        } else {
                            (self.prg_banks - 1) as usize
                        }
                    }
                    _ => unreachable!(),
                };
                let offset = (bank_idx % self.prg_banks as usize) * 16384 + (addr & 0x3FFF) as usize;
                Some(offset)
            }
            _ => None,
        }
    }

    fn map_cpu_write(&mut self, addr: u16, val: u8) -> Option<usize> {
        match addr {
            0x6000..=0x7FFF => {
                Some((addr - 0x6000) as usize)
            }
            0x8000..=0xFFFF => {
                if (val & 0x80) != 0 {
                    self.shift_reg = 0x10;
                    self.write_count = 0;
                    self.control |= 0x0C;
                } else {
                    let bit = val & 0x01;
                    self.shift_reg >>= 1;
                    self.shift_reg |= bit << 4;
                    self.write_count += 1;
                    if self.write_count == 5 {
                        let reg_val = self.shift_reg;
                        match addr {
                            0x8000..=0x9FFF => self.control = reg_val,
                            0xA000..=0xBFFF => self.chr_bank_0 = reg_val,
                            0xC000..=0xDFFF => self.chr_bank_1 = reg_val,
                            0xE000..=0xFFFF => self.prg_bank = reg_val,
                            _ => {}
                        }
                        self.shift_reg = 0x10;
                        self.write_count = 0;
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn map_ppu_read(&self, addr: u16) -> Option<usize> {
        if addr < 0x2000 {
            let chr_mode = (self.control >> 4) & 0x01;
            if chr_mode != 0 {
                // 4 KB mode
                let chr_banks_4kb = if self.chr_banks > 0 { (self.chr_banks as usize) * 2 } else { 2 };
                let bank_idx = if addr < 0x1000 {
                    self.chr_bank_0 as usize % chr_banks_4kb
                } else {
                    self.chr_bank_1 as usize % chr_banks_4kb
                };
                Some(bank_idx * 4096 + (addr & 0x0FFF) as usize)
            } else {
                // 8 KB mode
                let chr_banks_8kb = if self.chr_banks > 0 { self.chr_banks as usize } else { 1 };
                let bank_idx = (self.chr_bank_0 & 0xFE) as usize % chr_banks_8kb;
                Some(bank_idx * 8192 + (addr & 0x1FFF) as usize)
            }
        } else {
            None
        }
    }

    fn map_ppu_write(&mut self, addr: u16, _val: u8) -> Option<usize> {
        self.map_ppu_read(addr)
    }

    fn mirroring(&self) -> Option<MirroringMode> {
        let mode = self.control & 0x03;
        match mode {
            0 => Some(MirroringMode::SingleScreenLower),
            1 => Some(MirroringMode::SingleScreenUpper),
            2 => Some(MirroringMode::Vertical),
            3 => Some(MirroringMode::Horizontal),
            _ => unreachable!(),
        }
    }

    fn save_state(&self) -> Vec<u8> {
        let mut state = Vec::with_capacity(6);
        state.push(self.shift_reg);
        state.push(self.write_count);
        state.push(self.control);
        state.push(self.chr_bank_0);
        state.push(self.chr_bank_1);
        state.push(self.prg_bank);
        state
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 6 {
            self.shift_reg = state[0];
            self.write_count = state[1];
            self.control = state[2];
            self.chr_bank_0 = state[3];
            self.chr_bank_1 = state[4];
            self.prg_bank = state[5];
        }
    }
}

/// Mapper227 (Multicart / Chinese pirate board) mapping logic.
/// PRG ROM: Up to 1MB.
/// CHR RAM: 8KB.
pub struct Mapper227 {
    _prg_banks: u8,
    _chr_banks: u8,
    latch: u16, // Holds the 16-bit latched address written to $8000-$FFFF
}

impl Mapper227 {
    pub fn new(prg_banks: u8, chr_banks: u8) -> Self {
        Self {
            _prg_banks: prg_banks,
            _chr_banks: chr_banks,
            latch: 0, // Power-on default value: 0
        }
    }
}

impl Mapper for Mapper227 {
    fn map_cpu_read(&self, addr: u16) -> Option<usize> {
        if addr >= 0x8000 {
            let outer_bank = ((self.latch >> 5) & 0x07) as usize;
            let inner_bank = ((self.latch >> 2) & 0x07) as usize;
            let s = self.latch & 0x01;
            let o = (self.latch >> 7) & 0x01;
            let l = (self.latch >> 9) & 0x01;

            let bank_16k = if addr < 0xC000 {
                // range $8000-$BFFF
                if o == 1 {
                    if s == 1 {
                        // NROM-256 Mode
                        (inner_bank & 0x06) | ((addr >> 14) & 0x01) as usize
                    } else {
                        // NROM-128 Mode
                        inner_bank
                    }
                } else {
                    if s == 1 {
                        // PRG A14 is fixed to 0
                        inner_bank & 0x06
                    } else {
                        inner_bank
                    }
                }
            } else {
                // range $C000-$FFFF
                if o == 1 {
                    if s == 1 {
                        // NROM-256 Mode
                        (inner_bank & 0x06) | 0x01
                    } else {
                        // NROM-128 Mode (mirrored)
                        inner_bank
                    }
                } else {
                    if l == 1 {
                        // UNROM Mode: fixed inner bank 7
                        7
                    } else {
                        // Fixed low bank 0
                        0
                    }
                }
            };

            // 16KB bank selection offset inside 128KB outer block
            let offset = (outer_bank * 128 * 1024) + (bank_16k * 16 * 1024) + (addr as usize & 0x3FFF);
            Some(offset)
        } else if (0x6000..=0x7FFF).contains(&addr) {
            // Some Mapper 227 carts have 8KB battery WRAM
            Some((addr - 0x6000) as usize)
        } else {
            None
        }
    }

    fn map_cpu_write(&mut self, addr: u16, _val: u8) -> Option<usize> {
        if addr >= 0x8000 {
            // Address Latch: the written CPU address acts as the latch data register!
            self.latch = addr;
            None
        } else if (0x6000..=0x7FFF).contains(&addr) {
            Some((addr - 0x6000) as usize)
        } else {
            None
        }
    }

    fn map_ppu_read(&self, addr: u16) -> Option<usize> {
        if addr < 0x2000 {
            // 8KB unbanked CHR RAM
            Some(addr as usize)
        } else {
            None
        }
    }

    fn map_ppu_write(&mut self, addr: u16, _val: u8) -> Option<usize> {
        if addr < 0x2000 {
            let o = (self.latch >> 7) & 0x01;
            if o == 1 {
                // CHR-RAM is write-protected in NROM modes (O == 1)
                None
            } else {
                Some(addr as usize)
            }
        } else {
            None
        }
    }

    fn mirroring(&self) -> Option<MirroringMode> {
        let m = (self.latch >> 1) & 0x01;
        if m != 0 {
            Some(MirroringMode::Horizontal)
        } else {
            Some(MirroringMode::Vertical)
        }
    }

    fn save_state(&self) -> Vec<u8> {
        let mut state = Vec::with_capacity(2);
        state.extend_from_slice(&self.latch.to_le_bytes());
        state
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 2 {
            self.latch = u16::from_le_bytes(state[0..2].try_into().unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper0_sram_mapping() {
        let mut mapper = Mapper0::new(2, 1);
        assert_eq!(mapper.map_cpu_read(0x6000), Some(0));
        assert_eq!(mapper.map_cpu_read(0x7FFF), Some(0x1FFF));
        assert_eq!(mapper.map_cpu_write(0x6000, 0xAA), Some(0));
        assert_eq!(mapper.map_cpu_write(0x7FFF, 0x55), Some(0x1FFF));
    }

    #[test]
    fn test_mapper1_sram_mapping() {
        let mut mapper = Mapper1::new(4, 2);
        assert_eq!(mapper.map_cpu_read(0x6000), Some(0));
        assert_eq!(mapper.map_cpu_read(0x7FFF), Some(0x1FFF));
        assert_eq!(mapper.map_cpu_write(0x6000, 0xAA), Some(0));
        assert_eq!(mapper.map_cpu_write(0x7FFF, 0x55), Some(0x1FFF));
    }

    #[test]
    fn test_mapper1_shift_reg_and_prg_banking() {
        let mut mapper = Mapper1::new(4, 2);
        
        // Write 0x03 (binary 00011) to the PRG bank register
        mapper.map_cpu_write(0xE000, 0x01);
        mapper.map_cpu_write(0xE000, 0x01);
        mapper.map_cpu_write(0xE000, 0x00);
        mapper.map_cpu_write(0xE000, 0x00);
        mapper.map_cpu_write(0xE000, 0x00);
        
        assert_eq!(mapper.prg_bank, 0x03);
        
        // Under mode 3:
        // $8000-$BFFF maps to bank 3
        // $C000-$FFFF maps to last bank (bank 3 since prg_banks is 4)
        assert_eq!(mapper.map_cpu_read(0x8000), Some(3 * 16384));
        assert_eq!(mapper.map_cpu_read(0xC000), Some(3 * 16384));
        
        // Reset
        mapper.map_cpu_write(0xE000, 0x80);
        assert_eq!(mapper.shift_reg, 0x10);
        assert_eq!(mapper.write_count, 0);
    }

    #[test]
    fn test_mapper227_bankswitching() {
        let mut mapper = Mapper227::new(32, 0); // 512KB PRG-ROM, CHR-RAM

        // Test Case 1: Reset / Power-On Default (UNROM-like with fixed bank 0)
        // Latch = 0 (o = 0, s = 0, l = 0, outer = 0, inner = 0)
        // CPU $8000-$BFFF maps to inner bank 0, block 0 -> offset 0
        assert_eq!(mapper.map_cpu_read(0x8000), Some(0));
        // CPU $C000-$FFFF maps to fixed bank 0, block 0 -> offset 0
        assert_eq!(mapper.map_cpu_read(0xC000), Some(0));
        // Mirroring should be Vertical (M = 0)
        assert_eq!(mapper.mirroring(), Some(MirroringMode::Vertical));

        // Test Case 2: NROM-128 Mode (O = 1, S = 0, Inner Bank = 3, Outer Block = 2, Mirror = Horizontal)
        // Latch Bits:
        // Outer (Bits 7,6,5) = 2 (binary 010) -> 2 << 5 = 0x40
        // Inner (Bits 4,3,2) = 3 (binary 011) -> 3 << 2 = 0x0C
        // Mirror (Bit 1) = 1 -> 1 << 1 = 0x02
        // S (Bit 0) = 0 -> 0
        // O (Bit 7) = 1 -> 1 << 7 = 0x80
        // Total Latch Address = 0x80 | 0x40 | 0x0C | 0x02 = 0xC4e => Write to 0x8000 + 0xC4e = 0x8C4E
        // Let's do a simple direct test: write 0x8000 | 0x00C2 (O=1, outer=6, inner=0, mirror=1, s=0)
        mapper.map_cpu_write(0x80C2, 0);
        assert_eq!(mapper.mirroring(), Some(MirroringMode::Horizontal));
        // NROM-128 mode: CPU $8000 maps to inner bank PPp = 0 inside outer block 6 -> offset 6 * 128KB = 768KB
        assert_eq!(mapper.map_cpu_read(0x8000), Some(6 * 128 * 1024));
        // CPU $C000 maps to mirrored PPp = 0 inside outer 6 -> offset 768KB
        assert_eq!(mapper.map_cpu_read(0xC000), Some(6 * 128 * 1024));
        // CHR-RAM should be write-protected (O = 1)
        assert_eq!(mapper.map_ppu_write(0x1000, 0xAA), None);

        // Test Case 3: NROM-256 Mode (O = 1, S = 1, Inner Bank = 4, Outer Block = 1)
        // Latch address: O=1 (0x80), S=1 (0x01), Inner=4 (0x10), Outer=1 (0x20) -> latch = 0xB1
        mapper.map_cpu_write(0x80B1, 0);
        // CPU A14 determines A14 line:
        // CPU $8000 (A14=0) -> bank 4 (inner 4 & 6 = 4) -> offset 5 * 128KB + 4 * 16KB = 720896 bytes
        assert_eq!(mapper.map_cpu_read(0x8000), Some(5 * 128 * 1024 + 4 * 16 * 1024));
        // CPU $C000 (A14=1) -> bank 5 (inner 4 | 1 = 5) -> offset 5 * 128KB + 5 * 16KB = 737280 bytes
        assert_eq!(mapper.map_cpu_read(0xC000), Some(5 * 128 * 1024 + 5 * 16 * 1024));

        // Test Case 4: UNROM Mode (O = 0, L = 1, Inner Bank = 3, Outer Block = 0)
        // Latch address: O=0, L=1 (0x200 -> Bit 9!), Inner=3 (0x0C) -> latch = 0x020C
        mapper.map_cpu_write(0x820C, 0);
        // CPU $8000 maps to switchable inner bank 3 -> offset 3 * 16KB = 48KB
        assert_eq!(mapper.map_cpu_read(0x8000), Some(3 * 16 * 1024));
        // CPU $C000 maps to fixed inner bank #7 -> offset 7 * 16KB = 112KB
        assert_eq!(mapper.map_cpu_read(0xC000), Some(7 * 16 * 1024));
        // CHR-RAM write should be enabled (O = 0)
        assert_eq!(mapper.map_ppu_write(0x1000, 0xAA), Some(0x1000));
    }
}
