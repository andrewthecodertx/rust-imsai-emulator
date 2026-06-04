#![warn(missing_docs)]

pub mod bus;
pub mod cards;
pub mod chips;
pub mod disk;
pub mod dpb;
pub mod emulator;
pub mod io;
pub mod program;

// Re-export the main components
pub use bus::ImsaiBus;
pub use cards::{load_memory_from_file, save_memory_to_file};
pub use cards::{
    FrontPanel, IoEvent, MemoryCard, PanelLeds, PanelSwitch, RunState, SerialCard, TarbellCard,
};
pub use chips::Fd1771;
pub use chips::Uart8251;
pub use disk::DiskImage;
pub use emulator::Imsai8080;
pub use io::Keyboard;
pub use io::VideoDisplay;
pub use program::{
    execute_panel_program, find_program_start, load_program_file, memory_to_program, parse_hex16,
    parse_hex8, parse_hex_bytes, save_program_file, PanelProgram, PanelStep,
};
