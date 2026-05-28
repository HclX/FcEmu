#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulatorRegion {
    Ntsc,
    Pal,
}

#[derive(Clone, Copy)]
pub struct TimingSpec {
    pub region: EmulatorRegion,
    pub pre_render_scanline: i16,
    pub vblank_start_scanline: i16,
    pub total_scanlines: i16,
    pub cpu_clock_speed: f64,
    pub ppu_accum_mult: u32,
    pub ppu_accum_div: u32,
    pub apu_4_step_rates: [u32; 4],
    pub apu_5_step_rates: [u32; 5],
    pub noise_period_table: [u16; 16],
    pub dmc_rate_table: [u16; 16],
}

pub static NTSC_TIMING: TimingSpec = TimingSpec {
    region: EmulatorRegion::Ntsc,
    pre_render_scanline: 261,
    vblank_start_scanline: 241,
    total_scanlines: 262,
    cpu_clock_speed: 1789773.0,
    ppu_accum_mult: 3,
    ppu_accum_div: 1,
    apu_4_step_rates: [7457, 7456, 7458, 7458],
    apu_5_step_rates: [7457, 7456, 7458, 7458, 7452],
    noise_period_table: [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ],
    dmc_rate_table: [
        428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
    ],
};

pub static PAL_TIMING: TimingSpec = TimingSpec {
    region: EmulatorRegion::Pal,
    pre_render_scanline: 311,
    vblank_start_scanline: 241,
    total_scanlines: 312,
    cpu_clock_speed: 1661925.0,
    ppu_accum_mult: 16,
    ppu_accum_div: 5,
    apu_4_step_rates: [8313, 8314, 8314, 8314],
    apu_5_step_rates: [8313, 8314, 8314, 8314, 8310],
    noise_period_table: [
        4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
    ],
    dmc_rate_table: [
        398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
    ],
};
