#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirroringMode {
    Horizontal,
    Vertical,
    SingleScreenLower,
    SingleScreenUpper,
    FourScreen,
}

/// Interface contract between the CPU and the system bus.
pub trait CpuBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn poll_nmi(&mut self) -> bool;
    fn poll_irq(&mut self) -> bool;
    fn clear_nmi(&mut self);
}

/// Interface contract between the PPU and the visual memory bus.
pub trait PpuBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn set_mirroring(&mut self, mode: MirroringMode);
}

use crate::core::apu::Apu;
use crate::core::cartridge::Cartridge;
use crate::core::ppu::Ppu;

pub struct SimpleBus {
    pub mem: [u8; 65536],
    pub cartridge: Option<Cartridge>,
    pub ppu: Ppu,
    pub apu: Apu,
    pub vram: [u8; 2048],
    pub controller_state: u8,
    pub controller_latch: u8,
    pub controller_shift: u8,
    pub controller2_state: u8,
    pub controller2_shift: u8,
    pub ppu_frame_complete: bool,
    pub ppu_ticked_cycles: u32,
}

pub struct SimplePpuBus<'a> {
    pub cartridge: &'a mut Option<Cartridge>,
    pub vram: &'a mut [u8; 2048],
}

impl<'a> SimplePpuBus<'a> {
    pub fn mirror_nametable_addr(&self, addr: u16) -> u16 {
        let addr = (addr - 0x2000) & 0x0FFF;
        let mirroring = if let Some(ref cart) = self.cartridge {
            cart.mapper.mirroring().unwrap_or(cart.mirroring)
        } else {
            MirroringMode::Horizontal
        };

        match mirroring {
            MirroringMode::Horizontal => {
                if addr < 0x0800 {
                    addr & 0x03FF
                } else {
                    0x0400 + (addr & 0x03FF)
                }
            }
            MirroringMode::Vertical => addr & 0x07FF,
            MirroringMode::SingleScreenLower => addr & 0x03FF,
            MirroringMode::SingleScreenUpper => 0x0400 + (addr & 0x03FF),
            MirroringMode::FourScreen => addr & 0x07FF,
        }
    }
}

impl<'a> PpuBus for SimplePpuBus<'a> {
    fn read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF;
        if addr < 0x2000 {
            if let Some(ref cart) = self.cartridge {
                cart.read_ppu(addr)
            } else {
                0
            }
        } else if addr < 0x3F00 {
            let mirrored = self.mirror_nametable_addr(addr);
            self.vram[mirrored as usize]
        } else {
            0
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        let addr = addr & 0x3FFF;
        if addr < 0x2000 {
            if let Some(ref mut cart) = self.cartridge {
                cart.write_ppu(addr, val);
            }
        } else if addr < 0x3F00 {
            let mirrored = self.mirror_nametable_addr(addr);
            self.vram[mirrored as usize] = val;
        }
    }

    fn set_mirroring(&mut self, _mode: MirroringMode) {}
}

impl Default for SimpleBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleBus {
    pub fn new() -> Self {
        Self {
            mem: [0; 65536],
            cartridge: None,
            ppu: Ppu::new(),
            apu: Apu::new(),
            vram: [0; 2048],
            controller_state: 0,
            controller_latch: 0,
            controller_shift: 0,
            controller2_state: 0,
            controller2_shift: 0,
            ppu_frame_complete: false,
            ppu_ticked_cycles: 0,
        }
    }

    pub fn load_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    pub fn tick_ppu(&mut self, cycles: u32) {
        self.ppu_ticked_cycles += cycles;
        for _ in 0..cycles {
            let mut ppu_bus = SimplePpuBus {
                cartridge: &mut self.cartridge,
                vram: &mut self.vram,
            };
            if self.ppu.step(&mut ppu_bus) {
                self.ppu_frame_complete = true;
            }
        }
    }
}

impl CpuBus for SimpleBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.tick_ppu(3);
        match addr {
            0x2000..=0x3FFF => {
                let mut ppu_bus = SimplePpuBus {
                    cartridge: &mut self.cartridge,
                    vram: &mut self.vram,
                };
                self.ppu.read_reg(addr, &mut ppu_bus)
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read_reg(addr),
            0x4016 => {
                if self.controller_latch == 1 {
                    self.controller_shift = self.controller_state;
                }
                let bit = (self.controller_shift & 0x01) | 0x40;
                if self.controller_latch == 0 {
                    self.controller_shift = (self.controller_shift >> 1) | 0x80;
                }
                bit
            }
            0x4017 => {
                if self.controller_latch == 1 {
                    self.controller2_shift = self.controller2_state;
                }
                let bit = (self.controller2_shift & 0x01) | 0x40;
                if self.controller_latch == 0 {
                    self.controller2_shift = (self.controller2_shift >> 1) | 0x80;
                }
                bit
            }
            0x4020..=0xFFFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.read_cpu(addr)
                } else {
                    self.mem[addr as usize]
                }
            }
            0x0000..=0x1FFF => self.mem[(addr & 0x07FF) as usize],
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.tick_ppu(3);
        match addr {
            0x2000..=0x3FFF => {
                let mut ppu_bus = SimplePpuBus {
                    cartridge: &mut self.cartridge,
                    vram: &mut self.vram,
                };
                self.ppu.write_reg(addr, val, &mut ppu_bus);
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => {
                self.apu.write_reg(addr, val);
            }
            0x4014 => {
                let page_addr = (val as u16) << 8;
                let mut dma_data = [0u8; 256];
                for i in 0..256 {
                    dma_data[i] = self.read(page_addr + i as u16);
                }
                self.ppu.write_oam_dma(&dma_data);
            }
            0x4016 => {
                self.controller_latch = val & 0x01;
                if self.controller_latch == 1 {
                    self.controller_shift = self.controller_state;
                    self.controller2_shift = self.controller2_state;
                }
            }
            0x4020..=0xFFFF => {
                if let Some(ref mut cart) = self.cartridge {
                    cart.write_cpu(addr, val);
                } else {
                    self.mem[addr as usize] = val;
                }
            }
            0x0000..=0x1FFF => {
                self.mem[(addr & 0x07FF) as usize] = val;
            }
            _ => {}
        }
    }

    fn poll_nmi(&mut self) -> bool {
        let res = self.ppu.nmi_asserted;
        self.ppu.nmi_asserted = false;
        res
    }

    fn poll_irq(&mut self) -> bool {
        self.apu.poll_irq()
    }

    fn clear_nmi(&mut self) {
        self.ppu.nmi_asserted = false;
    }
}
