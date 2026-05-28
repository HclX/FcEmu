use super::Ppu;

impl Ppu {
    /// Write to PPUCTRL register ($2000)
    pub fn write_ctrl(&mut self, val: u8) {
        let prev_nmi_enable = self.ctrl & 0x80;
        self.ctrl = val;
        // Update temporary VRAM address with nametable select bits
        // t: ...BA.......... = val: ......BA
        self.t = (self.t & !0x0C00) | (((val & 0x03) as u16) << 10);

        // NMI re-trigger: if NMI enable was just turned on and VBlank flag is already set
        if prev_nmi_enable == 0 && (val & 0x80) != 0 && (self.status & 0x80) != 0 {
            self.nmi_asserted = true;
        }
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

#[cfg(test)]
mod tests {
    use crate::core::ppu::Ppu;

    // ── Loopy register operations ──────────────────────────────────

    #[test]
    fn test_increment_coarse_x_normal() {
        let mut ppu = Ppu::new();
        ppu.v = 0x0005; // coarse X = 5
        ppu.increment_coarse_x();
        assert_eq!(ppu.v & 0x001F, 6);
    }

    #[test]
    fn test_increment_coarse_x_wrap_nametable() {
        let mut ppu = Ppu::new();
        ppu.v = 0x001F; // coarse X = 31
        ppu.increment_coarse_x();
        assert_eq!(ppu.v & 0x001F, 0, "coarse X should wrap to 0");
        assert_ne!(ppu.v & 0x0400, 0, "horizontal nametable should toggle");
    }

    #[test]
    fn test_increment_coarse_x_wrap_back() {
        let mut ppu = Ppu::new();
        ppu.v = 0x0400 | 31; // coarse X = 31 in nametable 1
        ppu.increment_coarse_x();
        assert_eq!(ppu.v & 0x001F, 0);
        assert_eq!(ppu.v & 0x0400, 0, "nametable should toggle back");
    }

    #[test]
    fn test_increment_y_fine_y() {
        let mut ppu = Ppu::new();
        ppu.v = 0x0000; // fine Y = 0
        ppu.increment_y();
        assert_eq!(ppu.v & 0x7000, 0x1000, "fine Y should be 1");
    }

    #[test]
    fn test_increment_y_fine_y_overflow_to_coarse_y() {
        let mut ppu = Ppu::new();
        ppu.v = 0x7000; // fine Y = 7, coarse Y = 0
        ppu.increment_y();
        assert_eq!(ppu.v & 0x7000, 0, "fine Y should wrap to 0");
        assert_eq!((ppu.v & 0x03E0) >> 5, 1, "coarse Y should be 1");
    }

    #[test]
    fn test_increment_y_coarse_y_29_wraps_nametable() {
        let mut ppu = Ppu::new();
        ppu.v = 0x7000 | (29 << 5); // fine Y = 7, coarse Y = 29
        ppu.increment_y();
        assert_eq!(ppu.v & 0x7000, 0, "fine Y should wrap to 0");
        assert_eq!((ppu.v & 0x03E0) >> 5, 0, "coarse Y should wrap to 0");
        assert_ne!(ppu.v & 0x0800, 0, "vertical nametable should toggle");
    }

    #[test]
    fn test_increment_y_coarse_y_31_wraps_no_nametable() {
        let mut ppu = Ppu::new();
        ppu.v = 0x7000 | (31 << 5); // fine Y = 7, coarse Y = 31
        ppu.increment_y();
        assert_eq!(ppu.v & 0x7000, 0);
        assert_eq!((ppu.v & 0x03E0) >> 5, 0, "coarse Y should wrap to 0");
        assert_eq!(ppu.v & 0x0800, 0, "nametable should NOT toggle");
    }

    #[test]
    fn test_transfer_x() {
        let mut ppu = Ppu::new();
        ppu.v = 0x7FFF; // all bits set
        ppu.t = 0x0000;
        ppu.transfer_x();
        // Bits 0-4 (coarse X) and bit 10 (H nametable) should come from t
        assert_eq!(ppu.v & 0x041F, 0x0000);
        // Other bits should remain from v
        assert_eq!(ppu.v & !0x041Fu16, 0x7FFF & !0x041Fu16);
    }

    #[test]
    fn test_transfer_y() {
        let mut ppu = Ppu::new();
        ppu.v = 0x0000;
        ppu.t = 0x7BE0; // fine Y, coarse Y, V nametable all set
        ppu.transfer_y();
        assert_eq!(ppu.v & 0x7BE0, 0x7BE0);
    }

    // ── Palette mirroring ──────────────────────────────────────────

    #[test]
    fn test_palette_addr_normal() {
        let ppu = Ppu::new();
        assert_eq!(ppu.get_palette_addr(0x3F01), 1);
        assert_eq!(ppu.get_palette_addr(0x3F0F), 15);
    }

    #[test]
    fn test_palette_addr_sprite_bg_mirror() {
        let ppu = Ppu::new();
        // Sprite palette addresses that are multiples of 4 mirror to BG
        assert_eq!(ppu.get_palette_addr(0x3F10), 0);   // $3F10 -> $3F00
        assert_eq!(ppu.get_palette_addr(0x3F14), 4);   // $3F14 -> $3F04
        assert_eq!(ppu.get_palette_addr(0x3F18), 8);   // $3F18 -> $3F08
        assert_eq!(ppu.get_palette_addr(0x3F1C), 12);  // $3F1C -> $3F0C
    }

    #[test]
    fn test_palette_addr_non_mirror_sprite() {
        let ppu = Ppu::new();
        // Non-multiple-of-4 sprite palette addresses should NOT mirror
        assert_eq!(ppu.get_palette_addr(0x3F11), 17);
        assert_eq!(ppu.get_palette_addr(0x3F15), 21);
        assert_eq!(ppu.get_palette_addr(0x3F19), 25);
    }

    // ── NMI re-trigger on PPUCTRL write ────────────────────────────

    #[test]
    fn test_nmi_retrigger_on_ctrl_write() {
        let mut ppu = Ppu::new();
        // Simulate VBlank already set, NMI disabled
        ppu.status = 0x80;
        ppu.ctrl = 0x00;
        ppu.nmi_asserted = false;

        // Enable NMI via PPUCTRL write
        ppu.write_ctrl(0x80);
        assert!(ppu.nmi_asserted, "NMI should be re-triggered");
    }

    #[test]
    fn test_no_nmi_retrigger_when_vblank_clear() {
        let mut ppu = Ppu::new();
        ppu.status = 0x00; // VBlank NOT set
        ppu.ctrl = 0x00;
        ppu.nmi_asserted = false;

        ppu.write_ctrl(0x80);
        assert!(!ppu.nmi_asserted, "NMI should NOT fire when VBlank is clear");
    }

    #[test]
    fn test_no_nmi_retrigger_when_already_enabled() {
        let mut ppu = Ppu::new();
        ppu.status = 0x80;
        ppu.ctrl = 0x80; // NMI was already enabled
        ppu.nmi_asserted = false;

        ppu.write_ctrl(0x80);
        assert!(!ppu.nmi_asserted, "NMI should NOT fire if NMI enable was already set");
    }

    // ── Grayscale mode ─────────────────────────────────────────────

    #[test]
    fn test_grayscale_palette_mask() {
        // When grayscale bit (PPUMASK bit 0) is set, palette index should be ANDed with 0x30
        let color: u8 = 0x2D;
        let grayscale_color = color & 0x30;
        assert_eq!(grayscale_color, 0x20);
    }

    // ── Odd frame toggle ───────────────────────────────────────────

    #[test]
    fn test_odd_frame_toggle() {
        let mut ppu = Ppu::new();
        assert!(!ppu.odd_frame, "should start as even frame");
        ppu.odd_frame = !ppu.odd_frame;
        assert!(ppu.odd_frame, "should be odd after first toggle");
        ppu.odd_frame = !ppu.odd_frame;
        assert!(!ppu.odd_frame, "should be even after second toggle");
    }

    #[test]
    fn test_odd_frame_reset() {
        let mut ppu = Ppu::new();
        ppu.odd_frame = true;
        ppu.reset();
        assert!(!ppu.odd_frame, "odd_frame should be false after reset");
    }
}
