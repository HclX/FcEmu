//! Integration tests for the NES emulator core.
//! These tests verify the interaction between CPU, PPU, APU, and Bus.

use fce_core::core::bus::{CpuBus, SimpleBus};
use fce_core::core::cpu::Cpu;
use fce_core::core::ppu::Ppu;
use fce_core::core::region::{EmulatorRegion, NTSC_TIMING, PAL_TIMING};

#[test]
fn test_cpu_reset_reads_reset_vector() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    // Write reset vector at $FFFC/$FFFD
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    assert_eq!(cpu.pc, 0x8000);
}

#[test]
fn test_cpu_nop_takes_2_cycles() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    bus.mem[0x8000] = 0xEA; // NOP
    let cycles = cpu.step(&mut bus);
    assert_eq!(cycles, 2);
    assert_eq!(cpu.pc, 0x8001);
}

#[test]
fn test_cpu_lda_immediate() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    bus.mem[0x8000] = 0xA9; // LDA #imm
    bus.mem[0x8001] = 0x42;
    let cycles = cpu.step(&mut bus);
    assert_eq!(cycles, 2);
    assert_eq!(cpu.a, 0x42);
}

#[test]
fn test_cpu_adc_carry() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    cpu.a = 0xFF;
    bus.mem[0x8000] = 0x69; // ADC #imm
    bus.mem[0x8001] = 0x01;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.status & 0x01 != 0, "Carry should be set");
    assert!(cpu.status & 0x02 != 0, "Zero should be set");
}

#[test]
fn test_cpu_sbc_borrow() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    cpu.a = 0x50;
    cpu.status |= 0x01; // Set carry (no borrow)
    bus.mem[0x8000] = 0xE9; // SBC #imm
    bus.mem[0x8001] = 0x30;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x20);
    assert!(cpu.status & 0x01 != 0, "Carry should be set (no borrow)");
}

#[test]
fn test_cpu_jmp_indirect_page_boundary_bug() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    // JMP ($02FF) - the famous 6502 page wrap bug
    bus.mem[0x8000] = 0x6C; // JMP indirect
    bus.mem[0x8001] = 0xFF;
    bus.mem[0x8002] = 0x02;
    bus.mem[0x02FF] = 0x34;
    bus.mem[0x0200] = 0x12; // Should wrap to $0200, not $0300
    bus.mem[0x0300] = 0x56; // Wrong byte if no page wrap bug
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x1234);
}

#[test]
fn test_cpu_jsr_rts_roundtrip() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    // JSR $9000
    bus.mem[0x8000] = 0x20; // JSR
    bus.mem[0x8001] = 0x00;
    bus.mem[0x8002] = 0x90;
    // At $9000: RTS
    bus.mem[0x9000] = 0x60; // RTS
    cpu.step(&mut bus); // JSR
    assert_eq!(cpu.pc, 0x9000);
    cpu.step(&mut bus); // RTS
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_cpu_stack_push_pop() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    // PHA ($48) followed by LDA #0, then PLA ($68)
    cpu.a = 0x42;
    bus.mem[0x8000] = 0x48; // PHA
    bus.mem[0x8001] = 0xA9; // LDA #0
    bus.mem[0x8002] = 0x00;
    bus.mem[0x8003] = 0x68; // PLA
    cpu.step(&mut bus); // PHA
    cpu.step(&mut bus); // LDA #0
    assert_eq!(cpu.a, 0x00);
    cpu.step(&mut bus); // PLA
    assert_eq!(cpu.a, 0x42);
}

#[test]
fn test_cpu_branch_taken_not_taken() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // BEQ with zero flag set - should branch
    cpu.status |= 0x02; // Set zero flag
    bus.mem[0x8000] = 0xF0; // BEQ
    bus.mem[0x8001] = 0x05; // +5
    let cycles = cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x8007); // 0x8002 + 5
    assert_eq!(cycles, 3); // 2 base + 1 taken, no page cross
}

