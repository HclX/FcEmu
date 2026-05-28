use crate::core::region::{TimingSpec, NTSC_TIMING};

const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

// ──────────────────────────── Envelope Unit ────────────────────────────

#[derive(Default, Clone)]
pub struct Envelope {
    pub start: bool,
    pub divider: u8,
    pub decay_level: u8,
    pub volume: u8,
    pub constant_volume: bool,
    pub loop_flag: bool,
}

impl Envelope {
    pub fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay_level = 15;
            self.divider = self.volume;
        } else if self.divider > 0 {
            self.divider -= 1;
        } else {
            self.divider = self.volume;
            if self.decay_level > 0 {
                self.decay_level -= 1;
            } else if self.loop_flag {
                self.decay_level = 15;
            }
        }
    }

    pub fn output(&self) -> u8 {
        if self.constant_volume {
            self.volume
        } else {
            self.decay_level
        }
    }
}

// ──────────────────────────── Sweep Unit ────────────────────────────

#[derive(Default, Clone)]
pub struct Sweep {
    pub enabled: bool,
    pub divider: u8,
    pub negate: bool,
    pub shift: u8,
    pub reload: bool,
    pub period: u8,
    pub is_channel_1: bool,
}

impl Sweep {
    /// Compute the target period given the current timer period.
    fn target_period(&self, timer_period: u16) -> u16 {
        let change = timer_period >> self.shift;
        if self.negate {
            if self.is_channel_1 {
                // Pulse 1: one's complement (subtract change + 1, i.e. bitwise NOT)
                timer_period.wrapping_sub(change.wrapping_add(1))
            } else {
                // Pulse 2: two's complement (subtract change)
                timer_period.wrapping_sub(change)
            }
        } else {
            timer_period.wrapping_add(change)
        }
    }

    /// Returns true if the channel should be muted (target period > 0x7FF or current period < 8).
    pub fn muting(&self, timer_period: u16) -> bool {
        timer_period < 8 || self.target_period(timer_period) > 0x7FF
    }

    /// Clock the sweep unit. Returns the (possibly updated) timer period.
    pub fn clock(&mut self, timer_period: u16) -> u16 {
        let mut new_period = timer_period;
        if self.divider == 0 && self.enabled && !self.muting(timer_period) && self.shift > 0 {
            new_period = self.target_period(timer_period);
        }
        if self.divider == 0 || self.reload {
            self.divider = self.period;
            self.reload = false;
        } else {
            self.divider -= 1;
        }
        new_period
    }
}

// ──────────────────────────── Pulse Channel ────────────────────────────

#[derive(Default, Clone)]
pub struct PulseChannel {
    pub enabled: bool,
    pub duty: u8,
    pub constant_volume: bool,
    pub volume: u8,
    pub timer_period: u16,
    pub timer: u16,
    pub duty_step: u8,
    pub length_counter: u8,
    pub length_counter_halt: bool,
    pub envelope: Envelope,
    pub sweep: Sweep,
}

impl PulseChannel {
    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            if self.timer > 0 {
                self.timer -= 1;
            } else {
                self.timer = self.timer_period;
                self.duty_step = (self.duty_step + 1) & 7;
            }
        }
    }

    pub fn sample(&self) -> f32 {
        if !self.enabled
            || self.length_counter == 0
            || self.sweep.muting(self.timer_period)
        {
            return 0.0;
        }
        let sequence = match self.duty {
            0 => 0b01000000,
            1 => 0b01100000,
            2 => 0b01111000,
            3 => 0b10011111,
            _ => 0,
        };
        let bit = (sequence >> (7 - self.duty_step)) & 1;
        if bit != 0 {
            self.envelope.output() as f32
        } else {
            0.0
        }
    }
}

// ──────────────────────────── Triangle Channel ────────────────────────────

#[derive(Default, Clone)]
pub struct TriangleChannel {
    pub enabled: bool,
    pub control_flag: bool,
    pub linear_counter_reload: u8,
    pub linear_counter: u8,
    pub linear_counter_reload_flag: bool,
    pub timer_period: u16,
    pub timer: u16,
    pub step: u8,
    pub length_counter: u8,
}

