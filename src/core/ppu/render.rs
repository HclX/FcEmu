use super::Ppu;
use crate::core::bus::PpuBus;
use crate::core::region::EmulatorRegion;

// Standard NTSC NES Palette (64 RGB colors)
pub const NES_PALETTE: [u8; 64 * 3] = [
    0x80, 0x80, 0x80, 0x00, 0x3D, 0xA6, 0x00, 0x12, 0xB0, 0x44, 0x00, 0x96, 0xA1, 0x00, 0x5E, 0xC7,
    0x00, 0x28, 0xBD, 0x06, 0x00, 0x86, 0x17, 0x00, 0x55, 0x2F, 0x00, 0x00, 0x7F, 0x00, 0x00, 0x43,
    0x00, 0x00, 0x3C, 0x00, 0x00, 0x10, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xC0, 0xC0, 0xC0, 0x1F, 0x7C, 0xEA, 0x30, 0x5F, 0xFE, 0x7E, 0x40, 0xF4, 0xB4, 0x34, 0xCE, 0xE4,
    0x30, 0x90, 0xDF, 0x43, 0x1F, 0xB7, 0x59, 0x00, 0x7E, 0x77, 0x00, 0x4E, 0x7F, 0x00, 0x3F, 0x7F,
    0x00, 0x00, 0x78, 0x45, 0x1F, 0x7C, 0xCE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0x64, 0xB0, 0xFF, 0x6C, 0x9E, 0xFF, 0xC2, 0x78, 0xFF, 0xFB, 0x62, 0xFF, 0xFF,
    0x62, 0xE9, 0xFF, 0x7D, 0x5E, 0xF0, 0x92, 0x00, 0xB4, 0xB6, 0x00, 0x7F, 0xBB, 0x00, 0x6B, 0xC2,
    0x3D, 0x58, 0xC8, 0x95, 0x48, 0xCD, 0xDE, 0x4F, 0x4F, 0x4F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xC0, 0xDF, 0xFF, 0xD2, 0xD2, 0xFF, 0xE8, 0xC8, 0xFF, 0xFB, 0xC2, 0xFF, 0xFF,
    0xC4, 0xEA, 0xFF, 0xCC, 0xB0, 0xF7, 0xD2, 0x96, 0xDF, 0xDF, 0x88, 0xC8, 0xE7, 0x8C, 0xC4, 0xEA,
    0xB0, 0xB2, 0xEB, 0xDC, 0xC8, 0xE7, 0xF1, 0xDF, 0xDF, 0xDF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl Ppu {
    /// Step PPU by 1 PPU cycle. Returns true if a frame was completed.
    pub fn step<B: PpuBus>(&mut self, bus: &mut B) -> bool {
        let mut frame_complete = false;

        // Render visible pixel if on visible scanlines
        if self.scanline >= 0 && self.scanline < 240 && self.cycle >= 1 && self.cycle <= 256 {
            self.render_pixel(bus);
        }

        // Internal scroll/address register updates (Loopy updates during active rendering)
        if self.rendering_enabled()
            && (self.scanline == self.timing.pre_render_scanline || (self.scanline >= 0 && self.scanline < 240))
        {
            // Increment coarse X every 8 cycles during visible/prerender for cycles 8..=248 (cycle % 8 == 0)
            if self.cycle >= 8 && self.cycle <= 248 && self.cycle % 8 == 0 {
                self.increment_coarse_x();
            }
            // Tile prefetch increments at cycles 328 and 336
            if self.cycle == 328 || self.cycle == 336 {
                self.increment_coarse_x();
            }
            // Increment Y at cycle 256
            if self.cycle == 256 {
                self.increment_y();
            }
            // Transfer X (horizontal reset) at cycle 257
            if self.cycle == 257 {
                self.transfer_x();
            }
            // Transfer Y (vertical reset) during cycles 280..=304 of pre-render scanline
            if self.scanline == self.timing.pre_render_scanline && self.cycle >= 280 && self.cycle <= 304 {
                self.transfer_y();
            }
        }

        // Clear flags at dot 1 of the pre-render scanline
        if self.scanline == self.timing.pre_render_scanline && self.cycle == 1 {
            self.status &= !0x80; // Clear VBlank flag
            self.status &= !0x40; // Clear Sprite 0 Hit
            self.status &= !0x20; // Clear Sprite Overflow
            self.nmi_asserted = false;
        }

        // Advance cycle counter
        self.cycle += 1;

        // Odd frame skip: on NTSC, on odd frames when rendering is enabled,
        // skip the last dot of the pre-render scanline
        let cycle_limit = if self.scanline == self.timing.pre_render_scanline
            && self.odd_frame
            && self.rendering_enabled()
            && self.timing.region == EmulatorRegion::Ntsc
        {
            340 // skip cycle 340 (one fewer dot)
        } else {
            341
        };

        if self.cycle >= cycle_limit {
            self.cycle = 0;
            self.scanline += 1;

            if self.scanline == 241 {
                // Start VBlank period
                self.status |= 0x80; // Set VBlank flag
                if (self.ctrl & 0x80) != 0 {
                    self.nmi_asserted = true;
                }
                frame_complete = true;
            } else if self.scanline >= self.timing.total_scanlines {
                self.scanline = 0;
                self.odd_frame = !self.odd_frame;
            }
        }

        frame_complete
    }

    /// Render a single pixel at the current scanline and cycle
    fn render_pixel<B: PpuBus>(&mut self, bus: &mut B) {
        let x = (self.cycle - 1) as usize;
        let y = self.scanline as usize;

        if x >= 256 || y >= 240 {
            return;
        }

        // Clipping mask properties of standard PPU
        let bg_clipped = (self.mask & 0x02) == 0 && x < 8;
        let sprite_clipped = (self.mask & 0x04) == 0 && x < 8;

        // 1. Compute Background pixel
        let mut bg_opaque = false;
        let mut bg_color_idx = self.palette_ram[0];

        if self.show_background() && !bg_clipped {
            // Use self.v directly — Loopy coarse-X increments already keep v
            // pointing at the correct tile. No manual v_fetch adjustment needed.
            let coarse_x = self.v & 0x001F;
            let coarse_y = (self.v & 0x03E0) >> 5;
            let fine_y = (self.v & 0x7000) >> 12;
            let nametable_select = (self.v & 0x0C00) >> 10;

            let nt_base = 0x2000 + (nametable_select * 0x400);
            let nt_addr = nt_base + (coarse_y * 32) + coarse_x;
            let tile_idx = bus.read(nt_addr);

            let pattern_base = if (self.ctrl & 0x10) != 0 {
                0x1000
            } else {
                0x0000
            };
            let pattern_addr = pattern_base + (tile_idx as u16 * 16) + fine_y;
            let low_plane = bus.read(pattern_addr);
            let high_plane = bus.read(pattern_addr + 8);

            // Use fine-X scroll register directly for bit selection
            let bit_shift = 7 - self.x;
            let pixel_low = (low_plane >> bit_shift) & 0x01;
            let pixel_high = (high_plane >> bit_shift) & 0x01;
            let color_idx = (pixel_high << 1) | pixel_low;

            if color_idx > 0 {
                bg_opaque = true;
                let attr_addr = nt_base + 0x3C0 + ((coarse_y >> 2) * 8) + (coarse_x >> 2);
                let attr_byte = bus.read(attr_addr);
                let shift = ((coarse_y & 2) << 1) | (coarse_x & 2);
                let palette_idx = (attr_byte >> shift) & 0x03;

                let pal_ram_addr = 0x3F00 + (palette_idx as u16 * 4) + color_idx as u16;
                bg_color_idx = self.palette_ram[(pal_ram_addr & 0x001F) as usize];
            }
        }

        // 2. Compute Sprite pixel (Scan OAM from 0 to 63)
        let mut sprite_opaque = false;
        let mut sprite_color_idx = 0u8;
        let mut sprite_priority = false;
        let mut is_sprite_zero = false;

        if self.show_sprites() && !sprite_clipped {
            let sprite_height = if (self.ctrl & 0x20) != 0 { 16 } else { 8 };

            // Sprite overflow detection: count sprites on this scanline
            let mut sprites_found: usize = 0;

            for i in 0..64 {
                let oam_idx = i * 4;
                let sprite_y = (self.oam_data[oam_idx] as usize) + 1;

                // A sprite is active on this scanline if Y is in range
                if y >= sprite_y && y < sprite_y + sprite_height {
                    sprites_found += 1;

                    if sprites_found > 8 {
                        // Set sprite overflow flag
                        self.status |= 0x20;
                        break; // Stop evaluating further sprites
                    }

                    let oam_tile = self.oam_data[oam_idx + 1];
                    let oam_attr = self.oam_data[oam_idx + 2];
                    let sprite_x = self.oam_data[oam_idx + 3] as usize;

                    // Check if current X is within the 8-pixel width of the sprite
                    if x >= sprite_x && x < sprite_x + 8 && !sprite_opaque {
                        // Found overlapping sprite! Resolve fine pixel offsets
                        let mut fine_y = (y - sprite_y) as u16;
                        if (oam_attr & 0x80) != 0 {
                            // Vertically flipped
                            fine_y = (sprite_height as u16 - 1) - fine_y;
                        }

                        let mut pattern_base = if (self.ctrl & 0x08) != 0 {
                            0x1000
                        } else {
                            0x0000
                        };
                        let mut tile_idx = oam_tile as u16;

                        if sprite_height == 16 {
                            // 8x16 mode
                            pattern_base = if (oam_tile & 0x01) != 0 {
                                0x1000
                            } else {
                                0x0000
                            };
                            let mut tile = oam_tile & 0xFE;
                            if fine_y >= 8 {
                                fine_y -= 8;
                                if (oam_attr & 0x80) == 0 {
                                    tile += 1;
                                }
                            } else if (oam_attr & 0x80) != 0 {
                                tile += 1;
                            }
                            tile_idx = tile as u16;
                        }

                        let pattern_addr = pattern_base + (tile_idx * 16) + fine_y;
                        let low_plane = bus.read(pattern_addr);
                        let high_plane = bus.read(pattern_addr + 8);

                        let mut bit_shift = 7 - (x - sprite_x) as u16;
                        if (oam_attr & 0x40) != 0 {
                            // Horizontally flipped
                            bit_shift = (x - sprite_x) as u16;
                        }

                        let pixel_low = (low_plane >> bit_shift) & 0x01;
                        let pixel_high = (high_plane >> bit_shift) & 0x01;
                        let color_idx = (pixel_high << 1) | pixel_low;

                        if color_idx > 0 {
                            // Opaque sprite pixel!
                            sprite_opaque = true;
                            let palette_idx = oam_attr & 0x03;
                            let pal_ram_addr = 0x3F10 + (palette_idx as u16 * 4) + color_idx as u16;
                            sprite_color_idx = self.palette_ram[(pal_ram_addr & 0x001F) as usize];
                            sprite_priority = (oam_attr & 0x20) != 0;
                            is_sprite_zero = i == 0;
                            // Don't break — continue counting sprites for overflow detection
                        }
                    }
                }
            }
        }

        // 3. Blend Background & Sprite Pixels
        let final_color_idx = match (bg_opaque, sprite_opaque) {
            (false, false) => self.palette_ram[0],
            (true, false) => bg_color_idx,
            (false, true) => sprite_color_idx,
            (true, true) => {
                // Sprite-0 Hit Detection:
                if is_sprite_zero && x < 255 {
                    self.status |= 0x40; // Set Sprite 0 Hit!
                }

                if sprite_priority {
                    bg_color_idx
                } else {
                    sprite_color_idx
                }
            }
        };

        // Apply grayscale if PPUMASK bit 0 is set
        let final_color_idx = if (self.mask & 0x01) != 0 {
            final_color_idx & 0x30
        } else {
            final_color_idx
        };

        // Resolve final RGB color from palette index
        let pal_idx = (final_color_idx & 0x3F) as usize;
        let r = NES_PALETTE[pal_idx * 3];
        let g = NES_PALETTE[pal_idx * 3 + 1];
        let b = NES_PALETTE[pal_idx * 3 + 2];

        let fb_idx = (y * 256 + x) * 4;
        self.frame_buffer[fb_idx] = r;
        self.frame_buffer[fb_idx + 1] = g;
        self.frame_buffer[fb_idx + 2] = b;
        self.frame_buffer[fb_idx + 3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bus::{MirroringMode, PpuBus};
    use crate::core::region::NTSC_TIMING;

    // ── Mock PPU Bus ──────────────────────────────────────────────────
    // Stores pattern tables in a 8KB array and nametable/attribute in a 2KB array.
    struct MockPpuBus {
        pattern: [u8; 0x2000],  // $0000-$1FFF
        nametable: [u8; 0x0800], // $2000-$27FF (mirrored)
    }

    impl MockPpuBus {
        fn new() -> Self {
            Self {
                pattern: [0; 0x2000],
                nametable: [0; 0x0800],
            }
        }
    }

    impl PpuBus for MockPpuBus {
        fn read(&mut self, addr: u16) -> u8 {
            let addr = addr & 0x3FFF;
            if addr < 0x2000 {
                self.pattern[addr as usize]
            } else if addr < 0x3F00 {
                let mirrored = ((addr - 0x2000) & 0x07FF) as usize;
                self.nametable[mirrored]
            } else {
                0
            }
        }

        fn write(&mut self, addr: u16, val: u8) {
            let addr = addr & 0x3FFF;
            if addr < 0x2000 {
                self.pattern[addr as usize] = val;
            } else if addr < 0x3F00 {
                let mirrored = ((addr - 0x2000) & 0x07FF) as usize;
                self.nametable[mirrored] = val;
            }
        }

        fn set_mirroring(&mut self, _mode: MirroringMode) {}
    }

    /// Helper: create a PPU positioned on a visible scanline/cycle with rendering enabled.
    fn make_render_ppu() -> Ppu {
        let mut ppu = Ppu::new();
        // Enable background + sprites (PPUMASK bits 3,4) and show left column (bits 1,2)
        ppu.mask = 0x1E;
        // Position on visible scanline 100, cycle 1 (first visible pixel)
        ppu.scanline = 100;
        ppu.cycle = 1;
        // Default palette: entry 0 = color 0x0F (black)
        ppu.palette_ram[0] = 0x0F;
        ppu
    }

    /// Place a sprite in OAM at the given slot.
    fn place_sprite(ppu: &mut Ppu, slot: usize, y: u8, tile: u8, attr: u8, x: u8) {
        let base = slot * 4;
        ppu.oam_data[base] = y;       // Y position (sprite appears at y+1)
        ppu.oam_data[base + 1] = tile; // Tile index
        ppu.oam_data[base + 2] = attr; // Attributes
        ppu.oam_data[base + 3] = x;    // X position
    }

    // ── Sprite Evaluation: 8-sprite limit per scanline ────────────────

    #[test]
    fn test_sprite_overflow_flag_set_on_ninth_sprite() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place 9 sprites all visible on scanline 100.
        // OAM Y value = scanline - 1 = 99 (sprite appears at Y+1 = 100).
        for i in 0..9 {
            place_sprite(&mut ppu, i, scanline - 1, 0, 0, 0);
        }
        // Make all sprite tiles opaque (all 1s in pattern planes)
        // Tile 0 at pattern base 0x0000: low plane at fine_y=0 -> addr 0x0000, high plane at 0x0008
        bus.pattern[0x0000] = 0xFF;
        bus.pattern[0x0008] = 0xFF;
        // Fill sprite palette so opaque pixels have a visible color
        // Sprite palette: base 0x3F10, pal 0, color 3 -> addr 0x3F13, index = 0x13 & 0x1F = 0x13
        ppu.palette_ram[0x13] = 0x15;

        // Ensure overflow flag is clear before rendering
        assert_eq!(ppu.status & 0x20, 0, "overflow flag should be clear initially");

        ppu.render_pixel(&mut bus);

        // After rendering, the 9th sprite should trigger overflow
        assert_ne!(ppu.status & 0x20, 0, "overflow flag should be set after 9 sprites");
    }

    #[test]
    fn test_no_sprite_overflow_with_eight_or_fewer() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place exactly 8 sprites on the scanline
        for i in 0..8 {
            place_sprite(&mut ppu, i, scanline - 1, 0, 0, 0);
        }
        bus.pattern[0x0000] = 0xFF;
        bus.pattern[0x0008] = 0xFF;
        ppu.palette_ram[0x13] = 0x15;

        ppu.render_pixel(&mut bus);

        assert_eq!(ppu.status & 0x20, 0, "overflow flag should NOT be set with only 8 sprites");
    }

    // ── Sprite-0 Hit Detection ────────────────────────────────────────

    #[test]
    fn test_sprite_zero_hit_when_bg_and_sprite0_opaque() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place sprite 0 at x=0 on the current scanline
        // Use tile 2 for sprite so it doesn't collide with BG tile
        place_sprite(&mut ppu, 0, scanline - 1, 2, 0, 0);

        // Make sprite tile 2 opaque
        // Sprite pattern base: ctrl & 0x08 = 0 -> base 0x0000
        // Tile 2 pattern: addr = 0x0000 + 2*16 = 0x0020
        bus.pattern[0x0020] = 0xFF; // low plane at fine_y=0
        bus.pattern[0x0028] = 0xFF; // high plane at fine_y=0
        // Sprite palette: base 0x3F10, palette 0, color index 3 -> 0x3F13
        ppu.palette_ram[0x13] = 0x15;

        // Make background opaque using tile 1
        ppu.v = 0; // coarse_x=0, coarse_y=0, fine_y=0, nt=0
        bus.nametable[0] = 1; // tile index 1
        // BG pattern base: ctrl & 0x10 = 0 -> base 0x0000
        // Tile 1 pattern: addr = 0x0000 + 1*16 = 0x0010
        bus.pattern[0x0010] = 0xFF; // low plane
        bus.pattern[0x0018] = 0xFF; // high plane
        // Attribute byte: nt_base(0x2000) + 0x3C0 = 0x23C0. Mock nametable index = 0x3C0
        bus.nametable[0x3C0] = 0x00; // palette group 0
        // BG palette group 0, color index 3 -> palette_ram addr = 0x3F00 + 0*4 + 3 = 0x3F03 -> index 3
        ppu.palette_ram[3] = 0x20;

        // Clear sprite 0 hit flag
        ppu.status &= !0x40;

        ppu.render_pixel(&mut bus);

        assert_ne!(ppu.status & 0x40, 0, "Sprite 0 hit flag should be set");
    }

    #[test]
    fn test_sprite_zero_hit_not_set_at_x_255() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place sprite 0 at x=248, so pixel x=255 is within sprite range (248..256)
        place_sprite(&mut ppu, 0, scanline - 1, 2, 0, 248);

        bus.pattern[0x0020] = 0xFF;
        bus.pattern[0x0028] = 0xFF;
        ppu.palette_ram[0x13] = 0x15;

        // Set up opaque BG
        ppu.v = 0;
        bus.nametable[0] = 1;
        bus.pattern[0x0010] = 0xFF;
        bus.pattern[0x0018] = 0xFF;
        bus.nametable[0x3C0] = 0x00;
        ppu.palette_ram[3] = 0x20;

        // Position at cycle 256 -> pixel x = 255
        ppu.cycle = 256;
        ppu.status &= !0x40;

        ppu.render_pixel(&mut bus);

        // Sprite-0 hit should NOT trigger at x=255 (the code checks x < 255)
        assert_eq!(ppu.status & 0x40, 0, "Sprite 0 hit should not trigger at x=255");
    }

    // ── Background/Sprite Priority ────────────────────────────────────

    #[test]
    fn test_sprite_behind_bg_priority() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place sprite 0 with priority bit set (behind BG): attr bit 5 = 0x20
        place_sprite(&mut ppu, 0, scanline - 1, 2, 0x20, 0);

        // Opaque sprite (tile 2)
        bus.pattern[0x0020] = 0xFF;
        bus.pattern[0x0028] = 0xFF;
        let sprite_color: u8 = 0x16;
        ppu.palette_ram[0x13] = sprite_color; // sprite palette: pal 0, color 3 -> index 0x13

        // Opaque BG (tile 1)
        ppu.v = 0;
        bus.nametable[0] = 1;
        bus.pattern[0x0010] = 0xFF;
        bus.pattern[0x0018] = 0xFF;
        bus.nametable[0x3C0] = 0x00; // palette group 0
        let bg_color: u8 = 0x30;
        ppu.palette_ram[3] = bg_color; // BG palette group 0, color 3 -> index 3

        ppu.render_pixel(&mut bus);

        // With priority bit set and both opaque, BG should win
        let fb_idx = (ppu.scanline as usize * 256 + 0) * 4;
        let rendered_r = ppu.frame_buffer[fb_idx];
        let bg_r = NES_PALETTE[(bg_color & 0x3F) as usize * 3];
        assert_eq!(rendered_r, bg_r, "BG pixel should be displayed when sprite has behind-BG priority");
    }

    #[test]
    fn test_sprite_in_front_of_bg_priority() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        let scanline = ppu.scanline as u8;
        // Place sprite 0 with priority=0 (in front of BG): attr = 0x00
        place_sprite(&mut ppu, 0, scanline - 1, 2, 0x00, 0);

        // Opaque sprite (tile 2)
        bus.pattern[0x0020] = 0xFF;
        bus.pattern[0x0028] = 0xFF;
        let sprite_color: u8 = 0x16;
        ppu.palette_ram[0x13] = sprite_color; // sprite palette: pal 0, color 3 -> index 0x13

        // Opaque BG (tile 1)
        ppu.v = 0;
        bus.nametable[0] = 1;
        bus.pattern[0x0010] = 0xFF;
        bus.pattern[0x0018] = 0xFF;
        bus.nametable[0x3C0] = 0x00; // palette group 0
        let bg_color: u8 = 0x30;
        ppu.palette_ram[3] = bg_color; // BG palette group 0, color 3 -> index 3

        ppu.render_pixel(&mut bus);

        // With priority = 0 and both opaque, sprite should win
        let fb_idx = (ppu.scanline as usize * 256 + 0) * 4;
        let rendered_r = ppu.frame_buffer[fb_idx];
        let sprite_r = NES_PALETTE[(sprite_color & 0x3F) as usize * 3];
        assert_eq!(rendered_r, sprite_r, "Sprite pixel should be displayed when sprite has in-front priority");
    }

    // ── Fine X Scroll Pixel Shifting ──────────────────────────────────

    #[test]
    fn test_fine_x_scroll_shifts_bg_pixel() {
        let mut ppu = make_render_ppu();
        let mut bus = MockPpuBus::new();

        // Disable sprites, enable BG + show left column
        ppu.mask = 0x0A; // bits 1 (show left BG) + 3 (show BG)

        // Set up a BG tile where only bit 7 of low plane is set (leftmost pixel)
        ppu.v = 0;
        bus.nametable[0] = 1;
        bus.pattern[0x0010] = 0x80; // low plane: only pixel 0 opaque
        bus.pattern[0x0018] = 0x00; // high plane: 0
        bus.nametable[0x3C0] = 0x00;
        ppu.palette_ram[1] = 0x15; // color index 1 -> palette color

        // fine X = 0: bit_shift = 7 - 0 = 7, so bit 7 is selected -> opaque
        ppu.x = 0;
        ppu.render_pixel(&mut bus);
        let fb_idx = (ppu.scanline as usize * 256) * 4;
        let px0_r = ppu.frame_buffer[fb_idx];

        // fine X = 1: bit_shift = 7 - 1 = 6, bit 6 is 0 -> transparent (backdrop color)
        ppu.x = 1;
        ppu.cycle = 2; // advance to next pixel
        ppu.render_pixel(&mut bus);
        let fb_idx2 = (ppu.scanline as usize * 256 + 1) * 4;
        let px1_r = ppu.frame_buffer[fb_idx2];

        let backdrop_r = NES_PALETTE[(ppu.palette_ram[0] & 0x3F) as usize * 3];
        let opaque_r = NES_PALETTE[(0x15u8 & 0x3F) as usize * 3];

        assert_eq!(px0_r, opaque_r, "fine_x=0 should select the opaque bit 7");
        assert_eq!(px1_r, backdrop_r, "fine_x=1 should select transparent bit 6 (backdrop)");
    }

    // ── Scanline Counter Behavior ─────────────────────────────────────

    #[test]
    fn test_scanline_increments_after_341_cycles() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        ppu.scanline = 0;
        ppu.cycle = 0;

        // Step through 341 PPU cycles (one full scanline)
        for _ in 0..341 {
            ppu.step(&mut bus);
        }

        assert_eq!(ppu.scanline, 1, "scanline should advance to 1 after 341 cycles");
        assert_eq!(ppu.cycle, 0, "cycle should reset to 0");
    }

    #[test]
    fn test_vblank_flag_set_at_scanline_241() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        // Position just before scanline 241
        ppu.scanline = 240;
        ppu.cycle = 340; // last cycle of scanline 240

        let frame_complete = ppu.step(&mut bus);

        assert_eq!(ppu.scanline, 241, "should now be at scanline 241");
        assert_ne!(ppu.status & 0x80, 0, "VBlank flag should be set");
        assert!(frame_complete, "frame_complete should be true");
    }

    #[test]
    fn test_nmi_asserted_when_ctrl_nmi_enabled() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        ppu.ctrl = 0x80; // NMI enable
        ppu.scanline = 240;
        ppu.cycle = 340;

        ppu.step(&mut bus);

        assert!(ppu.nmi_asserted, "NMI should be asserted when PPUCTRL NMI is enabled and VBlank starts");
    }

    #[test]
    fn test_nmi_not_asserted_when_ctrl_nmi_disabled() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        ppu.ctrl = 0x00; // NMI disabled
        ppu.scanline = 240;
        ppu.cycle = 340;

        ppu.step(&mut bus);

        assert!(!ppu.nmi_asserted, "NMI should NOT be asserted when PPUCTRL NMI is disabled");
    }

    #[test]
    fn test_prerender_scanline_clears_flags() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        // Set all flags that should be cleared
        ppu.status = 0x80 | 0x40 | 0x20; // VBlank + Sprite0 Hit + Overflow
        ppu.nmi_asserted = true;

        // Position at pre-render scanline, cycle 0.
        // The flag-clear check runs BEFORE the cycle increment (checks cycle==1),
        // so we need to step twice: step at cycle=0 increments to 1,
        // step at cycle=1 sees the check and clears flags.
        ppu.scanline = NTSC_TIMING.pre_render_scanline;
        ppu.cycle = 0;

        ppu.step(&mut bus); // cycle 0 -> processes cycle 0, increments to 1
        ppu.step(&mut bus); // cycle 1 -> flag clear triggers, increments to 2

        // After cycle 1 of pre-render scanline, flags should be cleared
        assert_eq!(ppu.status & 0x80, 0, "VBlank flag should be cleared");
        assert_eq!(ppu.status & 0x40, 0, "Sprite 0 Hit should be cleared");
        assert_eq!(ppu.status & 0x20, 0, "Sprite Overflow should be cleared");
        assert!(!ppu.nmi_asserted, "NMI asserted should be cleared");
    }

    #[test]
    fn test_odd_frame_toggles_on_new_frame() {
        let mut ppu = Ppu::new();
        let mut bus = MockPpuBus::new();

        // Start at the last cycle of the last scanline before wrap
        ppu.scanline = NTSC_TIMING.total_scanlines - 1;
        ppu.cycle = 340;
        ppu.odd_frame = false;

        ppu.step(&mut bus);

        assert_eq!(ppu.scanline, 0, "scanline should wrap to 0");
        assert!(ppu.odd_frame, "odd_frame should toggle to true");
    }
}
