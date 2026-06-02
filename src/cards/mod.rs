//! S-100 bus card implementations
//!
//! Cards compose chip-level models with board-level logic (address decoding,
//! wait state generators, drive select circuitry, etc.).
//!
//! Current cards:
//! - MemoryCard: 64K RAM (full address space)
//! - SerialCard: IMSAI SIO-2 board (2x Intel 8251A UART)
//! - TarbellCard: Tarbell 1011 floppy controller (WD FD1771 + board logic)
//! - FrontPanel: IMSAI 8080 front panel (switches + LEDs)

mod front_panel;
mod memory;
mod serial;
mod tarbell;

pub use front_panel::FrontPanel;
pub use front_panel::IoEvent;
pub use front_panel::PanelLeds;
pub use front_panel::PanelSwitch;
pub use front_panel::RunState;
pub use memory::MemoryCard;
pub use memory::save_memory_to_file;
pub use memory::load_memory_from_file;
pub use serial::SerialCard;
pub use tarbell::TarbellCard;

/// An S-100 bus card.
///
/// Cards respond to two kinds of bus transactions:
/// - Memory transactions (mem_read/mem_write) for the address bus
/// - I/O transactions (io_read/io_write) for the port bus
///
/// A memory card (RAM) responds to memory transactions.
/// A peripheral card (Serial, Tarbell) responds to I/O transactions.
/// Some cards could do both (e.g., memory-mapped I/O or ROM boards).
///
/// The front panel is NOT a Card: it directly accesses the bus and CPU
/// for examine/deposit/run/stop operations. It doesn't respond to I/O ports.
pub trait Card {
    /// Read from an I/O port this card owns.
    fn io_read(&mut self, port: u8) -> u8;
    /// Write to an I/O port this card owns.
    fn io_write(&mut self, port: u8, value: u8);
    /// Does this card respond to the given I/O port?
    fn owns_port(&self, port: u8) -> bool;

    /// Read from a memory address this card owns.
    fn mem_read(&self, addr: u16) -> Option<u8>;
    /// Write to a memory address this card owns.
    fn mem_write(&mut self, addr: u16, value: u8) -> bool;
    /// Does this card own the given memory address?
    fn owns_address(&self, addr: u16) -> bool;

    /// Human-readable name for diagnostics.
    fn name(&self) -> &'static str;
    /// Downcast support (mutable).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    /// Downcast support (immutable).
    fn as_any(&self) -> &dyn std::any::Any;
}