impl TriangleChannel {
    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            if self.timer > 0 {
                self.timer -= 1;
            } else {
                self.timer = self.timer_period;
                if self.linear_counter > 0 && self.length_counter > 0 {
                    self.step = (self.step + 1) & 31;
                }
            }
        }
    }

    pub fn sample(&self) -> f32 {
        if !self.enabled
            || self.length_counter == 0
            || self.linear_counter == 0
            || self.timer_period < 2
        {
            return 0.0;
        }
        let val = if self.step < 16 {
            15 - self.step
        } else {
            self.step - 16
        };
        val as f32
    }
}

// ──────────────────────────── Noise Channel ────────────────────────────

#[derive(Clone)]
pub struct NoiseChannel {
    pub enabled: bool,
    pub constant_volume: bool,
    pub volume: u8,
    pub loop_noise: bool,
    pub timer_period: u16,
    pub timer: u16,
    pub shift_register: u16,
    pub length_counter: u8,
    pub length_counter_halt: bool,
    pub envelope: Envelope,
}

impl Default for NoiseChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            constant_volume: false,
            volume: 0,
            loop_noise: false,
            timer_period: 0,
            timer: 0,
            shift_register: 1,
            length_counter: 0,
            length_counter_halt: false,
            envelope: Envelope::default(),
        }
    }
}

impl NoiseChannel {
    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            if self.timer > 0 {
                self.timer -= 1;
            } else {
                self.timer = self.timer_period;
                let other_bit = if self.loop_noise { 6 } else { 1 };
                let feedback = (self.shift_register & 1) ^ ((self.shift_register >> other_bit) & 1);
                self.shift_register = (self.shift_register >> 1) | (feedback << 14);
            }
        }
    }

    pub fn sample(&self) -> f32 {
        if !self.enabled || self.length_counter == 0 {
            return 0.0;
        }
        if (self.shift_register & 1) != 0 {
            0.0
        } else {
            self.envelope.output() as f32
        }
    }
}

// ──────────────────────────── APU ────────────────────────────

