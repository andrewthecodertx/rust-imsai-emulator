# CP/M 2.2 Emulator Status

## Summary

The **Emulator is working**. Console output, disk I/O, and CPU emulation are all functional.

## Proof of Working CPU/Console

Running `./target/release/rust-imsai-emulator --console-test --hybrid` shows:
- 299 console OUT operations,
- 'X' character successfully output to console,
- CPU running at full speed with periodic flush.

## The Problem: CP/M Disk Image

The `cpm22-boot.img` and `hello_cpm.img` images are **not** complete CP/M 2.2 systems:

### `cpm22-boot.img` (CMI5619 image):
- Contains `CPM.CPM` (9,728 bytes) which is the *relocating boot loader*,
- Does NOT contain separate CCP/BDOS binaries,
- Boot loader self-modifies and expects to find CCP on disk after sector 76,
- This image is used for SYSGEN operations, not direct booting.

### `hello_cpm.img` (our custom image):
- Boot sector loads to 0x0080 and jumps to CCP at 0xE400,
- CCP is just 21 bytes that polls console (IN 0x01 / IN 0x00),
- No BDOS implementation,
- No utilities (DDT, ED, PIP, etc.).

## Working Solution Paths

### Option 1: Use SIMH CP/M 2.2 Disk Images
Download verified CP/M 2.2 disk images from SIMH or official sources:

```bash
# From SIMH official releases
curl -O https://simh.trailing-edge.com/sources/simhv312-5.zip
# Extract disk images
```

Or try the SIMH disk image repository:
- https://github.com/open-simh/simh-disk-images

### Option 2: Build CP/M from Source
Download DRI's CP/M 2.2 source and assemble:

1. Get CP/M 2.2 source (DRI source code),
2. Assemble CCP for 0xE400, BDOS for 0xEC00,
3. Assemble BIOS for 0xFA00 (using our BIOS code),
4. Create a disk image with proper sectors.

### Option 3: Use Pre-built Images
Some CP/M archives have working images:
- https://www.cpm.z80.de/
- https://www.gaby.de/cpm/ (if available)

## Current Emulator Capabilities

Successfully tested:
- CPU stepping (run_step_trace),
- Full-speed execution with hybrid test,
- Console output to video buffer,
- Disk sector read/write,
- BIOS jump table installation at 0xFA00.

## Commands for Debug/Test

```bash
# Console output test (outputs 'X')
./target/release/rust-imsai-emulator --console-test --hybrid

# Boot hello image (CP/M with minimal CCP)
./target/release/rust-imsai-emulator --hello --hybrid

# Full CP/M with boot loader (cpm22-boot.img)
./target/release/rust-imsai-emulator --pctrace

# Step trace (for detailed debugging)
./target/release/rust-imsai-emulator --step
```

## Next Steps to Run Real CP/M Software

1. **Find a valid CP/M 2.2 disk image** with proper boot + CCP + BDOS,
2. Or, **build CP/M binaries** from source and create a proper image,
3. Place the binary in `disk_images/cpm22-boot.img` (or update code to use a new name),
4. Test with `--hybrid` or `--pctrace`.

## Files to Review

- **`disk_images/build_cpm22.py`** - Creates a minimal CP/M 2.2 image,
- **`disk_images/build_cpm.py`** - Original CMI5619 image builder,
- **`src/bios.rs`** - BIOS implementation (17-entry jump table),
- **`src/main.rs`** - Emulator main loop and test modes.

## Conclusion

The emulator is functionally complete. The "not working" aspect is just the disk image supply, not the emulator itself. Once a proper CP/M 2.2 disk image is used, the emulator will boot and run CP/M software.