#[test]
fn test_cpu_branch_page_crossing() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0xF0; // Start near page boundary
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);
    // BNE with forward branch that crosses page
    cpu.status &= !0x02; // Clear zero flag
    bus.mem[0x80F0] = 0xD0; // BNE
    bus.mem[0x80F1] = 0x20; // +32 (crosses from $80xx to $81xx)
    let cycles = cpu.step(&mut bus);
    assert_eq!(cycles, 4); // 2 base + 1 taken + 1 page cross
    assert_eq!(cpu.pc, 0x8112);
}

#[test]
fn test_cpu_and_or_eor() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // AND #$0F
    cpu.a = 0xFF;
    bus.mem[0x8000] = 0x29; // AND #imm
    bus.mem[0x8001] = 0x0F;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x0F);

    // ORA #$F0
    bus.mem[0x8002] = 0x09; // ORA #imm
    bus.mem[0x8003] = 0xF0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xFF);

    // EOR #$FF
    bus.mem[0x8004] = 0x49; // EOR #imm
    bus.mem[0x8005] = 0xFF;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.status & 0x02 != 0, "Zero flag should be set");
}

#[test]
fn test_cpu_inc_dec() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // INC $10
    bus.mem[0x0010] = 0x05;
    bus.mem[0x8000] = 0xE6; // INC zp
    bus.mem[0x8001] = 0x10;
    cpu.step(&mut bus);
    assert_eq!(bus.mem[0x0010], 0x06);

    // DEC $10
    bus.mem[0x8002] = 0xC6; // DEC zp
    bus.mem[0x8003] = 0x10;
    cpu.step(&mut bus);
    assert_eq!(bus.mem[0x0010], 0x05);
}

#[test]
fn test_cpu_bit_instruction() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // BIT $10 - tests bits 7 and 6 of memory, AND with A
    bus.mem[0x0010] = 0xC0; // bits 7 and 6 set
    cpu.a = 0x00;
    bus.mem[0x8000] = 0x24; // BIT zp
    bus.mem[0x8001] = 0x10;
    cpu.step(&mut bus);
    assert!(cpu.status & 0x80 != 0, "Negative should be set (bit 7)");
    assert!(cpu.status & 0x40 != 0, "Overflow should be set (bit 6)");
    assert!(cpu.status & 0x02 != 0, "Zero should be set (A & M == 0)");
}

#[test]
fn test_cpu_shift_operations() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // ASL A
    cpu.a = 0x81;
    bus.mem[0x8000] = 0x0A; // ASL A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x02);
    assert!(cpu.status & 0x01 != 0, "Carry should be set");

    // LSR A
    cpu.a = 0x03;
    bus.mem[0x8001] = 0x4A; // LSR A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x01);
    assert!(cpu.status & 0x01 != 0, "Carry should be set");

    // ROL A (with carry set)
    cpu.a = 0x80;
    cpu.status |= 0x01;
    bus.mem[0x8002] = 0x2A; // ROL A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x01);
    assert!(cpu.status & 0x01 != 0, "Carry should be set");
}

#[test]
fn test_ppu_palette_mirroring() {
    let ppu = Ppu::new();
    // Sprite palette background colors mirror to BG palette
    assert_eq!(ppu.get_palette_addr(0x3F10), 0); // $3F10 -> $3F00
    assert_eq!(ppu.get_palette_addr(0x3F14), 4); // $3F14 -> $3F04
    assert_eq!(ppu.get_palette_addr(0x3F18), 8); // $3F18 -> $3F08
    assert_eq!(ppu.get_palette_addr(0x3F1C), 12); // $3F1C -> $3F0C
                                                  // Non-mirrored addresses stay the same
    assert_eq!(ppu.get_palette_addr(0x3F01), 1);
    assert_eq!(ppu.get_palette_addr(0x3F11), 17);
}

#[test]
fn test_ntsc_timing_constants() {
    assert_eq!(NTSC_TIMING.total_scanlines, 262);
    assert_eq!(NTSC_TIMING.pre_render_scanline, 261);
    assert_eq!(NTSC_TIMING.ppu_accum_mult, 3);
    assert_eq!(NTSC_TIMING.ppu_accum_div, 1);
}

