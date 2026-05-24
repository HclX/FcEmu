import os

def create_nes_rom(path, prg_code):
    # 16-byte iNES header
    header = bytearray([
        0x4E, 0x45, 0x53, 0x1A, # "NES\x1a"
        0x01,                   # 1 bank of PRG-ROM (16KB)
        0x00,                   # 0 banks of CHR-ROM (CHR-RAM)
        0x00,                   # Mapper 0, horizontal mirroring
        0x00,                   # Mapper 0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 # padding
    ])
    
    # PRG-ROM size is 16384 bytes
    prg_data = bytearray(16384)
    
    # Copy code to the beginning of PRG-ROM (mapped to $C000)
    for i, b in enumerate(prg_code):
        prg_data[i] = b
        
    # Set reset vector at $FFFC-$FFFD (offset 16380-16381 in PRG-ROM)
    # Points to $C000
    prg_data[16380] = 0x00
    prg_data[16381] = 0xC0
    
    with open(path, 'wb') as f:
        f.write(header)
        f.write(prg_data)
    print(f"Created {path}")

def main():
    os.makedirs("tests/roms", exist_ok=True)
    
    # 1. Pass ROM
    # LDA #$80; STA $6000
    # LDA #$DE; STA $6001
    # LDA #$B0; STA $6002
    # LDA #$61; STA $6003
    # LDA #$00; STA $6000
    # JMP $C019 (loop)
    pass_code = [
        0xA9, 0x80, 0x8D, 0x00, 0x60,
        0xA9, 0xDE, 0x8D, 0x01, 0x60,
        0xA9, 0xB0, 0x8D, 0x02, 0x60,
        0xA9, 0x61, 0x8D, 0x03, 0x60,
        0xA9, 0x00, 0x8D, 0x00, 0x60,
        0x4C, 0x19, 0xC0
    ]
    create_nes_rom("tests/roms/blargg_mock_pass.nes", pass_code)
    
    # 2. Fail ROM
    # LDA #$80; STA $6000
    # LDA #$DE; STA $6001
    # LDA #$B0; STA $6002
    # LDA #$61; STA $6003
    # LDA #'F'; STA $6004
    # LDA #'a'; STA $6005
    # LDA #'i'; STA $6006
    # LDA #'l'; STA $6007
    # LDA #0;   STA $6008
    # LDA #$01; STA $6000
    # JMP $C032 (loop)
    fail_code = [
        0xA9, 0x80, 0x8D, 0x00, 0x60,
        0xA9, 0xDE, 0x8D, 0x01, 0x60,
        0xA9, 0xB0, 0x8D, 0x02, 0x60,
        0xA9, 0x61, 0x8D, 0x03, 0x60,
        0xA9, 0x46, 0x8D, 0x04, 0x60,
        0xA9, 0x61, 0x8D, 0x05, 0x60,
        0xA9, 0x69, 0x8D, 0x06, 0x60,
        0xA9, 0x6C, 0x8D, 0x07, 0x60,
        0xA9, 0x00, 0x8D, 0x08, 0x60,
        0xA9, 0x01, 0x8D, 0x00, 0x60,
        0x4C, 0x32, 0xC0
    ]
    create_nes_rom("tests/roms/blargg_mock_fail.nes", fail_code)

    # 3. Reset ROM
    # $C000: LDA #$80; STA $6000
    # $C005: LDA #$DE; STA $6001
    # $C00A: LDA #$B0; STA $6002
    # $C00F: LDA #$61; STA $6003
    # $C014: LDA $0000
    # $C017: CMP #$55
    # $C019: BEQ $C028
    # $C01B: LDA #$55; STA $0000
    # $C020: LDA #$81; STA $6000
    # $C025: JMP $C025
    # $C028: LDA #$00; STA $6000
    # $C02D: JMP $C02D
    reset_code = [
        0xA9, 0x80, 0x8D, 0x00, 0x60,
        0xA9, 0xDE, 0x8D, 0x01, 0x60,
        0xA9, 0xB0, 0x8D, 0x02, 0x60,
        0xA9, 0x61, 0x8D, 0x03, 0x60,
        0xAD, 0x00, 0x00,
        0xC9, 0x55,
        0xF0, 0x0D,
        0xA9, 0x55, 0x8D, 0x00, 0x00,
        0xA9, 0x81, 0x8D, 0x00, 0x60,
        0x4C, 0x25, 0xC0,
        0xA9, 0x00, 0x8D, 0x00, 0x60,
        0x4C, 0x2D, 0xC0
    ]
    create_nes_rom("tests/roms/blargg_mock_reset.nes", reset_code)

if __name__ == "__main__":
    main()
