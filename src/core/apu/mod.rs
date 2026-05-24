const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

const DMC_RATE_TABLE: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106,  84,  72,  54
];

const NTSC_4_STEP_RATES: [u32; 4] = [7457, 7456, 7458, 7458];
const NTSC_5_STEP_RATES: [u32; 5] = [7457, 7456, 7458, 7458, 7452];

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
        if !self.enabled || self.length_counter == 0 || self.timer_period < 8 {
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
            self.volume as f32
        } else {
            0.0
        }
    }
}

#[derive(Default, Clone)]
pub struct TriangleChannel {
    pub enabled: bool,
    pub control_flag: bool,
    pub linear_counter_reload: u8,
    pub linear_counter: u8,
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
            self.volume as f32
        }
    }
}

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
    pub frame_counter_last_val: u8,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        Self {
            pulse1: PulseChannel::default(),
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
            frame_counter_last_val: 0x00,
        }
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

    // Clock the Quarter Frame (Triangle linear counter)
    fn clock_quarter_frame(&mut self) {
        if self.triangle.control_flag {
            self.triangle.linear_counter = self.triangle.linear_counter_reload;
        } else if self.triangle.linear_counter > 0 {
            self.triangle.linear_counter -= 1;
        }
    }

    // Clock the Half Frame (Length counters for Pulse 1, Pulse 2, Triangle, and Noise)
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
    }

    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.cpu_cycle_count += 1;
            // 1. Tick channels by 1 cycle
            self.pulse1.tick(1);
            self.pulse2.tick(1);
            self.triangle.tick(1);
            self.noise.tick(1);

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
                let limit = NTSC_4_STEP_RATES[self.frame_counter_step as usize];
                
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
                let limit = NTSC_5_STEP_RATES[self.frame_counter_step as usize];
                if self.frame_counter_cycle >= limit {
                    self.frame_counter_step = (self.frame_counter_step + 1) % 5;
                    if self.frame_counter_step < 4 {
                        self.clock_quarter_frame();
                    }
                    if self.frame_counter_step == 2 || self.frame_counter_step == 0 {
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
        let cycle_step = 1789773.0 / 44100.0;
        while self.cycle_accumulator >= cycle_step {
            let p1 = self.pulse1.sample();
            let p2 = self.pulse2.sample();
            let tri = self.triangle.sample();
            let ns = self.noise.sample();
            let raw_sample = mix(p1, p2, tri, ns);

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
                self.pulse1.constant_volume = (val & 0x10) != 0;
                self.pulse1.volume = val & 0x0F;
            }
            0x4001 => {} // Sweep stub
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
            }

            // Pulse 2
            0x4004 => {
                self.pulse2.duty = (val >> 6) & 3;
                self.pulse2.length_counter_halt = (val & 0x20) != 0;
                self.pulse2.constant_volume = (val & 0x10) != 0;
                self.pulse2.volume = val & 0x0F;
            }
            0x4005 => {} // Sweep stub
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
                self.triangle.linear_counter = self.triangle.linear_counter_reload;
            }

            // Noise
            0x400C => {
                self.noise.length_counter_halt = (val & 0x20) != 0;
                self.noise.constant_volume = (val & 0x10) != 0;
                self.noise.volume = val & 0x0F;
            }
            0x400D => {}
            0x400E => {
                self.noise.loop_noise = (val & 0x80) != 0;
                self.noise.timer_period = NOISE_PERIOD_TABLE[(val & 0x0F) as usize];
            }
            0x400F => {
                if self.noise.enabled {
                    self.noise.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                }
            }

            // DMC
            0x4010 => {
                self.dmc_irq_enable = (val & 0x80) != 0;
                self.dmc_loop = (val & 0x40) != 0;
                self.dmc_rate = DMC_RATE_TABLE[(val & 0x0F) as usize];
                if !self.dmc_irq_enable {
                    self.dmc_irq_pending = false;
                }
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

fn mix(pulse1: f32, pulse2: f32, triangle: f32, noise: f32) -> f32 {
    let pulse_out = if pulse1 + pulse2 > 0.0 {
        95.88 / ((8128.0 / (pulse1 + pulse2)) + 100.0)
    } else {
        0.0
    };

    let tnd_sum = triangle / 8227.0 + noise / 12241.0;
    let tnd_out = if tnd_sum > 0.0 {
        159.79 / ((1.0 / tnd_sum) + 100.0)
    } else {
        0.0
    };

    pulse_out + tnd_out
}
