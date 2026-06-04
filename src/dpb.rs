
/// Sectors per track (128-byte sectors)
pub const SPT: u16 = 26;
/// Block mask (bytes per allocation block - 1: 1024 - 1 = 1023, stored as 7)
pub const BLM: u8 = 7;
/// Maximum data block number (242 = 0xF2)
pub const DSM: u16 = 242;
/// Maximum directory entry number (63 = entries 0-63)
pub const DRM: u16 = 63;
/// Number of reserved (system) tracks
pub const OFF: u16 = 2;
/// Bytes per sector
pub const SECTOR_SIZE: usize = 128;
/// Bytes per allocation block
pub const BLOCK_SIZE: usize = 1024;
/// Total tracks per disk
pub const TOTAL_TRACKS: u8 = 77;

/// Standard IBM 3740 6:1 interleave skew table (logical-to-physical sector mapping)
///
/// Maps logical sector numbers 0-25 to physical sector numbers 1-26.
/// This is the canonical single source of truth for the Tarbell controller
/// sector translation. Used by `disk.rs` for logical-to-physical mapping.
pub const SKEW_TABLE: [u8; 26] = [
    1, 7, 13, 19, 25, 5, 11, 17, 23, 3, 9, 15, 21, 2, 8, 14, 20, 26, 6, 12, 18, 24, 4,
    10, 16, 22,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpb_constants() {
        assert_eq!(SPT, 26);
        assert_eq!(BLM, 7);
        assert_eq!(DSM, 242);
        assert_eq!(DRM, 63);
        assert_eq!(OFF, 2);
        assert_eq!(SECTOR_SIZE, 128);
        assert_eq!(BLOCK_SIZE, 1024);
        assert_eq!(TOTAL_TRACKS, 77);
    }
}