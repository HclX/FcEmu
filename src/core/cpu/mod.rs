use crate::core::bus::CpuBus;

const CARRY: u8 = 0x01;
const ZERO: u8 = 0x02;
const INTERRUPT: u8 = 0x04;
const DECIMAL: u8 = 0x08;
const BREAK: u8 = 0x10;
const BREAK2: u8 = 0x20;
const OVERFLOW: u8 = 0x40;
const NEGATIVE: u8 = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndirectX,
    IndirectY,
}

pub struct Cpu {
    pub a: u8,       // Accumulator
    pub x: u8,       // Index X
    pub y: u8,       // Index Y
    pub pc: u16,     // Program Counter
    pub sp: u8,      // Stack Pointer
    pub status: u8,  // Status Flags
    pub cycles: u64, // Elapsed CPU cycles
    pub power_on: bool, // First power-on boot reset flag
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            pc: 0xC000,
            sp: 0xFD,
            status: 0x34, // Status starts with INTERRUPT, BREAK2 and unused bit 5 set (0x34)
            cycles: 0,
            power_on: true,
        }
    }

    pub fn reset<B: CpuBus>(&mut self, bus: &mut B) {
        if self.power_on {
            self.cycles = 7; // Power-on reset sequence cycles
            self.power_on = false;
        } else {
            self.sp = self.sp.wrapping_sub(3);
            self.status |= 0x04; // Set Interrupt Disable flag (I = 1)
            self.cycles = 7;     // Soft reset takes exactly 7 cycles
        }
        let low = bus.read(0xFFFC) as u16;
        let high = bus.read(0xFFFD) as u16;
        self.pc = (high << 8) | low;
    }

    fn update_zero_and_negative_flags(&mut self, val: u8) {
        if val == 0 {
            self.status |= ZERO;
        } else {
            self.status &= !ZERO;
        }

        if (val & 0x80) != 0 {
            self.status |= NEGATIVE;
        } else {
            self.status &= !NEGATIVE;
        }
    }

    fn page_crossed(&self, addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    fn get_operand_address<B: CpuBus>(&self, mode: AddressingMode, bus: &mut B) -> (u16, bool) {
        match mode {
            AddressingMode::Immediate => (self.pc, false),
            AddressingMode::ZeroPage => (bus.read(self.pc) as u16, false),
            AddressingMode::Absolute => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                ((high << 8) | low, false)
            }
            AddressingMode::ZeroPageX => {
                let pos = bus.read(self.pc);
                let addr = pos.wrapping_add(self.x) as u16;
                (addr, false)
            }
            AddressingMode::ZeroPageY => {
                let pos = bus.read(self.pc);
                let addr = pos.wrapping_add(self.y) as u16;
                (addr, false)
            }
            AddressingMode::AbsoluteX => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                let base = (high << 8) | low;
                let addr = base.wrapping_add(self.x as u16);
                (addr, self.page_crossed(base, addr))
            }
            AddressingMode::AbsoluteY => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                let base = (high << 8) | low;
                let addr = base.wrapping_add(self.y as u16);
                (addr, self.page_crossed(base, addr))
            }
            AddressingMode::IndirectX => {
                let base = bus.read(self.pc);
                let ptr = base.wrapping_add(self.x);
                let lo = bus.read(ptr as u16) as u16;
                let hi = bus.read(ptr.wrapping_add(1) as u16) as u16;
                ((hi << 8) | lo, false)
            }
            AddressingMode::IndirectY => {
                let base = bus.read(self.pc);
                let lo = bus.read(base as u16) as u16;
                let hi = bus.read(base.wrapping_add(1) as u16) as u16;
                let base_addr = (hi << 8) | lo;
                let addr = base_addr.wrapping_add(self.y as u16);
                (addr, self.page_crossed(base_addr, addr))
            }
        }
    }

    fn get_instruction_len(&self, mode: AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate
            | AddressingMode::ZeroPage
            | AddressingMode::ZeroPageX
            | AddressingMode::ZeroPageY
            | AddressingMode::IndirectX
            | AddressingMode::IndirectY => 1,
            AddressingMode::Absolute | AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 2,
        }
    }

    fn push<B: CpuBus>(&mut self, bus: &mut B, val: u8) {
        bus.write(0x0100 + self.sp as u16, val);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 + self.sp as u16)
    }

    fn push_u16<B: CpuBus>(&mut self, bus: &mut B, val: u16) {
        self.push(bus, (val >> 8) as u8);
        self.push(bus, (val & 0xFF) as u8);
    }

    fn pop_u16<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.pop(bus) as u16;
        let hi = self.pop(bus) as u16;
        (hi << 8) | lo
    }

    pub fn nmi<B: CpuBus>(&mut self, bus: &mut B) {
        self.push_u16(bus, self.pc);
        let status = (self.status & !BREAK) | BREAK2;
        self.push(bus, status);
        self.status |= INTERRUPT;

        let low = bus.read(0xFFFA) as u16;
        let high = bus.read(0xFFFB) as u16;
        self.pc = (high << 8) | low;
        self.cycles += 7;
    }

    pub fn irq<B: CpuBus>(&mut self, bus: &mut B) {
        self.push_u16(bus, self.pc);
        let status = (self.status & !BREAK) | BREAK2;
        self.push(bus, status);
        self.status |= INTERRUPT;

        let low = bus.read(0xFFFE) as u16;
        let high = bus.read(0xFFFF) as u16;
        self.pc = (high << 8) | low;
        self.cycles += 7;
    }

    fn branch<B: CpuBus>(&mut self, bus: &mut B, condition: bool) -> u32 {
        let offset = bus.read(self.pc) as i8 as i16;
        self.pc = self.pc.wrapping_add(1);
        let mut cycles = 2;
        if condition {
            cycles += 1;
            let old_pc = self.pc;
            self.pc = self.pc.wrapping_add(offset as u16);
            if self.page_crossed(old_pc, self.pc) {
                cycles += 1;
            }
        }
        cycles
    }

    fn adc(&mut self, val: u8) {
        let carry = self.status & CARRY;
        let sum = self.a as u16 + val as u16 + carry as u16;

        if sum > 0xFF {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }

        let result = sum as u8;
        if ((self.a ^ val) & 0x80 == 0) && ((self.a ^ result) & 0x80 != 0) {
            self.status |= OVERFLOW;
        } else {
            self.status &= !OVERFLOW;
        }

        self.a = result;
        self.update_zero_and_negative_flags(self.a);
    }

    fn load_reg<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> (u8, bool) {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr);
        self.update_zero_and_negative_flags(val);
        (val, crossed)
    }

    fn store_reg<B: CpuBus>(&mut self, mode: AddressingMode, val: u8, bus: &mut B) {
        let (addr, _) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        bus.write(addr, val);
    }

    fn compare_reg<B: CpuBus>(&mut self, mode: AddressingMode, reg_val: u8, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr);
        let diff = reg_val.wrapping_sub(val);

        if reg_val >= val {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        self.update_zero_and_negative_flags(diff);
        crossed
    }

    fn dummy_read<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let _ = bus.read(addr);
        crossed
    }

    fn lax<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> (u8, bool) {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr);
        self.a = val;
        self.x = val;
        self.update_zero_and_negative_flags(val);
        (val, crossed)
    }

    fn sax<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) {
        let (addr, _) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        bus.write(addr, self.a & self.x);
    }

    fn dcp<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr).wrapping_sub(1);
        bus.write(addr, val);
        
        let reg_val = self.a;
        let diff = reg_val.wrapping_sub(val);
        if reg_val >= val {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        self.update_zero_and_negative_flags(diff);
        crossed
    }

    fn isb<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr).wrapping_add(1);
        bus.write(addr, val);
        self.adc(val ^ 0xFF);
        crossed
    }

    fn slo<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let mut val = bus.read(addr);
        if (val & 0x80) != 0 {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        val = val.wrapping_shl(1);
        bus.write(addr, val);
        self.a |= val;
        self.update_zero_and_negative_flags(self.a);
        crossed
    }

    fn rla<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let mut val = bus.read(addr);
        let old_carry = if (self.status & CARRY) != 0 { 1 } else { 0 };
        if (val & 0x80) != 0 {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        val = val.wrapping_shl(1) | old_carry;
        bus.write(addr, val);
        self.a &= val;
        self.update_zero_and_negative_flags(self.a);
        crossed
    }

    fn sre<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let mut val = bus.read(addr);
        if (val & 0x01) != 0 {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        val = val.wrapping_shr(1);
        bus.write(addr, val);
        self.a ^= val;
        self.update_zero_and_negative_flags(self.a);
        crossed
    }

    fn rra<B: CpuBus>(&mut self, mode: AddressingMode, bus: &mut B) -> bool {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let mut val = bus.read(addr);
        let old_carry = if (self.status & CARRY) != 0 { 0x80 } else { 0 };
        if (val & 0x01) != 0 {
            self.status |= CARRY;
        } else {
            self.status &= !CARRY;
        }
        val = val.wrapping_shr(1) | old_carry;
        bus.write(addr, val);
        self.adc(val);
        crossed
    }


    fn alu_op<B: CpuBus>(&mut self, mode: AddressingMode, op: u8, bus: &mut B) {
        let (addr, crossed) = self.get_operand_address(mode, bus);
        self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
        let val = bus.read(addr);
        match op {
            0 => {
                // AND
                self.a &= val;
                self.update_zero_and_negative_flags(self.a);
            }
            1 => {
                // ORA
                self.a |= val;
                self.update_zero_and_negative_flags(self.a);
            }
            2 => {
                // EOR
                self.a ^= val;
                self.update_zero_and_negative_flags(self.a);
            }
            3 => {
                // ADC
                self.adc(val);
            }
            4 => {
                // SBC
                self.adc(val ^ 0xFF);
            }
            _ => {}
        }
        if crossed {
            self.cycles += 1;
        }
    }

    pub fn step<B: CpuBus>(&mut self, bus: &mut B) -> u32 {
        if bus.poll_nmi() {
            self.nmi(bus);
            return 7;
        }

        if (self.status & INTERRUPT) == 0 && bus.poll_irq() {
            self.irq(bus);
            return 7;
        }

        let opcode = bus.read(self.pc);
        let inst_pc = self.pc;
        self.pc = self.pc.wrapping_add(1);

        let start_cycles = self.cycles;

        let cycles = match opcode {
            // BRK (Force Interrupt)
            0x00 => {
                self.pc = self.pc.wrapping_add(1); // Point to PC + 2 (BRK is a 2-byte instruction)
                self.push_u16(bus, self.pc);
                self.push(bus, self.status | BREAK | BREAK2);
                self.status |= INTERRUPT;

                let low = bus.read(0xFFFE) as u16;
                let high = bus.read(0xFFFF) as u16;
                self.pc = (high << 8) | low;
                7
            }

            // NOP
            0xEA => 2,

            // Undocumented NOPs
            0x04 | 0x44 | 0x64 => {
                self.dummy_read(AddressingMode::ZeroPage, bus);
                3
            }
            0x0C => {
                self.dummy_read(AddressingMode::Absolute, bus);
                4
            }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                self.dummy_read(AddressingMode::ZeroPageX, bus);
                4
            }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let c = self.dummy_read(AddressingMode::AbsoluteX, bus);
                4 + if c { 1 } else { 0 }
            }
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                self.dummy_read(AddressingMode::Immediate, bus);
                2
            }
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => 2,

            // Undocumented LAX
            0xA3 => {
                self.lax(AddressingMode::IndirectX, bus);
                6
            }
            0xA7 => {
                self.lax(AddressingMode::ZeroPage, bus);
                3
            }
            0xAF => {
                self.lax(AddressingMode::Absolute, bus);
                4
            }
            0xB3 => {
                let (_, c) = self.lax(AddressingMode::IndirectY, bus);
                5 + if c { 1 } else { 0 }
            }
            0xB7 => {
                self.lax(AddressingMode::ZeroPageY, bus);
                4
            }
            0xBF => {
                let (_, c) = self.lax(AddressingMode::AbsoluteY, bus);
                4 + if c { 1 } else { 0 }
            }

            // Undocumented SAX
            0x83 => {
                self.sax(AddressingMode::IndirectX, bus);
                6
            }
            0x87 => {
                self.sax(AddressingMode::ZeroPage, bus);
                3
            }
            0x8F => {
                self.sax(AddressingMode::Absolute, bus);
                4
            }
            0x97 => {
                self.sax(AddressingMode::ZeroPageY, bus);
                4
            }

            // Undocumented DCP
            0xC3 => {
                self.dcp(AddressingMode::IndirectX, bus);
                8
            }
            0xC7 => {
                self.dcp(AddressingMode::ZeroPage, bus);
                5
            }
            0xCF => {
                self.dcp(AddressingMode::Absolute, bus);
                6
            }
            0xD3 => {
                self.dcp(AddressingMode::IndirectY, bus);
                8
            }
            0xD7 => {
                self.dcp(AddressingMode::ZeroPageX, bus);
                6
            }
            0xDB => {
                self.dcp(AddressingMode::AbsoluteY, bus);
                7
            }
            0xDF => {
                self.dcp(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented ISB
            0xE3 => {
                self.isb(AddressingMode::IndirectX, bus);
                8
            }
            0xE7 => {
                self.isb(AddressingMode::ZeroPage, bus);
                5
            }
            0xEF => {
                self.isb(AddressingMode::Absolute, bus);
                6
            }
            0xF3 => {
                self.isb(AddressingMode::IndirectY, bus);
                8
            }
            0xF7 => {
                self.isb(AddressingMode::ZeroPageX, bus);
                6
            }
            0xFB => {
                self.isb(AddressingMode::AbsoluteY, bus);
                7
            }
            0xFF => {
                self.isb(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented SLO
            0x03 => {
                self.slo(AddressingMode::IndirectX, bus);
                8
            }
            0x07 => {
                self.slo(AddressingMode::ZeroPage, bus);
                5
            }
            0x0F => {
                self.slo(AddressingMode::Absolute, bus);
                6
            }
            0x13 => {
                self.slo(AddressingMode::IndirectY, bus);
                8
            }
            0x17 => {
                self.slo(AddressingMode::ZeroPageX, bus);
                6
            }
            0x1B => {
                self.slo(AddressingMode::AbsoluteY, bus);
                7
            }
            0x1F => {
                self.slo(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented RLA
            0x23 => {
                self.rla(AddressingMode::IndirectX, bus);
                8
            }
            0x27 => {
                self.rla(AddressingMode::ZeroPage, bus);
                5
            }
            0x2F => {
                self.rla(AddressingMode::Absolute, bus);
                6
            }
            0x33 => {
                self.rla(AddressingMode::IndirectY, bus);
                8
            }
            0x37 => {
                self.rla(AddressingMode::ZeroPageX, bus);
                6
            }
            0x3B => {
                self.rla(AddressingMode::AbsoluteY, bus);
                7
            }
            0x3F => {
                self.rla(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented SRE
            0x43 => {
                self.sre(AddressingMode::IndirectX, bus);
                8
            }
            0x47 => {
                self.sre(AddressingMode::ZeroPage, bus);
                5
            }
            0x4F => {
                self.sre(AddressingMode::Absolute, bus);
                6
            }
            0x53 => {
                self.sre(AddressingMode::IndirectY, bus);
                8
            }
            0x57 => {
                self.sre(AddressingMode::ZeroPageX, bus);
                6
            }
            0x5B => {
                self.sre(AddressingMode::AbsoluteY, bus);
                7
            }
            0x5F => {
                self.sre(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented RRA
            0x63 => {
                self.rra(AddressingMode::IndirectX, bus);
                8
            }
            0x67 => {
                self.rra(AddressingMode::ZeroPage, bus);
                5
            }
            0x6F => {
                self.rra(AddressingMode::Absolute, bus);
                6
            }
            0x73 => {
                self.rra(AddressingMode::IndirectY, bus);
                8
            }
            0x77 => {
                self.rra(AddressingMode::ZeroPageX, bus);
                6
            }
            0x7B => {
                self.rra(AddressingMode::AbsoluteY, bus);
                7
            }
            0x7F => {
                self.rra(AddressingMode::AbsoluteX, bus);
                7
            }

            // Undocumented SBC
            0xEB => {
                self.alu_op(AddressingMode::Immediate, 4, bus);
                2
            }


            // LDA
            0xA9 => {
                let (v, _) = self.load_reg(AddressingMode::Immediate, bus);
                self.a = v;
                2
            }
            0xA5 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPage, bus);
                self.a = v;
                3
            }
            0xB5 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPageX, bus);
                self.a = v;
                4
            }
            0xAD => {
                let (v, _) = self.load_reg(AddressingMode::Absolute, bus);
                self.a = v;
                4
            }
            0xBD => {
                let (v, c) = self.load_reg(AddressingMode::AbsoluteX, bus);
                self.a = v;
                4 + if c { 1 } else { 0 }
            }
            0xB9 => {
                let (v, c) = self.load_reg(AddressingMode::AbsoluteY, bus);
                self.a = v;
                4 + if c { 1 } else { 0 }
            }
            0xA1 => {
                let (v, _) = self.load_reg(AddressingMode::IndirectX, bus);
                self.a = v;
                6
            }
            0xB1 => {
                let (v, c) = self.load_reg(AddressingMode::IndirectY, bus);
                self.a = v;
                5 + if c { 1 } else { 0 }
            }

            // LDX
            0xA2 => {
                let (v, _) = self.load_reg(AddressingMode::Immediate, bus);
                self.x = v;
                2
            }
            0xA6 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPage, bus);
                self.x = v;
                3
            }
            0xB6 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPageY, bus);
                self.x = v;
                4
            }
            0xAE => {
                let (v, _) = self.load_reg(AddressingMode::Absolute, bus);
                self.x = v;
                4
            }
            0xBE => {
                let (v, c) = self.load_reg(AddressingMode::AbsoluteY, bus);
                self.x = v;
                4 + if c { 1 } else { 0 }
            }

            // LDY
            0xA0 => {
                let (v, _) = self.load_reg(AddressingMode::Immediate, bus);
                self.y = v;
                2
            }
            0xA4 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPage, bus);
                self.y = v;
                3
            }
            0xB4 => {
                let (v, _) = self.load_reg(AddressingMode::ZeroPageX, bus);
                self.y = v;
                4
            }
            0xAC => {
                let (v, _) = self.load_reg(AddressingMode::Absolute, bus);
                self.y = v;
                4
            }
            0xBC => {
                let (v, c) = self.load_reg(AddressingMode::AbsoluteX, bus);
                self.y = v;
                4 + if c { 1 } else { 0 }
            }

            // STA
            0x85 => {
                self.store_reg(AddressingMode::ZeroPage, self.a, bus);
                3
            }
            0x95 => {
                self.store_reg(AddressingMode::ZeroPageX, self.a, bus);
                4
            }
            0x8D => {
                self.store_reg(AddressingMode::Absolute, self.a, bus);
                4
            }
            0x9D => {
                self.store_reg(AddressingMode::AbsoluteX, self.a, bus);
                5
            }
            0x99 => {
                self.store_reg(AddressingMode::AbsoluteY, self.a, bus);
                5
            }
            0x81 => {
                self.store_reg(AddressingMode::IndirectX, self.a, bus);
                6
            }
            0x91 => {
                self.store_reg(AddressingMode::IndirectY, self.a, bus);
                6
            }

            // STX
            0x86 => {
                self.store_reg(AddressingMode::ZeroPage, self.x, bus);
                3
            }
            0x96 => {
                self.store_reg(AddressingMode::ZeroPageY, self.x, bus);
                4
            }
            0x8E => {
                self.store_reg(AddressingMode::Absolute, self.x, bus);
                4
            }

            // STY
            0x84 => {
                self.store_reg(AddressingMode::ZeroPage, self.y, bus);
                3
            }
            0x94 => {
                self.store_reg(AddressingMode::ZeroPageX, self.y, bus);
                4
            }
            0x8C => {
                self.store_reg(AddressingMode::Absolute, self.y, bus);
                4
            }

            // CMP
            0xC9 => {
                let c = self.compare_reg(AddressingMode::Immediate, self.a, bus);
                2 + if c { 1 } else { 0 }
            }
            0xC5 => {
                self.compare_reg(AddressingMode::ZeroPage, self.a, bus);
                3
            }
            0xD5 => {
                self.compare_reg(AddressingMode::ZeroPageX, self.a, bus);
                4
            }
            0xCD => {
                self.compare_reg(AddressingMode::Absolute, self.a, bus);
                4
            }
            0xDD => {
                let c = self.compare_reg(AddressingMode::AbsoluteX, self.a, bus);
                4 + if c { 1 } else { 0 }
            }
            0xD9 => {
                let c = self.compare_reg(AddressingMode::AbsoluteY, self.a, bus);
                4 + if c { 1 } else { 0 }
            }
            0xC1 => {
                self.compare_reg(AddressingMode::IndirectX, self.a, bus);
                6
            }
            0xD1 => {
                let c = self.compare_reg(AddressingMode::IndirectY, self.a, bus);
                5 + if c { 1 } else { 0 }
            }

            // CPX
            0xE0 => {
                self.compare_reg(AddressingMode::Immediate, self.x, bus);
                2
            }
            0xE4 => {
                self.compare_reg(AddressingMode::ZeroPage, self.x, bus);
                3
            }
            0xEC => {
                self.compare_reg(AddressingMode::Absolute, self.x, bus);
                4
            }

            // CPY
            0xC0 => {
                self.compare_reg(AddressingMode::Immediate, self.y, bus);
                2
            }
            0xC4 => {
                self.compare_reg(AddressingMode::ZeroPage, self.y, bus);
                3
            }
            0xCC => {
                self.compare_reg(AddressingMode::Absolute, self.y, bus);
                4
            }

            // AND
            0x29 => {
                self.alu_op(AddressingMode::Immediate, 0, bus);
                2
            }
            0x25 => {
                self.alu_op(AddressingMode::ZeroPage, 0, bus);
                3
            }
            0x35 => {
                self.alu_op(AddressingMode::ZeroPageX, 0, bus);
                4
            }
            0x2D => {
                self.alu_op(AddressingMode::Absolute, 0, bus);
                4
            }
            0x3D => {
                self.alu_op(AddressingMode::AbsoluteX, 0, bus);
                4
            }
            0x39 => {
                self.alu_op(AddressingMode::AbsoluteY, 0, bus);
                4
            }
            0x21 => {
                self.alu_op(AddressingMode::IndirectX, 0, bus);
                6
            }
            0x31 => {
                self.alu_op(AddressingMode::IndirectY, 0, bus);
                5
            }

            // ORA
            0x09 => {
                self.alu_op(AddressingMode::Immediate, 1, bus);
                2
            }
            0x05 => {
                self.alu_op(AddressingMode::ZeroPage, 1, bus);
                3
            }
            0x15 => {
                self.alu_op(AddressingMode::ZeroPageX, 1, bus);
                4
            }
            0x0D => {
                self.alu_op(AddressingMode::Absolute, 1, bus);
                4
            }
            0x1D => {
                self.alu_op(AddressingMode::AbsoluteX, 1, bus);
                4
            }
            0x19 => {
                self.alu_op(AddressingMode::AbsoluteY, 1, bus);
                4
            }
            0x01 => {
                self.alu_op(AddressingMode::IndirectX, 1, bus);
                6
            }
            0x11 => {
                self.alu_op(AddressingMode::IndirectY, 1, bus);
                5
            }

            // EOR
            0x49 => {
                self.alu_op(AddressingMode::Immediate, 2, bus);
                2
            }
            0x45 => {
                self.alu_op(AddressingMode::ZeroPage, 2, bus);
                3
            }
            0x55 => {
                self.alu_op(AddressingMode::ZeroPageX, 2, bus);
                4
            }
            0x4D => {
                self.alu_op(AddressingMode::Absolute, 2, bus);
                4
            }
            0x5D => {
                self.alu_op(AddressingMode::AbsoluteX, 2, bus);
                4
            }
            0x59 => {
                self.alu_op(AddressingMode::AbsoluteY, 2, bus);
                4
            }
            0x41 => {
                self.alu_op(AddressingMode::IndirectX, 2, bus);
                6
            }
            0x51 => {
                self.alu_op(AddressingMode::IndirectY, 2, bus);
                5
            }

            // ADC
            0x69 => {
                self.alu_op(AddressingMode::Immediate, 3, bus);
                2
            }
            0x65 => {
                self.alu_op(AddressingMode::ZeroPage, 3, bus);
                3
            }
            0x75 => {
                self.alu_op(AddressingMode::ZeroPageX, 3, bus);
                4
            }
            0x6D => {
                self.alu_op(AddressingMode::Absolute, 3, bus);
                4
            }
            0x7D => {
                self.alu_op(AddressingMode::AbsoluteX, 3, bus);
                4
            }
            0x79 => {
                self.alu_op(AddressingMode::AbsoluteY, 3, bus);
                4
            }
            0x61 => {
                self.alu_op(AddressingMode::IndirectX, 3, bus);
                6
            }
            0x71 => {
                self.alu_op(AddressingMode::IndirectY, 3, bus);
                5
            }

            // SBC
            0xE9 => {
                self.alu_op(AddressingMode::Immediate, 4, bus);
                2
            }
            0xE5 => {
                self.alu_op(AddressingMode::ZeroPage, 4, bus);
                3
            }
            0xF5 => {
                self.alu_op(AddressingMode::ZeroPageX, 4, bus);
                4
            }
            0xED => {
                self.alu_op(AddressingMode::Absolute, 4, bus);
                4
            }
            0xFD => {
                self.alu_op(AddressingMode::AbsoluteX, 4, bus);
                4
            }
            0xF9 => {
                self.alu_op(AddressingMode::AbsoluteY, 4, bus);
                4
            }
            0xE1 => {
                self.alu_op(AddressingMode::IndirectX, 4, bus);
                6
            }
            0xF1 => {
                self.alu_op(AddressingMode::IndirectY, 4, bus);
                5
            }

            // TAX
            0xAA => {
                self.x = self.a;
                self.update_zero_and_negative_flags(self.x);
                2
            }
            // TXA
            0x8A => {
                self.a = self.x;
                self.update_zero_and_negative_flags(self.a);
                2
            }
            // TAY
            0xA8 => {
                self.y = self.a;
                self.update_zero_and_negative_flags(self.y);
                2
            }
            // TYA
            0x98 => {
                self.a = self.y;
                self.update_zero_and_negative_flags(self.a);
                2
            }

            // INC
            0xE6 | 0xF6 | 0xEE | 0xFE => {
                let mode = match opcode {
                    0xE6 => AddressingMode::ZeroPage,
                    0xF6 => AddressingMode::ZeroPageX,
                    0xEE => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let val = bus.read(addr).wrapping_add(1);
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0xE6 => 5,
                    0xF6 => 6,
                    0xEE => 6,
                    _ => 7,
                }
            }

            // DEC
            0xC6 | 0xD6 | 0xCE | 0xDE => {
                let mode = match opcode {
                    0xC6 => AddressingMode::ZeroPage,
                    0xD6 => AddressingMode::ZeroPageX,
                    0xCE => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let val = bus.read(addr).wrapping_sub(1);
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0xC6 => 5,
                    0xD6 => 6,
                    0xCE => 6,
                    _ => 7,
                }
            }

            // INX
            0xE8 => {
                self.x = self.x.wrapping_add(1);
                self.update_zero_and_negative_flags(self.x);
                2
            }
            // DEX
            0xCA => {
                self.x = self.x.wrapping_sub(1);
                self.update_zero_and_negative_flags(self.x);
                2
            }
            // INY
            0xC8 => {
                self.y = self.y.wrapping_add(1);
                self.update_zero_and_negative_flags(self.y);
                2
            }
            // DEY
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                self.update_zero_and_negative_flags(self.y);
                2
            }

            // CLC
            0x18 => {
                self.status &= !CARRY;
                2
            }
            // SEC
            0x38 => {
                self.status |= CARRY;
                2
            }
            // CLI
            0x58 => {
                self.status &= !INTERRUPT;
                2
            }
            // SEI
            0x78 => {
                self.status |= INTERRUPT;
                2
            }
            // CLV
            0xB8 => {
                self.status &= !OVERFLOW;
                2
            }
            // CLD
            0xD8 => {
                self.status &= !DECIMAL;
                2
            }
            // SED
            0xF8 => {
                self.status |= DECIMAL;
                2
            }

            // JMP absolute
            0x4C => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                self.pc = (high << 8) | low;
                3
            }
            // JMP indirect
            0x6C => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                let ptr = (high << 8) | low;
                let lo = bus.read(ptr) as u16;
                let hi = if (ptr & 0x00FF) == 0x00FF {
                    bus.read(ptr & 0xFF00) as u16
                } else {
                    bus.read(ptr + 1) as u16
                };
                self.pc = (hi << 8) | lo;
                5
            }

            // JSR
            0x20 => {
                let low = bus.read(self.pc) as u16;
                let high = bus.read(self.pc + 1) as u16;
                let addr = (high << 8) | low;
                self.pc = self.pc.wrapping_add(2);
                self.push_u16(bus, self.pc.wrapping_sub(1));
                self.pc = addr;
                6
            }
            // RTS
            0x60 => {
                let addr = self.pop_u16(bus);
                self.pc = addr.wrapping_add(1);
                6
            }
            // RTI
            0x40 => {
                self.status = (self.pop(bus) & !BREAK) | BREAK2;
                self.pc = self.pop_u16(bus);
                6
            }

            // Branches
            0x10 => self.branch(bus, (self.status & NEGATIVE) == 0), // BPL
            0x30 => self.branch(bus, (self.status & NEGATIVE) != 0), // BMI
            0x50 => self.branch(bus, (self.status & OVERFLOW) == 0), // BVC
            0x70 => self.branch(bus, (self.status & OVERFLOW) != 0), // BVS
            0x90 => self.branch(bus, (self.status & CARRY) == 0),    // BCC
            0xB0 => self.branch(bus, (self.status & CARRY) != 0),    // BCS
            0xD0 => self.branch(bus, (self.status & ZERO) == 0),     // BNE
            0xF0 => self.branch(bus, (self.status & ZERO) != 0),     // BEQ

            // BIT
            0x24 | 0x2C => {
                let mode = if opcode == 0x24 {
                    AddressingMode::ZeroPage
                } else {
                    AddressingMode::Absolute
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let val = bus.read(addr);
                if (self.a & val) == 0 {
                    self.status |= ZERO;
                } else {
                    self.status &= !ZERO;
                }
                self.status = (self.status & !0xC0) | (val & 0xC0);
                if opcode == 0x24 {
                    3
                } else {
                    4
                }
            }

            // Stack operations
            0x48 => {
                self.push(bus, self.a);
                3
            } // PHA
            0x68 => {
                self.a = self.pop(bus);
                self.update_zero_and_negative_flags(self.a);
                4
            } // PLA
            0x08 => {
                self.push(bus, self.status | BREAK | BREAK2);
                3
            } // PHP
            0x28 => {
                self.status = (self.pop(bus) & !BREAK) | BREAK2;
                4
            } // PLP
            0x9A => {
                self.sp = self.x;
                2
            } // TXS
            0xBA => {
                self.x = self.sp;
                self.update_zero_and_negative_flags(self.x);
                2
            } // TSX

            // ASL accumulator
            0x0A => {
                if (self.a & 0x80) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                self.a <<= 1;
                self.update_zero_and_negative_flags(self.a);
                2
            }
            // ASL memory
            0x06 | 0x16 | 0x0E | 0x1E => {
                let mode = match opcode {
                    0x06 => AddressingMode::ZeroPage,
                    0x16 => AddressingMode::ZeroPageX,
                    0x0E => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let mut val = bus.read(addr);
                if (val & 0x80) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                val <<= 1;
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0x06 => 5,
                    0x16 => 6,
                    0x0E => 6,
                    _ => 7,
                }
            }

            // LSR accumulator
            0x4A => {
                if (self.a & 0x01) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                self.a >>= 1;
                self.update_zero_and_negative_flags(self.a);
                2
            }
            // LSR memory
            0x46 | 0x56 | 0x4E | 0x5E => {
                let mode = match opcode {
                    0x46 => AddressingMode::ZeroPage,
                    0x56 => AddressingMode::ZeroPageX,
                    0x4E => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let mut val = bus.read(addr);
                if (val & 0x01) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                val >>= 1;
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0x46 => 5,
                    0x56 => 6,
                    0x4E => 6,
                    _ => 7,
                }
            }

            // ROL accumulator
            0x2A => {
                let old_carry = self.status & CARRY;
                if (self.a & 0x80) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                self.a = (self.a << 1) | old_carry;
                self.update_zero_and_negative_flags(self.a);
                2
            }
            // ROL memory
            0x26 | 0x36 | 0x2E | 0x3E => {
                let mode = match opcode {
                    0x26 => AddressingMode::ZeroPage,
                    0x36 => AddressingMode::ZeroPageX,
                    0x2E => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let mut val = bus.read(addr);
                let old_carry = self.status & CARRY;
                if (val & 0x80) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                val = (val << 1) | old_carry;
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0x26 => 5,
                    0x36 => 6,
                    0x2E => 6,
                    _ => 7,
                }
            }

            // ROR accumulator
            0x6A => {
                let old_carry = self.status & CARRY;
                if (self.a & 0x01) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                self.a = (self.a >> 1) | (old_carry << 7);
                self.update_zero_and_negative_flags(self.a);
                2
            }
            // ROR memory
            0x66 | 0x76 | 0x6E | 0x7E => {
                let mode = match opcode {
                    0x66 => AddressingMode::ZeroPage,
                    0x76 => AddressingMode::ZeroPageX,
                    0x6E => AddressingMode::Absolute,
                    _ => AddressingMode::AbsoluteX,
                };
                let (addr, _) = self.get_operand_address(mode, bus);
                self.pc = self.pc.wrapping_add(self.get_instruction_len(mode));
                let mut val = bus.read(addr);
                let old_carry = self.status & CARRY;
                if (val & 0x01) != 0 {
                    self.status |= CARRY;
                } else {
                    self.status &= !CARRY;
                }
                val = (val >> 1) | (old_carry << 7);
                bus.write(addr, val);
                self.update_zero_and_negative_flags(val);
                match opcode {
                    0x66 => 5,
                    0x76 => 6,
                    0x6E => 6,
                    _ => 7,
                }
            }

            _ => {
                // Fallback NOP for robust timing execution
                self.pc = inst_pc.wrapping_add(1);
                2
            }
        };

        let extra_cycles = (self.cycles - start_cycles) as u32;
        self.cycles = start_cycles + cycles as u64 + extra_cycles as u64;
        cycles + extra_cycles
    }
}
