# AGENTS.md

IMSAI 8080 emulator (Rust). Only the raylib front-panel GUI binary
(`imsai-gui`) remains; the terminal CLI/TUI was removed. Full details in
`CLAUDE.md`, `README.md`, `docs/`; this file is the high-signal subset an
agent would otherwise have to infer.

## Build & run

- The only binary is `imsai-gui` (raylib front panel). It builds by default:
  `cargo run --bin imsai-gui`.
- **`gui` feature gates raylib.** It is a default feature, so `cargo build`/
  `cargo test` pull in raylib and build the GUI. Use `--no-default-features`
  for a fast headless library-only build/test (no raylib C library needed);
  the `required-features = ["gui"]` on the binary keeps `--no-default-features`
  from trying to build it.
- Rust 1.80+. CPU is an external git crate pinned to rev `aaea08f` in
  `Cargo.toml`; don't bump it without testing.

## Testing

- `cargo test` runs unit + integration tests. `cargo test <name>` filters by
  substring (e.g. `cargo test test_front_panel_deposit`).
- Tests construct `Imsai8080::new()` directly and drive `emu.step()` /
  `emu.process_panel()` — no TTY, no disk, no raylib. Safe to run headless.
- Doctests are disabled (`doctest = false` in `[lib]`).

## Non-obvious behavior

- **Memory persists to `imsai_memory.json` in the cwd.** The GUI saves on exit
  and restores on next launch when no `--load`/`--program` is given. This is a
  real file written next to wherever you run cargo — test artifacts and
  working directory matter. GUI `R` key cold-resets (clears memory + deletes the
  file).
- **Disk booting does not work.** Disks mount and the FD1771 is modeled, but
  there is no BIOS/boot loader. Don't claim or assume disk boot.
- **The front panel is not on the I/O bus.** It reads the CPU/address bus
  directly. Don't route front-panel logic through `io_in`/`io_out`.
- **Two run states are distinct:** `panel.run_state` (RUN/STOP) vs `cpu.halted`
  (`HLT`). `run_batch` stops on either; pressing RUN must clear `cpu.halted`.
- RAM powers on to `0xFF` (floating bus) — programs must not assume zeroed
  memory.
- `--shot <path>` is a hidden GUI flag (headless screenshot verification).

## Architecture (what's not obvious from filenames)

- Layering: `chips/` (silicon models, no bus knowledge) → `cards/` (board
  logic) → `bus.rs` (`ImsaiBus`, implements `intel8080::Bus`) → `emulator.rs`
  (`Imsai8080` = cpu + bus + panel).
- **No `Card` trait / dynamic dispatch.** Cards are concrete struct fields on
  `ImsaiBus` (`memory`, `serial`, `tarbell`). Accessors: `bus.serial()`,
  `bus.tarbell()`, `bus.memory` (public field).
- Port map: console UART at 0x00–0x03 (aliases 0x79/0x7B), Tarbell FD1771 at
  0x48–0x4B (aliases 0xF8–0xFF). Unclaimed ports read `0xFF`.
- Disk geometry lives only in `src/dpb.rs` — change it there, not in literals.

## Conventions

- Front-panel programs are JSON in `programs/` (`load`/`deposit`/`examine`/`run`
  steps), parsed by `src/program.rs`. Prefer the `load` action (writes bytes
  directly) over switch-by-switch steps.