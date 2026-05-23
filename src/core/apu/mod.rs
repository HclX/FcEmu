const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

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
    cycle_accumulator: f64,
    prev_input: f32,
    prev_output: f32,
    prev_lpf_output: f32,
    frame_counter_cycle: u32,
    frame_counter_step: u8,
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
        }
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
        if self.pulse1.length_counter > 0 {
            self.pulse1.length_counter -= 1;
        }
        if self.pulse2.length_counter > 0 {
            self.pulse2.length_counter -= 1;
        }
        if self.triangle.length_counter > 0 {
            self.triangle.length_counter -= 1;
        }
        if self.noise.length_counter > 0 {
            self.noise.length_counter -= 1;
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        self.pulse1.tick(cycles);
        self.pulse2.tick(cycles);
        self.triangle.tick(cycles);
        self.noise.tick(cycles);

        // Frame counter ticking (NTSC quarter step is 7457 cycles)
        self.frame_counter_cycle += cycles;
        while self.frame_counter_cycle >= 7457 {
            self.clock_quarter_frame();
            
            self.frame_counter_step = (self.frame_counter_step + 1) & 3;
            if self.frame_counter_step == 1 || self.frame_counter_step == 3 {
                self.clock_half_frame();
            }
            
            self.frame_counter_cycle -= 7457;
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
                self.pulse1.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                self.pulse1.duty_step = 0;
            }

            // Pulse 2
            0x4004 => {
                self.pulse2.duty = (val >> 6) & 3;
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
                self.pulse2.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
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
                self.triangle.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
                self.triangle.linear_counter = self.triangle.linear_counter_reload;
            }

            // Noise
            0x400C => {
                self.noise.constant_volume = (val & 0x10) != 0;
                self.noise.volume = val & 0x0F;
            }
            0x400D => {}
            0x400E => {
                self.noise.loop_noise = (val & 0x80) != 0;
                self.noise.timer_period = NOISE_PERIOD_TABLE[(val & 0x0F) as usize];
            }
            0x400F => {
                self.noise.length_counter = LENGTH_COUNTER_TABLE[(val >> 3) as usize];
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
            }

            // Frame counter
            0x4017 => {}

            _ => {}
        }
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
