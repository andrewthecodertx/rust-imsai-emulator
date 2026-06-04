use rust_imsai_emulator::{execute_panel_program, find_program_start, load_program_file};
use rust_imsai_emulator::{load_memory_from_file, save_memory_to_file};

mod cli;
mod disasm;
mod trace;
mod tui;

fn main() {
    let args = cli::parse_args();

    let mut emu = rust_imsai_emulator::Imsai8080::new();
    let mut start_pc: Option<u16> = None;

    if let Some(ref path) = args.program_path {
        let pbuf = std::path::PathBuf::from(path);
        match load_program_file(&pbuf) {
            Ok(prog) => {
                let start = find_program_start(&prog).unwrap_or(0);
                eprintln!("Loaded program: {} (start at 0x{:04X})", prog.name, start);
                if let Err(e) = execute_panel_program(&mut emu, &prog) {
                    eprintln!("Program execution error: {}", e);
                }
                start_pc = Some(start);
            }
            Err(e) => {
                eprintln!("Error loading program '{}': {}", path, e);
                return;
            }
        }
    } else if let Some(ref path) = args.load_path {
        match std::fs::read(path) {
            Ok(data) => {
                eprintln!(
                    "Loaded {} bytes from {} at 0x{:04X}",
                    data.len(),
                    path,
                    args.load_addr
                );
                emu.load_program(args.load_addr, &data);
                start_pc = Some(args.load_addr);
            }
            Err(e) => {
                eprintln!("Error loading '{}': {}", path, e);
                return;
            }
        }
    } else {
        let mem_path = std::path::Path::new("imsai_memory.json");
        if mem_path.exists() {
            match load_memory_from_file(&mut emu.bus.memory.ram, mem_path) {
                Ok(()) => eprintln!("Restored memory from {}", mem_path.display()),
                Err(e) => eprintln!("Warning: failed to load {}: {}", mem_path.display(), e),
            }
        } else {
            eprintln!("IMSAI 8080 - Bare-metal mode (empty memory, no program loaded)");
            eprintln!("Use --load <file> or --program <file.json> to load a program.");
        }
    }

    if let Some(ref path) = args.disk_path {
        match emu.bus.insert_disk(0, path) {
            Ok(()) => eprintln!("Disk mounted in drive A: {}", path),
            Err(e) => {
                eprintln!("Error mounting disk '{}': {}", path, e);
                return;
            }
        }
    }

    if let Some(pc) = start_pc {
        emu.cpu.pc = pc;
    }

    if let Some(ref cmd) = args.cmd_text {
        let input = cmd.replace("\\r", "\r").replace("\\n", "\n");
        emu.bus.serial().type_text(&input);
    }

    // Headless run paths (trace/diag/batch/scripted) don't draw the front-panel
    // LEDs, so skip that per-instruction bookkeeping for speed. The interactive
    // TUI (the final `else`) leaves it off.
    emu.fast = args.step_trace
        || args.diag
        || args.pc_trace
        || args.verbose_trace
        || args.trace
        || args.script
        || args.batch;

    if args.step_trace {
        trace::run_step_trace(&mut emu, 500);
    } else if args.diag {
        trace::run_diag(&mut emu, 50_000);
    } else if args.pc_trace {
        trace::run_pc_trace(&mut emu, 5_000_000);
    } else if args.verbose_trace {
        trace::run_verbose_trace(&mut emu, 200_000);
    } else if args.trace {
        trace::run_trace(&mut emu, 50_000);
    } else if args.script {
        trace::run_scripted(&mut emu, args.cmd_text.as_deref(), 100_000_000);
    } else if args.batch {
        trace::run_interactive(&mut emu, 50_000_000);
    } else {
        tui::run_terminal(&mut emu, args.speed_mhz);
    }

    let mem_path = std::path::Path::new("imsai_memory.json");
    match save_memory_to_file(&emu.bus.memory.ram, mem_path) {
        Ok(()) => eprintln!("Memory saved to {}", mem_path.display()),
        Err(e) => eprintln!("Warning: failed to save {}: {}", mem_path.display(), e),
    }
}