#[derive(Clone)]
pub struct Apu {
    pub pulse1: PulseChannel,
    pub pulse2: PulseChannel,
    pub triangle: TriangleChannel,
    pub noise: NoiseChannel,
    pub sample_buffer: Vec<f32>,
    pub cycle_accumulator: f64,
    pub prev_input: f32,
    pub prev_output: f32,
    pub prev_lpf_output: f32,
    pub frame_counter_cycle: u32,
    pub frame_counter_step: u8,
    pub frame_counter_mode: u8,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub frame_counter_write_pending: bool,
    pub frame_counter_write_val: u8,
    pub frame_counter_write_delay: i32,
    pub dmc_enabled: bool,
    pub irq_assertion_cycles: u32,
    pub cpu_cycle_count: u64,
    pub dmc_active: bool,
    pub dmc_bytes_remaining: u32,
    pub dmc_rate: u16,
    pub dmc_irq_enable: bool,
    pub dmc_loop: bool,
    pub dmc_sample_length: u16,
    pub dmc_irq_pending: bool,
    pub dmc_cycle_counter: u32,
    pub dmc_output_level: u8,
    pub frame_counter_last_val: u8,
    pub timing: TimingSpec,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        let mut pulse1 = PulseChannel::default();
        pulse1.sweep.is_channel_1 = true;
        Self {
            pulse1,
            pulse2: PulseChannel::default(),
            triangle: TriangleChannel::default(),
            noise: NoiseChannel::default(),
            sample_buffer: Vec::new(),
            cycle_accumulator: 0.0,
            prev_input: 0.0,
            prev_output: 0.0,
            prev_lpf_output: 0.0,
            frame_counter_cycle: 0,
            frame_counter_step: 0,
            frame_counter_mode: 0,
            irq_enabled: true,
            irq_pending: false,
            frame_counter_write_pending: false,
            frame_counter_write_val: 0,
            frame_counter_write_delay: 0,
            irq_assertion_cycles: 0,
            dmc_enabled: false,
            cpu_cycle_count: 0,
            dmc_active: false,
            dmc_bytes_remaining: 0,
            dmc_rate: 428,
            dmc_irq_enable: false,
            dmc_loop: false,
            dmc_sample_length: 0,
            dmc_irq_pending: false,
            dmc_cycle_counter: 0,
            dmc_output_level: 0,
            frame_counter_last_val: 0x00,
            timing: NTSC_TIMING,
        }
    }

    pub fn set_region(&mut self, timing: TimingSpec) {
        self.timing = timing;
    }

    pub fn reset(&mut self) {
        self.pulse1.length_counter = 0;
        self.pulse1.enabled = false;

        self.pulse2.length_counter = 0;
        self.pulse2.enabled = false;

        self.triangle.length_counter = 0;
        self.triangle.enabled = false;

        self.noise.length_counter = 0;
        self.noise.enabled = false;

        let last_val = self.frame_counter_last_val;
        self.write_reg(0x4017, last_val);
        self.irq_pending = false;
        self.dmc_enabled = false;
        self.dmc_active = false;
        self.dmc_bytes_remaining = 0;
        self.dmc_irq_pending = false;
        self.dmc_cycle_counter = 0;
    }

    // Clock the Quarter Frame (envelopes + triangle linear counter)
    fn clock_quarter_frame(&mut self) {
        // Envelope clocking
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();

        // Triangle linear counter with proper reload logic
        if self.triangle.linear_counter_reload_flag {
            self.triangle.linear_counter = self.triangle.linear_counter_reload;
        } else if self.triangle.linear_counter > 0 {
            self.triangle.linear_counter -= 1;
        }
        if !self.triangle.control_flag {
            self.triangle.linear_counter_reload_flag = false;
        }
    }

    // Clock the Half Frame (Length counters + sweep units)
    fn clock_half_frame(&mut self) {
        if self.pulse1.length_counter > 0 && !self.pulse1.length_counter_halt {
            self.pulse1.length_counter -= 1;
        }
        if self.pulse2.length_counter > 0 && !self.pulse2.length_counter_halt {
            self.pulse2.length_counter -= 1;
        }
        if self.triangle.length_counter > 0 && !self.triangle.control_flag {
            self.triangle.length_counter -= 1;
        }
        if self.noise.length_counter > 0 && !self.noise.length_counter_halt {
            self.noise.length_counter -= 1;
        }

        // Sweep unit clocking
        let p1_period = self.pulse1.timer_period;
        self.pulse1.timer_period = self.pulse1.sweep.clock(p1_period);
        let p2_period = self.pulse2.timer_period;
        self.pulse2.timer_period = self.pulse2.sweep.clock(p2_period);
    }

    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.cpu_cycle_count += 1;
            let is_even_cycle = self.cpu_cycle_count % 2 == 0;

            // 1. Tick channels: pulse/noise at half CPU rate, triangle at full CPU rate
            if is_even_cycle {
                self.pulse1.tick(1);
                self.pulse2.tick(1);
                self.noise.tick(1);
            }
            self.triangle.tick(1);

            // 2. Process write delay
            if self.frame_counter_write_pending {
                self.frame_counter_write_delay -= 1;
                if self.frame_counter_write_delay == 0 {
                    self.frame_counter_write_pending = false;
                    let val = self.frame_counter_write_val;
                    self.execute_4017_write(val);
                }
            }

            // 3. Tick Frame counter by 1 cycle
            self.frame_counter_cycle += 1;
            if self.frame_counter_mode == 0 {
                // 4-Step Mode (Mode 0)
                let limit = self.timing.apu_4_step_rates[self.frame_counter_step as usize];

                if self.frame_counter_cycle >= limit {
                    self.clock_quarter_frame();
                    self.frame_counter_step = (self.frame_counter_step + 1) % 4;
                    if self.frame_counter_step == 2 || self.frame_counter_step == 0 {
                        self.clock_half_frame();
                    }
                    self.frame_counter_cycle -= limit;

                    // Cycle 29828 since reset: wrap to Step 0
                    if self.frame_counter_step == 0 {
                        self.irq_assertion_cycles = 3;
                    }
                }

                if self.irq_assertion_cycles > 0 {
                    if self.irq_enabled {
                        self.irq_pending = true;
                    }
                    self.irq_assertion_cycles -= 1;
                }
            } else {
                // 5-Step Mode (Mode 1)
                // Clock based on CURRENT step before incrementing.
                // Step 4 is the empty step (no clocking).
                let limit = self.timing.apu_5_step_rates[self.frame_counter_step as usize];
                if self.frame_counter_cycle >= limit {
                    let current_step = self.frame_counter_step;
                    self.frame_counter_step = (self.frame_counter_step + 1) % 5;

                    // Step 4 is the empty step — no quarter or half frame clocking
                    if current_step != 4 {
                        self.clock_quarter_frame();
                    }
                    if current_step == 1 || current_step == 3 {
                        self.clock_half_frame();
                    }
                    self.frame_counter_cycle -= limit;
                }
            }
        }

        // Accurate DMC ticking
        if self.dmc_active && self.dmc_bytes_remaining > 0 {
            self.dmc_cycle_counter += cycles;
            let cycles_per_byte = self.dmc_rate as u32 * 8;
            while self.dmc_active && self.dmc_cycle_counter >= cycles_per_byte {
                self.dmc_cycle_counter -= cycles_per_byte;
                self.dmc_bytes_remaining -= 1;
                if self.dmc_bytes_remaining == 0 {
                    if self.dmc_loop {
                        self.dmc_bytes_remaining = self.dmc_sample_length as u32;
                    } else {
                        self.dmc_active = false;
                        if self.dmc_irq_enable {
                            self.dmc_irq_pending = true;
                        }
                    }
                }
            }
        }

        self.cycle_accumulator += cycles as f64;
        let cycle_step = self.timing.cpu_clock_speed / 44100.0;
        while self.cycle_accumulator >= cycle_step {
            let p1 = self.pulse1.sample();
            let p2 = self.pulse2.sample();
            let tri = self.triangle.sample();
            let ns = self.noise.sample();
            let dmc = self.dmc_output_level;
            let raw_sample = mix(p1, p2, tri, ns, dmc);

            // 1. High-Pass Filter (~90Hz cutoff to eliminate DC offset)
            let alpha_hpf = 0.996_f32;
            let hpf_sample = alpha_hpf * (self.prev_output + raw_sample - self.prev_input);
            self.prev_input = raw_sample;
            self.prev_output = hpf_sample;

            // 2. Low-Pass Filter (~14kHz cutoff to smooth square wave aliasing)
            let alpha_lpf = 0.666_f32;
            let lpf_sample = self.prev_lpf_output + alpha_lpf * (hpf_sample - self.prev_lpf_output);
            self.prev_lpf_output = lpf_sample;

            self.sample_buffer.push(lpf_sample);
            self.cycle_accumulator -= cycle_step;
        }
    }

    pub fn read_reg(&mut self, addr: u16) -> u8 {
        if addr == 0x4015 {
            let mut status = 0;
            if self.pulse1.length_counter > 0 {
                status |= 1;
            }
            if self.pulse2.length_counter > 0 {
                status |= 2;
            }
            if self.triangle.length_counter > 0 {
                status |= 4;
            }
            if self.noise.length_counter > 0 {
                status |= 8;
            }
            if self.dmc_active {
                status |= 0x10;
            }
            if self.irq_pending {
                status |= 0x40;
            }
            if self.dmc_irq_pending {
                status |= 0x80;
            }
            self.irq_pending = false; // Reading clears Frame IRQ pending flag!
            status
        } else if addr == 0x4017 {
            0x40
        } else {
            0
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        match addr {
            // Pulse 1
            0x4000 => {
                self.pulse1.duty = (val >> 6) & 3;
                self.pulse1.length_counter_halt = (val & 0x20) != 0;
                self.pulse1.envelope.loop_flag = (val & 0x20) != 0;
                self.pulse1.envelope.constant_volume = (val & 0x10) != 0;
                self.pulse1.envelope.volume = val & 0x0F;
            }
            0x4001 => {
                self.pulse1.sweep.enabled = (val & 0x80) != 0;
                self.pulse1.sweep.period = (val >> 4) & 0x07;
                self.pulse1.sweep.negate = (val & 0x08) != 0;
                self.pulse1.sweep.shift = val & 0x07;
                self.pulse1.sweep.reload = true;
            }
            0x4002 => {
                self.pulse1.timer_period = (self.pulse1.timer_period & 0x0700) | (val as u16);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x00FF) | (((val & 0x07) as u16) << 8);
                if self.pulse1.enabled {
                    self.pulse1.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                }
                self.pulse1.duty_step = 0;
                self.pulse1.envelope.start = true;
            }

            // Pulse 2
            0x4004 => {
                self.pulse2.duty = (val >> 6) & 3;
                self.pulse2.length_counter_halt = (val & 0x20) != 0;
                self.pulse2.envelope.loop_flag = (val & 0x20) != 0;
                self.pulse2.envelope.constant_volume = (val & 0x10) != 0;
                self.pulse2.envelope.volume = val & 0x0F;
            }
            0x4005 => {
                self.pulse2.sweep.enabled = (val & 0x80) != 0;
                self.pulse2.sweep.period = (val >> 4) & 0x07;
                self.pulse2.sweep.negate = (val & 0x08) != 0;
                self.pulse2.sweep.shift = val & 0x07;
                self.pulse2.sweep.reload = true;
            }
            0x4006 => {
                self.pulse2.timer_period = (self.pulse2.timer_period & 0x0700) | (val as u16);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x00FF) | (((val & 0x07) as u16) << 8);
                if self.pulse2.enabled {
                    self.pulse2.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                }
                self.pulse2.duty_step = 0;
                self.pulse2.envelope.start = true;
            }

            // Triangle
            0x4008 => {
                self.triangle.control_flag = (val & 0x80) != 0;
                self.triangle.linear_counter_reload = val & 0x7F;
            }
            0x4009 => {}
            0x400A => {
                self.triangle.timer_period = (self.triangle.timer_period & 0x0700) | (val as u16);
            }
            0x400B => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x00FF) | (((val & 0x07) as u16) << 8);
                if self.triangle.enabled {
                    self.triangle.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                }
                self.triangle.linear_counter_reload_flag = true;
            }

            // Noise
            0x400C => {
                self.noise.length_counter_halt = (val & 0x20) != 0;
                self.noise.envelope.loop_flag = (val & 0x20) != 0;
                self.noise.envelope.constant_volume = (val & 0x10) != 0;
                self.noise.envelope.volume = val & 0x0F;
            }
            0x400D => {}
            0x400E => {
                self.noise.loop_noise = (val & 0x80) != 0;
                self.noise.timer_period = self.timing.noise_period_table[(val & 0x0F) as usize];
            }
            0x400F => {
                if self.noise.enabled {
                    self.noise.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                }
                self.noise.envelope.start = true;
            }

            // DMC
            0x4010 => {
                self.dmc_irq_enable = (val & 0x80) != 0;
                self.dmc_loop = (val & 0x40) != 0;
                self.dmc_rate = self.timing.dmc_rate_table[(val & 0x0F) as usize];
                if !self.dmc_irq_enable {
                    self.dmc_irq_pending = false;
                }
            }
            0x4011 => {
                self.dmc_output_level = val & 0x7F;
            }
            0x4012 => {}
            0x4013 => {
                self.dmc_sample_length = (val as u16 * 16) + 1;
            }

            // Status Register
            0x4015 => {
                self.pulse1.enabled = (val & 1) != 0;
                if !self.pulse1.enabled {
                    self.pulse1.length_counter = 0;
                }

                self.pulse2.enabled = (val & 2) != 0;
                if !self.pulse2.enabled {
                    self.pulse2.length_counter = 0;
                }

                self.triangle.enabled = (val & 4) != 0;
                if !self.triangle.enabled {
                    self.triangle.length_counter = 0;
                }

                self.noise.enabled = (val & 8) != 0;
                if !self.noise.enabled {
                    self.noise.length_counter = 0;
                }

                self.dmc_enabled = (val & 0x10) != 0;
                if self.dmc_enabled {
                    if self.dmc_bytes_remaining == 0 {
                        self.dmc_bytes_remaining = self.dmc_sample_length as u32;
                        self.dmc_active = true;
                    }
                } else {
                    self.dmc_active = false;
                    self.dmc_bytes_remaining = 0;
                }
                self.dmc_irq_pending = false; // Any write to $4015 clears DMC IRQ
            }

            0x4017 => {
                self.execute_4017_write(val);
            }

            _ => {}
        }
    }

    fn execute_4017_write(&mut self, val: u8) {
        self.frame_counter_last_val = val;
        self.frame_counter_mode = (val >> 7) & 0x01;
        self.irq_enabled = (val & 0x40) == 0;
        if !self.irq_enabled {
            self.irq_pending = false;
        }
        self.frame_counter_cycle = 0;
        self.frame_counter_step = 0;
        self.irq_assertion_cycles = 0;
        if self.frame_counter_mode == 1 {
            self.clock_quarter_frame();
            self.clock_half_frame();
        }
    }

    pub fn write_reg_from_cpu(&mut self, addr: u16, val: u8) {
        if addr == 0x4017 {
            self.frame_counter_write_pending = true;
            self.frame_counter_write_val = val;
            let write_cycle = self.cpu_cycle_count + 3;
            let extra = if write_cycle % 2 == 1 { 1 } else { 0 };
            self.frame_counter_write_delay = 3 + extra;
        } else {
            self.write_reg(addr, val);
        }
    }

    pub fn poll_irq(&mut self) -> bool {
        self.irq_pending || self.dmc_irq_pending
    }
}

