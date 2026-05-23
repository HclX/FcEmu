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
}
