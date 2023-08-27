//! ## Disk Names
//! 
//! Collection of handy names for various disk kinds and track layouts.
//! These are mainly for pattern matching.
//! 
//! A disk kind comprises both mechanical and magnetic properties of
//! a disk.  It should not be confused with a file system.  For example,
//! the DOS33 kind could (and did) contain ProDOS, CP/M, or Pascal.

use super::{DiskKind,TrackLayout,BlockLayout,NibbleCode,FluxCode,DataRate};

macro_rules! uni {
    ($x:expr) => {
        [$x,0,0,0,0]
    };
}

pub const A2_DOS32: TrackLayout = TrackLayout {
    cylinders: uni!(35),
    sides: uni!(1),
    sectors: uni!(13),
    sector_size: uni!(256),
    flux_code: [FluxCode::GCR;5],
    nib_code: [NibbleCode::N53;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const A2_DOS33: TrackLayout = TrackLayout {
    cylinders: uni!(35),
    sides: uni!(1),
    sectors: uni!(16),
    sector_size: uni!(256),
    flux_code: [FluxCode::GCR;5],
    nib_code: [NibbleCode::N62;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const A2_400: TrackLayout = TrackLayout {
    cylinders: [16,16,16,16,16],
    sides: [1,1,1,1,1],
    sector_size: [524,524,524,524,524],
    sectors: [12,11,10,9,8],
    flux_code: [FluxCode::GCR;5],
    nib_code: [NibbleCode::N62;5],
    data_rate: [DataRate::R500Kbps;5]
};

pub const A2_800: TrackLayout = TrackLayout {
    cylinders: [16,16,16,16,16],
    sides: [2,2,2,2,2],
    sector_size: [524,524,524,524,524],
    sectors: [12,11,10,9,8],
    flux_code: [FluxCode::GCR;5],
    nib_code: [NibbleCode::N62;5],
    data_rate: [DataRate::R500Kbps;5]
};

pub const CPM_1: TrackLayout = TrackLayout {
    cylinders: uni!(77),
    sides: uni!(1),
    sector_size: uni!(128),
    sectors: uni!(26),
    flux_code: [FluxCode::FM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R500Kbps;5]
};

pub const AMSTRAD_184K: TrackLayout = TrackLayout {
    cylinders: uni!(40),
    sides: uni!(1),
    sector_size: uni!(512),
    sectors: uni!(9),
    flux_code: [FluxCode::MFM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const OSBORNE1_SD: TrackLayout = TrackLayout {
    cylinders: uni!(40),
    sides: uni!(1),
    sector_size: uni!(256),
    sectors: uni!(10),
    flux_code: [FluxCode::FM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const OSBORNE1_DD: TrackLayout = TrackLayout {
    cylinders: uni!(40),
    sides: uni!(1),
    sector_size: uni!(1024),
    sectors: uni!(5),
    flux_code: [FluxCode::MFM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const TRS80_M2_CPM: TrackLayout = TrackLayout {
    cylinders: [1,76,0,0,0],
    sides: [1,1,0,0,0],
    sector_size: [128,512,0,0,0],
    sectors: [26,16,0,0,0],
    flux_code: [FluxCode::FM,FluxCode::MFM,FluxCode::None,FluxCode::None,FluxCode::None],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R500Kbps;5]
};

pub const NABU_CPM: TrackLayout = TrackLayout {
    cylinders: [1,76,0,0,0],
    sides: [2,2,0,0,0],
    sector_size: [128,256,0,0,0],
    sectors: [26,26,0,0,0],
    flux_code: [FluxCode::FM,FluxCode::MFM,FluxCode::None,FluxCode::None,FluxCode::None],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R500Kbps;5]
};

pub const KAYPROII: TrackLayout = TrackLayout {
    cylinders: uni!(40),
    sides: uni!(1),
    sector_size: uni!(512),
    sectors: uni!(10),
    flux_code: [FluxCode::MFM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R250Kbps;5]
};

pub const KAYPRO4: TrackLayout = TrackLayout {
    cylinders: uni!(40),
    sides: uni!(2),
    sector_size: uni!(512),
    sectors: uni!(10),
    flux_code: [FluxCode::MFM;5],
    nib_code: [NibbleCode::None;5],
    data_rate: [DataRate::R250Kbps;5]
};

// This kind might contain DOS 3.0, 3.1, or 3.2.
pub const A2_DOS32_KIND: DiskKind = DiskKind::D525(A2_DOS32);

/// This kind might contain DOS 3.3, ProDOS, CP/M, or Pascal
pub const A2_DOS33_KIND: DiskKind = DiskKind::D525(A2_DOS33);

/// This kind might contain ProDOS
pub const A2_400_KIND: DiskKind = DiskKind::D35(A2_400);

/// This kind might contain ProDOS
pub const A2_800_KIND: DiskKind = DiskKind::D35(A2_800);

/// This kind might contain CP/M
pub const IBM_CPM1_KIND: DiskKind = DiskKind::D8(CPM_1);

/// This kind might contain ProDOS
pub const A2_HD_MAX: DiskKind = DiskKind::LogicalBlocks(BlockLayout {block_count: 65535, block_size: 512});

/// This kind might contain CP/M
pub const AMSTRAD_184K_KIND: DiskKind = DiskKind::D525(AMSTRAD_184K);

/// This kind might contain CP/M
pub const OSBORNE1_SD_KIND: DiskKind = DiskKind::D525(OSBORNE1_SD);

/// This kind might contain CP/M
pub const OSBORNE1_DD_KIND: DiskKind = DiskKind::D525(OSBORNE1_DD);

/// This kind might contain CP/M
pub const TRS80_M2_CPM_KIND: DiskKind = DiskKind::D8(TRS80_M2_CPM);

/// This kind might contain CP/M
pub const NABU_CPM_KIND: DiskKind = DiskKind::D8(NABU_CPM);

/// This kind might contain CP/M
pub const KAYPROII_KIND: DiskKind = DiskKind::D525(KAYPROII);

/// This kind might contain CP/M
pub const KAYPRO4_KIND: DiskKind = DiskKind::D525(KAYPRO4);