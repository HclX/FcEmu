pub mod registers;
pub mod render;

use crate::core::bus::PpuBus;

pub struct Ppu {
    // Scroll and Address Registers (Loopy's Registers)
    pub v: u16,  // Current VRAM address (15 bits)
    pub t: u16,  // Temporary VRAM address (15 bits)
    pub x: u8,   // Fine X scroll (3 bits)
    pub w: bool, // Write toggle (1 bit)

    // Control and Status Registers
    pub ctrl: u8,   // PPUCTRL
    pub mask: u8,   // PPUMASK
    pub status: u8, // PPUSTATUS

    // Data buffering
    pub data_buffer: u8, // Latched data on PPUDATA read

    // VRAM / OAM
    pub oam_addr: u8,
    pub oam_data: [u8; 256], // Sprite memory
    pub palette_ram: [u8; 32],

    // Internal Pipeline Counters
    pub scanline: i16,
    pub cycle: i16,

    // Output Frame Buffer (256 x 240 pixels, RGBA format)
    pub frame_buffer: Box<[u8; 256 * 240 * 4]>,

    // NMI signaling flags
    pub nmi_asserted: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            v: 0,
            t: 0,
            x: 0,
            w: false,
            ctrl: 0,
            mask: 0,
            status: 0,
            data_buffer: 0,
            oam_addr: 0,
            oam_data: [0; 256],
            palette_ram: [0; 32],
            scanline: 261, // Start at pre-render scanline
            cycle: 0,
            frame_buffer: Box::new([0; 256 * 240 * 4]),
            nmi_asserted: false,
        }
    }

    pub fn reset(&mut self) {
        self.v = 0;
        self.t = 0;
        self.x = 0;
        self.w = false;
        self.ctrl = 0;
        self.mask = 0;
        self.status = 0;
        self.data_buffer = 0;
        self.oam_addr = 0;
        self.oam_data = [0; 256];
        self.palette_ram = [0; 32];
        self.scanline = 261;
        self.cycle = 0;
        self.nmi_asserted = false;
        self.frame_buffer.fill(0);
    }

    /// Read PPU register from CPU ($2000 - $2007)
    pub fn read_reg<B: PpuBus>(&mut self, addr: u16, bus: &mut B) -> u8 {
        let reg = addr & 0x0007;
        match reg {
            // $2000 (PPUCTRL) - Write-only
            0 => 0,
            // $2001 (PPUMASK) - Write-only
            1 => 0,
            // $2002 (PPUSTATUS)
            2 => {
                let val = self.status;
                // Clear VBlank flag on read
                self.status &= !0x80;
                // Clear write latch
                self.w = false;
                val
            }
            // $2003 (OAMADDR) - Write-only
            3 => 0,
            // $2004 (OAMDATA)
            4 => self.oam_data[self.oam_addr as usize],
            // $2005 (PPUSCROLL) - Write-only
            5 => 0,
            // $2006 (PPUADDR) - Write-only
            6 => 0,
            // $2007 (PPUDATA)
            7 => {
                let val = if self.v < 0x3F00 {
                    // Buffered read
                    let buffered = self.data_buffer;
                    self.data_buffer = bus.read(self.v);
                    buffered
                } else {
                    // Immediate palette read
                    let palette_val = self.palette_ram[(self.v & 0x001F) as usize];
                    // Store background/nametable VRAM behind palette in buffer
                    self.data_buffer = bus.read(self.v - 0x1000); // dummy read from Nt mirror
                    palette_val
                };

                // Increment VRAM address
                let increment = if (self.ctrl & 0x04) != 0 { 32 } else { 1 };
                self.v = self.v.wrapping_add(increment) & 0x3FFF;

                val
            }
            _ => 0,
        }
    }

    /// Write to PPU register from CPU ($2000 - $2007)
    pub fn write_reg<B: PpuBus>(&mut self, addr: u16, val: u8, bus: &mut B) {
        let reg = addr & 0x0007;
        match reg {
            // $2000 (PPUCTRL)
            0 => self.write_ctrl(val),
            // $2001 (PPUMASK)
            1 => self.write_mask(val),
            // $2002 (PPUSTATUS) - Read-only
            2 => {}
            // $2003 (OAMADDR)
            3 => self.oam_addr = val,
            // $2004 (OAMDATA)
            4 => {
                self.oam_data[self.oam_addr as usize] = val;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            // $2005 (PPUSCROLL)
            5 => self.write_scroll(val),
            // $2006 (PPUADDR)
            6 => self.write_addr(val),
            // $2007 (PPUDATA)
            7 => {
                if self.v < 0x3F00 {
                    bus.write(self.v, val);
                } else {
                    let palette_addr = (self.v & 0x001F) as usize;
                    // Mirrors for palettes: $3F10, 3F14, 3F18, 3F1C mirror to background colors
                    let final_addr = if palette_addr >= 16 && palette_addr.is_multiple_of(4) {
                        palette_addr - 16
                    } else {
                        palette_addr
                    };
                    self.palette_ram[final_addr] = val;
                }

                // Increment VRAM address
                let increment = if (self.ctrl & 0x04) != 0 { 32 } else { 1 };
                self.v = self.v.wrapping_add(increment) & 0x3FFF;
            }
            _ => {}
        }
    }

    /// CPU writes to $4014 for OAM DMA
    pub fn write_oam_dma(&mut self, data: &[u8]) {
        let data_to_copy = &data[..256];
        if self.oam_addr == 0 {
            self.oam_data.copy_from_slice(data_to_copy);
        } else {
            let offset = self.oam_addr as usize;
            let first_len = 256 - offset;
            self.oam_data[offset..].copy_from_slice(&data_to_copy[..first_len]);
            self.oam_data[..offset].copy_from_slice(&data_to_copy[first_len..]);
        }
    }
}
