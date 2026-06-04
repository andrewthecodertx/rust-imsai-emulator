/// Parsed command-line arguments.
pub struct Args {
    pub load_path: Option<String>,
    pub load_addr: u16,
    pub program_path: Option<String>,
    pub disk_path: Option<String>,
    pub cmd_text: Option<String>,
    pub step_trace: bool,
    pub trace: bool,
    pub verbose_trace: bool,
    pub diag: bool,
    pub pc_trace: bool,
    pub batch: bool,
    pub script: bool,
}

/// Parse command-line arguments into an `Args` struct.
pub fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage(&args);
        std::process::exit(0);
    }

    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());
    let diag = args.contains(&"--diag".to_string()) || args.contains(&"-d".to_string());
    let step_trace = args.contains(&"--step".to_string()) || args.contains(&"-s".to_string());
    let pc_trace = args.contains(&"--pctrace".to_string()) || args.contains(&"-p".to_string());
    let batch = args.contains(&"--batch".to_string()) || args.contains(&"-b".to_string());
    let script = args.contains(&"--script".to_string());

    let cmd_text = args
        .iter()
        .position(|a| a == "--cmd")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());

    let load_path = args
        .iter()
        .position(|a| a == "--load")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    let program_path = args
        .iter()
        .position(|a| a == "--program")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    let disk_path = args
        .iter()
        .position(|a| a == "--disk")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    let load_addr: u16 = if load_path.is_some() {
        let load_pos = args.iter().position(|a| a == "--load").unwrap_or(0);
        args.get(load_pos + 2)
            .and_then(|s| {
                u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Args {
        load_path,
        load_addr,
        program_path,
        disk_path,
        cmd_text,
        step_trace,
        trace,
        verbose_trace,
        diag,
        pc_trace,
        batch,
        script,
    }
}

fn print_usage(args: &Vec<String>) {
    eprintln!("IMSAI 8080 Emulator - Terminal Mode");
    eprintln!();
    eprintln!(
        "Usage: {} [OPTIONS]",
        args.get(0).unwrap_or(&"imsai-cli".to_string())
    );
    eprintln!();
    eprintln!("Mode (choose one):");
    eprintln!("  --load <file> [addr]      Load raw binary at address (default 0x0000)");
    eprintln!("  --program <file.json>      Load a front panel program (.json)");
    eprintln!("  (no arguments)             Start with saved memory (or empty if first run)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --disk <file>              Mount disk image in drive A");
    eprintln!("  --batch, -b                Batch mode (non-interactive, 50M instructions)");
    eprintln!("  --trace, -t                Trace every instruction");
    eprintln!("  --vtrace, -v               Verbose trace (with I/O logging)");
    eprintln!("  --diag, -d                 Diagnostic mode (I/O log + region tracking)");
    eprintln!("  --step, -s                 Step trace (first 500 instructions)");
    eprintln!("  --pctrace, -p              PC ring-buffer trace (last 8K instructions)");
    eprintln!("  --script                   Scripted mode (captures console output)");
    eprintln!("  --cmd \"text\"             Pre-load keyboard input for scripted testing");
    eprintln!("  --help, -h                 Show this help");
}

