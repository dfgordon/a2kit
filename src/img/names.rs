//! ## Disk Names
//! 
//! Collection of handy names for various disk kinds and sector layouts.
//! A disk kind comprises both mechanical and magnetic properties of
//! a disk.  It should not be confused with a file system.  For example,
//! the DOS33 kind could (and did) contain ProDOS, CP/M, or Pascal.

use super::{DiskKind,SectorLayout,BlockLayout,NibbleCode,FluxCode};

pub const A2_DOS32_SECS: SectorLayout = SectorLayout {
    cylinders: 35,
    sides: 1,
    zones: 1,
    sectors: 13,
    sector_size: 256
};

pub const A2_DOS33_SECS: SectorLayout = SectorLayout {
    cylinders: 35,
    sides: 1,
    zones: 1,
    sectors: 16,
    sector_size: 256
};

pub const A2_400_SECS: SectorLayout = SectorLayout {
    cylinders: 80,
    sides: 1,
    zones: 5,
    sector_size: 524,
    sectors: 12
};

pub const A2_800_SECS: SectorLayout = SectorLayout {
    cylinders: 80,
    sides: 2,
    zones: 5,
    sector_size: 524,
    sectors: 12
};

pub const CPM_1_SECS: SectorLayout = SectorLayout {
    cylinders: 77,
    sides: 1,
    zones: 1,
    sector_size: 128,
    sectors: 26
};

pub const IBM_200_SECS: SectorLayout = SectorLayout {
    cylinders: 40,
    sides: 1,
    zones: 1,
    sector_size: 1024,
    sectors: 5
};

// This kind might contain DOS 3.0, 3.1, or 3.2.
pub const A2_DOS32_KIND: DiskKind = DiskKind::D525(A2_DOS32_SECS,NibbleCode::N53,FluxCode::GCR);

/// This kind might contain DOS 3.3, ProDOS, CP/M, or Pascal
pub const A2_DOS33_KIND: DiskKind = DiskKind::D525(A2_DOS33_SECS,NibbleCode::N62,FluxCode::GCR);

/// This kind might contain ProDOS
pub const A2_400_KIND: DiskKind = DiskKind::D35(A2_400_SECS, NibbleCode::N62,FluxCode::GCR);

/// This kind might contain ProDOS
pub const A2_800_KIND: DiskKind = DiskKind::D35(A2_800_SECS, NibbleCode::N62,FluxCode::GCR);

/// This kind might contain CP/M
pub const IBM_CPM1_KIND: DiskKind = DiskKind::D8(CPM_1_SECS,NibbleCode::None,FluxCode::FM);

/// This kind might contain ProDOS
pub const A2_HD_MAX: DiskKind = DiskKind::LogicalBlocks(BlockLayout {block_count: 65535, block_size: 512});

/// This kind might contain CP/M
pub const OSBORNE_KIND: DiskKind = DiskKind::D525(IBM_200_SECS,NibbleCode::None,FluxCode::MFM);