fn mix(pulse1: f32, pulse2: f32, triangle: f32, noise: f32, dmc: u8) -> f32 {
    let pulse_out = if pulse1 + pulse2 > 0.0 {
        95.88 / ((8128.0 / (pulse1 + pulse2)) + 100.0)
    } else {
        0.0
    };

    let tnd_sum = triangle / 8227.0 + noise / 12241.0 + dmc as f32 / 22638.0;
    let tnd_out = if tnd_sum > 0.0 {
        159.79 / ((1.0 / tnd_sum) + 100.0)
    } else {
        0.0
    };

    pulse_out + tnd_out
}

// ──────────────────────────── Tests ────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_decay_sequence() {
        let mut env = Envelope {
            start: true,
            divider: 0,
            decay_level: 0,
            volume: 2, // divider period = 2
            constant_volume: false,
            loop_flag: false,
        };
        // First clock: start flag is set, so decay_level resets to 15, divider resets to volume
        env.clock();
        assert!(!env.start);
        assert_eq!(env.decay_level, 15);
        assert_eq!(env.divider, 2);
        assert_eq!(env.output(), 15);

        // Next 2 clocks: divider counts down from 2 to 0
        env.clock(); // divider 2 -> 1
        assert_eq!(env.divider, 1);
        assert_eq!(env.decay_level, 15);

        env.clock(); // divider 1 -> 0
        assert_eq!(env.divider, 0);
        assert_eq!(env.decay_level, 15);

        // Next clock: divider was 0, so it reloads and decay_level decrements
        env.clock(); // divider reloads to 2, decay_level 15 -> 14
        assert_eq!(env.divider, 2);
        assert_eq!(env.decay_level, 14);
        assert_eq!(env.output(), 14);
    }

    #[test]
    fn test_envelope_constant_volume() {
        let env = Envelope {
            start: false,
            divider: 0,
            decay_level: 5,
            volume: 10,
            constant_volume: true,
            loop_flag: false,
        };
        assert_eq!(env.output(), 10);
    }

    #[test]
    fn test_envelope_loop_flag() {
        let mut env = Envelope {
            start: false,
            divider: 0,
            decay_level: 0,
            volume: 0, // divider period = 0, so it triggers every clock
            constant_volume: false,
            loop_flag: true,
        };
        // decay_level is 0, divider is 0, loop is true -> decay wraps to 15
        env.clock();
        assert_eq!(env.decay_level, 15);
    }

    #[test]
    fn test_sweep_pitch_change_pulse1() {
        // Pulse 1 uses one's complement negate
        let mut sweep = Sweep {
            enabled: true,
            divider: 0, // will trigger on next clock
            negate: true,
            shift: 1,
            reload: false,
            period: 0,
            is_channel_1: true,
        };
        let timer_period: u16 = 0x100;
        let new_period = sweep.clock(timer_period);
        // change = 0x100 >> 1 = 0x80
        // Pulse 1 negate: period - change - 1 = 0x100 - 0x80 - 1 = 0x7F
        assert_eq!(new_period, 0x7F);
    }

    #[test]
    fn test_sweep_pitch_change_pulse2() {
        // Pulse 2 uses two's complement negate
        let mut sweep = Sweep {
            enabled: true,
            divider: 0,
            negate: true,
            shift: 1,
            reload: false,
            period: 0,
            is_channel_1: false,
        };
        let timer_period: u16 = 0x100;
        let new_period = sweep.clock(timer_period);
        // change = 0x100 >> 1 = 0x80
        // Pulse 2 negate: period - change = 0x100 - 0x80 = 0x80
        assert_eq!(new_period, 0x80);
    }

    #[test]
    fn test_sweep_muting_low_period() {
        let sweep = Sweep {
            enabled: false,
            divider: 0,
            negate: false,
            shift: 0,
            reload: false,
            period: 0,
            is_channel_1: false,
        };
        // Timer period < 8 should be muted
        assert!(sweep.muting(7));
        assert!(!sweep.muting(8));
    }

    #[test]
    fn test_sweep_muting_high_target() {
        let sweep = Sweep {
            enabled: true,
            divider: 0,
            negate: false,
            shift: 1,
            reload: false,
            period: 0,
            is_channel_1: false,
        };
        // Timer period 0x600, shift 1: target = 0x600 + 0x300 = 0x900 > 0x7FF => muted
        assert!(sweep.muting(0x600));
    }

    #[test]
    fn test_length_counter_table_values() {
        // Spot-check some well-known values
        assert_eq!(LENGTH_COUNTER_TABLE[0], 10);
        assert_eq!(LENGTH_COUNTER_TABLE[1], 254);
        assert_eq!(LENGTH_COUNTER_TABLE[2], 20);
        assert_eq!(LENGTH_COUNTER_TABLE[3], 2);
        assert_eq!(LENGTH_COUNTER_TABLE[31], 30);
        // All 32 entries should be present
        assert_eq!(LENGTH_COUNTER_TABLE.len(), 32);
    }

    #[test]
    fn test_dmc_output_level() {
        let mut apu = Apu::new();
        // Write $4011 with value 0xFF -> should mask to 0x7F
        apu.write_reg(0x4011, 0xFF);
        assert_eq!(apu.dmc_output_level, 0x7F);

        apu.write_reg(0x4011, 0x40);
        assert_eq!(apu.dmc_output_level, 0x40);

        apu.write_reg(0x4011, 0x00);
        assert_eq!(apu.dmc_output_level, 0x00);
    }

    #[test]
    fn test_mix_function() {
        // All zeros -> 0
        assert_eq!(mix(0.0, 0.0, 0.0, 0.0, 0), 0.0);

        // Only pulse
        let pulse_only = mix(8.0, 7.0, 0.0, 0.0, 0);
        assert!(pulse_only > 0.0);

        // DMC contribution
        let with_dmc = mix(0.0, 0.0, 0.0, 0.0, 64);
        assert!(with_dmc > 0.0);

        // Full mix should be larger than any individual component
        let full_mix = mix(8.0, 7.0, 10.0, 5.0, 64);
        assert!(full_mix > pulse_only);
        assert!(full_mix > with_dmc);
    }

    #[test]
    fn test_noise_lfsr() {
        let mut noise = NoiseChannel::default();
        noise.enabled = true;
        noise.length_counter = 10;
        noise.envelope.constant_volume = true;
        noise.envelope.volume = 15;
        noise.timer_period = 0; // tick every cycle

        let initial_sr = noise.shift_register;
        assert_eq!(initial_sr, 1);

        // Tick once to shift the register
        noise.tick(1);
        // After one shift: feedback = (1 ^ 0) = 1, sr = (1 >> 1) | (1 << 14) = 0x4000
        assert_eq!(noise.shift_register, 0x4000);

        // Bit 0 is now 0, so sample should output volume
        let s = noise.sample();
        assert_eq!(s, 15.0);
    }

    #[test]
    fn test_triangle_step_sequence() {
        let mut tri = TriangleChannel::default();
        tri.enabled = true;
        tri.length_counter = 10;
        tri.linear_counter = 10;
        tri.timer_period = 0; // tick every cycle

        // Step starts at 0, sample = 15 - 0 = 15
        assert_eq!(tri.sample(), 15.0);

        // After one tick, step advances to 1 => sample = 15 - 1 = 14
        tri.tick(1);
        assert_eq!(tri.step, 1);
        assert_eq!(tri.sample(), 14.0);

        // Advance to step 16 => sample = 16 - 16 = 0
        tri.tick(15);
        assert_eq!(tri.step, 16);
        assert_eq!(tri.sample(), 0.0);

        // Step 17 => sample = 17 - 16 = 1
        tri.tick(1);
        assert_eq!(tri.step, 17);
        assert_eq!(tri.sample(), 1.0);
    }

    #[test]
    fn test_pulse_channel_1_is_channel_1() {
        let apu = Apu::new();
        assert!(apu.pulse1.sweep.is_channel_1);
        assert!(!apu.pulse2.sweep.is_channel_1);
    }

    #[test]
    fn test_envelope_start_on_high_byte_write() {
        let mut apu = Apu::new();
        apu.write_reg(0x4015, 0x0F); // enable all channels

        // Write $4003 should set pulse1 envelope start
        apu.write_reg(0x4003, 0x00);
        assert!(apu.pulse1.envelope.start);

        // Write $4007 should set pulse2 envelope start
        apu.write_reg(0x4007, 0x00);
        assert!(apu.pulse2.envelope.start);

        // Write $400F should set noise envelope start
        apu.write_reg(0x400F, 0x00);
        assert!(apu.noise.envelope.start);
    }

    #[test]
    fn test_sweep_register_writes() {
        let mut apu = Apu::new();

        // Write $4001: sweep for pulse 1
        apu.write_reg(0x4001, 0b1_011_1_010); // enabled, period=3, negate, shift=2
        assert!(apu.pulse1.sweep.enabled);
        assert_eq!(apu.pulse1.sweep.period, 3);
        assert!(apu.pulse1.sweep.negate);
        assert_eq!(apu.pulse1.sweep.shift, 2);
        assert!(apu.pulse1.sweep.reload);

        // Write $4005: sweep for pulse 2
        apu.write_reg(0x4005, 0b0_101_0_011); // disabled, period=5, no negate, shift=3
        assert!(!apu.pulse2.sweep.enabled);
        assert_eq!(apu.pulse2.sweep.period, 5);
        assert!(!apu.pulse2.sweep.negate);
        assert_eq!(apu.pulse2.sweep.shift, 3);
        assert!(apu.pulse2.sweep.reload);
    }
}
