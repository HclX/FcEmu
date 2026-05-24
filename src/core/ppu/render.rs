use super::Ppu;
use crate::core::bus::PpuBus;

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
            // Increment Y at cycle 256
            if self.cycle == 256 {
                self.increment_y();
            }
            // Transfer X (horizontal reset) at cycle 257
            if self.cycle == 257 {
                self.transfer_x();
            }
            // Transfer Y (vertical reset) during cycles 280..=304 of pre-render scanline 261
            if self.scanline == self.timing.pre_render_scanline && self.cycle >= 280 && self.cycle <= 304 {
                self.transfer_y();
            }
        }

        // Timings & cycle updates
        self.cycle += 1;
        if self.scanline == self.timing.pre_render_scanline && self.cycle == 1 {
            self.status &= !0x40; // Clear Sprite 0 Hit
        }

        if self.cycle >= 341 {
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
                // Pre-render scanline complete, clear flags
                self.status &= !0x80; // Clear VBlank flag
                self.status &= !0x20; // Clear Sprite Overflow
                self.nmi_asserted = false;
                if self.rendering_enabled() {
                    self.v = self.t;
                }
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
            let mut v_fetch = self.v;
            // Check if the fine X scroll boundary is crossed (i.e. pixel lies in the next tile coarse X + 1)
            if (x as u16 & 0x07) + self.x as u16 >= 8 {
                if (v_fetch & 0x001F) == 31 {
                    v_fetch &= !0x001F;
                    v_fetch ^= 0x0400; // Switch horizontal nametable
                } else {
                    v_fetch += 1;
                }
            }
            let coarse_x = v_fetch & 0x001F;
            let coarse_y = (v_fetch & 0x03E0) >> 5;
            let fine_y = (v_fetch & 0x7000) >> 12;

            let nametable_select = (v_fetch & 0x0C00) >> 10;
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

            let bit_shift = 7 - ((x as u16 + self.x as u16) & 0x07);
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
        let mut sprite_color_idx = 0;
        let mut sprite_priority = false;
        let mut is_sprite_zero = false;

        if self.show_sprites() && !sprite_clipped {
            let sprite_height = if (self.ctrl & 0x20) != 0 { 16 } else { 8 };

            for i in 0..64 {
                let oam_idx = i * 4;
                let sprite_y = (self.oam_data[oam_idx] as usize) + 1;

                // A sprite is active on this scanline if Y is in range
                if y >= sprite_y && y < sprite_y + sprite_height {
                    let oam_tile = self.oam_data[oam_idx + 1];
                    let oam_attr = self.oam_data[oam_idx + 2];
                    let sprite_x = self.oam_data[oam_idx + 3] as usize;

                    // Check if current X is within the 8-pixel width of the sprite
                    if x >= sprite_x && x < sprite_x + 8 {
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

                            // Standard lower index has priority, stop scanning other sprites!
                            break;
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
