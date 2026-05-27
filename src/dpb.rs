//! CP/M disk parameter block and boot support for the IMSAI 8080 emulator
//!
//! This module defines the CP/M 2.2 disk parameter block (DPB) for the
//! Tarbell controller's standard 8-inch single-density format, and
//! provides the boot ROM that loads CP/M from disk into memory.
//!
//! The standard Tarbell DPB (IBM 3740 compatible):
//!
//! | Parameter        | Value | Description                        |
//! |------------------|-------|------------------------------------|
//! | SPT              | 26    | Sectors per track (128-byte)       |
//! | BSH              | 3     | Block shift (data alloc block = 1K) |
//! | BLM              | 7     | Block mask                         |
//! | EXM              | 0     | Extent mask                        |
//! | DSM              | 242   | Maximum block number (243 blocks)  |
//! | DRM              | 63    | Maximum directory entry            |
//! | AL0              | 0xC0  | Directory alloc bits, high         |
//! | AL1              | 0x00  | Directory alloc bits, low          |
//! | CKS              | 8     | Check vector size                 |
//! | OFF              | 6     | Track offset (reserved tracks)    |
//!
//! The CP/M system occupies tracks 0-5 (6 reserved tracks). The data
//! area starts at track 6. Directory entries use 2 allocation blocks
//! (blocks 0 and 1), giving 64 directory entries.

/// Sectors per track (128-byte sectors)
pub const SPT: u16 = 26;
/// Block shift factor (log2 of bytes per allocation block: 2^3 = 1024)
pub const BSH: u8 = 3;
/// Block mask (bytes per allocation block - 1: 1024 - 1 = 1023, but stored as 7)
pub const BLM: u8 = 7;
/// Extent mask
pub const EXM: u8 = 0;
/// Maximum data block number (242 = 0xF2, blocks 0-242 usable)
pub const DSM: u16 = 242;
/// Maximum directory entry number (63 = entries 0-63)
pub const DRM: u16 = 63;
/// Directory allocation bits, high byte (blocks 0 and 1 reserved for directory)
pub const AL0: u8 = 0xC0;
/// Directory allocation bits, low byte
pub const AL1: u8 = 0x00;
/// Check vector size (directory entry bytes for hashing)
pub const CKS: u16 = 16;
/// Number of reserved (system) tracks
pub const OFF: u16 = 2;
/// Bytes per sector
pub const SECTOR_SIZE: usize = 128;
/// Bytes per allocation block
pub const BLOCK_SIZE: usize = 1024;
/// Number of reserved tracks (same as OFF, here for clarity)
pub const RESERVED_TRACKS: u8 = 2;
/// Total tracks per disk
pub const TOTAL_TRACKS: u8 = 77;
/// Directory entries
pub const DIRECTORY_ENTRIES: u16 = DRM + 1;
/// Allocation blocks
pub const ALLOC_BLOCKS: u16 = DSM + 1;

/// The CP/M Disk Parameter Block as stored in memory
///
/// This 16-byte structure lives in the CP/M BIOS and tells the BDOS
/// how the disk is laid out. The format matches the CP/M 2.2 DPB
/// specification exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskParameterBlock {
    /// Sectors per track
    pub spt: u16,
    /// Block shift factor
    pub bsh: u8,
    /// Block mask
    pub blm: u8,
    /// Extent mask
    pub exm: u8,
    /// Maximum data block number
    pub dsm: u16,
    /// Maximum directory entry number
    pub drm: u16,
    /// Allocation bits high
    pub al0: u8,
    /// Allocation bits low
    pub al1: u8,
    /// Check vector size
    pub cks: u16,
    /// Track offset (reserved tracks)
    pub off: u16,
}

impl Default for DiskParameterBlock {
    fn default() -> Self {
        Self::tarbell_standard()
    }
}

impl DiskParameterBlock {
    /// Create the standard Tarbell 8-inch single-density DPB
    pub fn tarbell_standard() -> Self {
        Self {
            spt: SPT,
            bsh: BSH,
            blm: BLM,
            exm: EXM,
            dsm: DSM,
            drm: DRM,
            al0: AL0,
            al1: AL1,
            cks: CKS,
            off: OFF,
        }
    }