#[test]
fn test_pal_timing_constants() {
    assert_eq!(PAL_TIMING.total_scanlines, 312);
    assert_eq!(PAL_TIMING.pre_render_scanline, 311);
    assert_eq!(PAL_TIMING.ppu_accum_mult, 16);
    assert_eq!(PAL_TIMING.ppu_accum_div, 5);
}

#[test]
fn test_region_switching() {
    let mut bus = SimpleBus::new();
    bus.set_region(EmulatorRegion::Pal);
    assert_eq!(bus.timing.region, EmulatorRegion::Pal);
    assert_eq!(bus.ppu.timing.region, EmulatorRegion::Pal);
    assert_eq!(bus.apu.timing.region, EmulatorRegion::Pal);
}

#[test]
fn test_ram_mirroring() {
    let mut bus = SimpleBus::new();
    // Write to $0000 should be readable at $0800, $1000, $1800
    bus.write(0x0000, 0xAB);
    assert_eq!(bus.read(0x0000), 0xAB);
    assert_eq!(bus.read(0x0800), 0xAB);
    assert_eq!(bus.read(0x1000), 0xAB);
    assert_eq!(bus.read(0x1800), 0xAB);

    // Write at mirror should reflect to base
    bus.write(0x1234, 0xCD);
    assert_eq!(bus.read(0x0234), 0xCD); // 0x1234 & 0x07FF = 0x0434... actually 0x1234 & 0x07FF = 0x0234
}

#[test]
fn test_controller_strobe() {
    let mut bus = SimpleBus::new();
    bus.controller_state = 0x05; // A and Select

    // Strobe high then low
    bus.write(0x4016, 1);
    bus.write(0x4016, 0);

    // Read button bits sequentially
    // Bit 0: A (1), Bit 1: B (0), Bit 2: Select (1), etc.
    let a = bus.read(0x4016) & 0x01;
    let b = bus.read(0x4016) & 0x01;
    let select = bus.read(0x4016) & 0x01;
    assert_eq!(a, 1);
    assert_eq!(b, 0);
    assert_eq!(select, 1);
}

#[test]
fn test_ppu_accumulator_ntsc() {
    let mut bus = SimpleBus::new();
    // NTSC: 3 PPU cycles per CPU cycle (exact)
    let ppu_cycles = bus.accumulate_ppu_cycles(1);
    assert_eq!(ppu_cycles, 3);

    let ppu_cycles = bus.accumulate_ppu_cycles(10);
    assert_eq!(ppu_cycles, 30);
}

#[test]
fn test_cpu_flags_after_transfer() {
    let mut bus = SimpleBus::new();
    let mut cpu = Cpu::new();
    bus.mem[0xFFFC] = 0x00;
    bus.mem[0xFFFD] = 0x80;
    cpu.reset(&mut bus);

    // TAX with A=0 should set zero flag
    cpu.a = 0x00;
    bus.mem[0x8000] = 0xAA; // TAX
    cpu.step(&mut bus);
    assert_eq!(cpu.x, 0x00);
    assert!(cpu.status & 0x02 != 0, "Zero flag should be set");

    // TAX with A=0x80 should set negative flag
    cpu.a = 0x80;
    bus.mem[0x8001] = 0xAA; // TAX
    cpu.step(&mut bus);
    assert_eq!(cpu.x, 0x80);
    assert!(cpu.status & 0x80 != 0, "Negative flag should be set");
}

#[test]
fn test_frame_boundary_cycle_counts() {
    // NTSC: ~29780.5 cycles/frame
    let ntsc_cycles_per_frame = (NTSC_TIMING.cpu_clock_speed / 60.0) as u32;
    assert!(
        ntsc_cycles_per_frame > 29700 && ntsc_cycles_per_frame < 29900,
        "NTSC cycles/frame: {}",
        ntsc_cycles_per_frame
    );
}
