use super::Ppu;

impl Ppu {
    /// Write to PPUCTRL register ($2000)
    pub fn write_ctrl(&mut self, val: u8) {
        self.ctrl = val;
        // Update temporary VRAM address with nametable select bits
        // t: ...BA.......... = val: ......BA
        self.t = (self.t & !0x0C00) | (((val & 0x03) as u16) << 10);
    }

    /// Write to PPUMASK register ($2001)
    pub fn write_mask(&mut self, val: u8) {
        self.mask = val;
    }

    /// Write to PPUSCROLL register ($2005)
    pub fn write_scroll(&mut self, val: u8) {
        if !self.w {
            // First write: X scroll
            // t: .......cba = val: cba
            // x = fine X scroll (3 bits)
            self.x = val & 0x07;
            self.t = (self.t & !0x001F) | ((val >> 3) as u16);
            self.w = true;
        } else {
            // Second write: Y scroll
            // t: .cba.. = val: cba
            // t: ..cba...... = val: cba
            self.t =
                (self.t & !0x73E0) | (((val & 0x07) as u16) << 12) | (((val >> 3) as u16) << 5);
            self.w = false;
        }
    }

    /// Write to PPUADDR register ($2006)
    pub fn write_addr(&mut self, val: u8) {
        if !self.w {
            // First write: High byte of address
            self.t = (self.t & 0x00FF) | (((val & 0x3F) as u16) << 8);
            self.w = true;
        } else {
            // Second write: Low byte of address
            self.t = (self.t & 0xFF00) | (val as u16);
            self.v = self.t;
            self.w = false;
        }
    }

    /// Increment coarse X in v (scroll address register)
    pub fn increment_coarse_x(&mut self) {
        if (self.v & 0x001F) == 31 {
            self.v &= !0x001F; // Coarse X = 0
            self.v ^= 0x0400; // Switch horizontal nametable
        } else {
            self.v += 1; // Coarse X + 1
        }
    }

    /// Increment fine Y (and coarse Y if overflowed) in v
    pub fn increment_y(&mut self) {
        if (self.v & 0x7000) != 0x7000 {
            self.v += 0x1000; // Increment fine Y
        } else {
            self.v &= !0x7000; // Fine Y = 0
            let mut y = (self.v & 0x03E0) >> 5; // Coarse Y
            if y == 29 {
                y = 0; // Reset coarse Y
                self.v ^= 0x0800; // Switch vertical nametable
            } else if y == 31 {
                y = 0; // Reset coarse Y (out-of-bounds area)
            } else {
                y += 1; // Coarse Y + 1
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }

    /// Copy coarse X and horizontal nametable bits from t to v
    pub fn transfer_x(&mut self) {
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }

    /// Copy fine Y, coarse Y, and vertical nametable bits from t to v
    pub fn transfer_y(&mut self) {
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    /// Get whether background rendering is enabled in PPUMASK
    pub fn show_background(&self) -> bool {
        (self.mask & 0x08) != 0
    }

    /// Get whether sprite rendering is enabled in PPUMASK
    pub fn show_sprites(&self) -> bool {
        (self.mask & 0x10) != 0
    }

    /// Get whether rendering (bg or sprites) is enabled
    pub fn rendering_enabled(&self) -> bool {
        self.show_background() || self.show_sprites()
    }
}