    /// Serialize the DPB into the 15-byte CP/M format
    ///
    /// CP/M stores the DPB as 15 bytes in memory:
    ///   SPT(2) + BSH(1) + BLM(1) + EXM(1) + DSM(2) + DRM(2) + AL0(1) + AL1(1) + CKS(2) + OFF(2)
    pub fn to_bytes(&self) -> [u8; 15] {
        let mut buf = [0u8; 15];
        buf[0..2].copy_from_slice(&self.spt.to_le_bytes());
        buf[2] = self.bsh;
        buf[3] = self.blm;
        buf[4] = self.exm;
        buf[5..7].copy_from_slice(&self.dsm.to_le_bytes());
        buf[7..9].copy_from_slice(&self.drm.to_le_bytes());
        buf[9] = self.al0;
        buf[10] = self.al1;
        buf[11..13].copy_from_slice(&self.cks.to_le_bytes());
        buf[13..15].copy_from_slice(&self.off.to_le_bytes());
        buf
    }

    /// Calculate the number of reserved sectors (system tracks * sectors per track)
    pub fn reserved_sectors(&self) -> u16 {
        self.off * self.spt
    }

    /// Calculate total data capacity in bytes (excludes reserved tracks)
    pub fn data_capacity(&self) -> u32 {
        let data_tracks = TOTAL_TRACKS as u32 - self.off as u32;
        data_tracks * self.spt as u32 * SECTOR_SIZE as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpb_default_is_tarbell() {
        let dpb = DiskParameterBlock::default();
        assert_eq!(dpb.spt, 26);
        assert_eq!(dpb.bsh, 3);
        assert_eq!(dpb.blm, 7);
        assert_eq!(dpb.exm, 0);
        assert_eq!(dpb.dsm, 242);
        assert_eq!(dpb.drm, 63);
        assert_eq!(dpb.al0, 0xC0);
        assert_eq!(dpb.al1, 0x00);
        assert_eq!(dpb.cks, 16);
        assert_eq!(dpb.off, 2);
    }

    #[test]
    fn test_dpb_serialization() {
        let dpb = DiskParameterBlock::tarbell_standard();
        let bytes = dpb.to_bytes();

        // SPT = 26 (0x1A, 0x00 LE)
        assert_eq!(bytes[0], 0x1A);
        assert_eq!(bytes[1], 0x00);
        // BSH = 3
        assert_eq!(bytes[2], 3);
        // BLM = 7
        assert_eq!(bytes[3], 7);
        // EXM = 0
        assert_eq!(bytes[4], 0);
        // DSM = 242 (0xF2, 0x00 LE)
        assert_eq!(bytes[5], 0xF2);
        assert_eq!(bytes[6], 0x00);
        // DRM = 63 (0x3F, 0x00 LE)
        assert_eq!(bytes[7], 0x3F);
        assert_eq!(bytes[8], 0x00);
        // AL0 = 0xC0, AL1 = 0x00
        assert_eq!(bytes[9], 0xC0);
        assert_eq!(bytes[10], 0x00);
        // CKS = 16
        assert_eq!(bytes[11], 0x10);
        assert_eq!(bytes[12], 0x00);
        // OFF = 2
        assert_eq!(bytes[13], 0x02);
        assert_eq!(bytes[14], 0x00);
    }

    #[test]
    fn test_dpb_reserved_sectors() {
        let dpb = DiskParameterBlock::tarbell_standard();
        // 2 reserved tracks * 26 sectors = 52 sectors
        assert_eq!(dpb.reserved_sectors(), 52);
    }

    #[test]
    fn test_dpb_data_capacity() {
        let dpb = DiskParameterBlock::tarbell_standard();
        // 75 data tracks * 26 sectors * 128 bytes = 249,600 bytes
        assert_eq!(dpb.data_capacity(), 249_600);
    }
}