use fce_core::core::bus::SimpleBus;
use fce_core::core::cpu::Cpu;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut rom_path = None;
    let mut log_path = None;
    let mut frames = 0;
    let mut checksum_flag = false;
    let mut inputs_str = None;

    let mut i = 1;
    let mut save_path = None;
    let mut audio_path = None;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                if i + 1 < args.len() {
                    rom_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --rom",
                    ));
                }
            }
            "--log" => {
                if i + 1 < args.len() {
                    log_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --log",
                    ));
                }
            }
            "--frames" => {
                if i + 1 < args.len() {
                    frames = args[i + 1].parse::<usize>().unwrap_or(0);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --frames",
                    ));
                }
            }
            "--checksum" => {
                checksum_flag = true;
                i += 1;
            }
            "--inputs" => {
                if i + 1 < args.len() {
                    inputs_str = Some(&args[i + 1]);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --inputs",
                    ));
                }
            }
            "--save" => {
                if i + 1 < args.len() {
                    save_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --save",
                    ));
                }
            }
            "--audio" => {
                if i + 1 < args.len() {
                    audio_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --audio",
                    ));
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    let rom_path = match rom_path {
        Some(p) => p,
        None => {
            println!("Usage: headless --rom <path> [--log <path>] [--frames <number>] [--checksum] [--inputs <string>]");
            return Ok(());
        }
    };

    // Read the ROM file
    let rom_data = std::fs::read(rom_path)?;

    let mut bus = SimpleBus::new();
    let cartridge = match fce_core::core::cartridge::Cartridge::from_rom(&rom_data) {
        Ok(cart) => cart,
        Err(e) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to load cartridge: {}", e),
            ));
        }
    };

    println!(
        "Loaded Cartridge successfully. Mapper {}",
        cartridge.mapper_id
    );
    bus.load_cartridge(cartridge);

    // Parse inputs if provided
    let mut input_map = HashMap::new();
    if let Some(s) = inputs_str {
        for part in s.split(',') {
            if part.is_empty() {
                continue;
            }
            let subparts: Vec<&str> = part.split(':').collect();
            if subparts.len() != 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid input format: {}", part),
                ));
            }
            let range_str = subparts[0].trim();
            let mask_str = subparts[1].trim();

            let mask = if mask_str.starts_with("0x") || mask_str.starts_with("0X") {
                u8::from_str_radix(&mask_str[2..], 16)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            } else {
                mask_str
                    .parse::<u8>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            };

            if range_str.contains('-') {
                let bounds: Vec<&str> = range_str.split('-').collect();
                if bounds.len() != 2 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Invalid frame range: {}", range_str),
                    ));
                }
                let start = bounds[0]
                    .trim()
                    .parse::<usize>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                let end = bounds[1]
                    .trim()
                    .parse::<usize>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                for f in start..=end {
                    input_map.insert(f, mask);
                }
            } else {
                let f = range_str
                    .parse::<usize>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                input_map.insert(f, mask);
            }
        }
    }

    if let Some(log_p) = log_path {
        let mut cpu = Cpu::new();
        // Set non-interactive automated nestest execution starting PC and cycles
        cpu.pc = 0xC000;
        cpu.cycles = 7; // reference log starts cycles at 7
        let mut log_file = File::create(log_p)?;

        // Loop and step the CPU 8991 times, matching the reference log count
        for _ in 0..8991 {
            let pc = cpu.pc;
            let log_line = format!(
                "{:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}\n",
                pc, cpu.a, cpu.x, cpu.y, cpu.status, cpu.sp, cpu.cycles
            );
            log_file.write_all(log_line.as_bytes())?;
            cpu.step(&mut bus);
        }
    } else if frames > 0 {
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        let mut frame_count = 0;

        while frame_count < frames {
            let current_frame = frame_count + 1;
            bus.controller_state = *input_map.get(&current_frame).unwrap_or(&0);

            bus.ppu_frame_complete = false;
            while !bus.ppu_frame_complete {
                let cycles = cpu.step(&mut bus);
                bus.apu.tick(cycles);
            }
            frame_count += 1;
        }

        println!(
            "Simulated {} execution frames. Total CPU cycles: {}",
            frames, cpu.cycles
        );

        if checksum_flag {
            let digest = md5::compute(*bus.ppu.frame_buffer);
            println!("Frame MD5: {:x}", digest);
        }

        if let Some(save_p) = save_path {
            let buffer = *bus.ppu.frame_buffer;
            image::save_buffer(
                save_p,
                &buffer,
                256,
                240,
                image::ColorType::Rgba8,
            ).map_err(io::Error::other)?;
            println!("Saved frame to {}", save_p);
        }

        if let Some(audio_p) = audio_path {
            let mut file = File::create(audio_p)?;
            let samples = &bus.apu.sample_buffer;
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr() as *const u8,
                    samples.len() * std::mem::size_of::<f32>(),
                )
            };
            file.write_all(bytes)?;
            println!("Saved {} raw f32 audio samples to {}", samples.len(), audio_p);
        }
    }

    Ok(())
}
