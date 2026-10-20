//! How many page tiles the machine can keep on the GPU. The compositor may use 60 percent of the machine's memory (the
//! share formvis-render uses), and a tile pixel costs 4 bytes: the tile's texture. The frame itself is drawn straight
//! to the surface, so there are no further copies to count. wgpu does not report the graphics processor's own memory,
//! so the budget is the system's RAM; on machines with separate video memory the tiles also live there, which this
//! does not yet measure.

use crate::TILE;

/// The share of memory the compositor may use.
pub const SHARE: f64 = 0.6;
/// Bytes one tile pixel costs.
pub const BYTES_PER_PIXEL: u64 = 4;
/// Bytes one tile costs on the GPU.
pub const TILE_BYTES: u64 = (TILE * TILE) as u64 * BYTES_PER_PIXEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The machine's memory.
    pub ram: u64,
    /// What the compositor may use.
    pub bytes: u64,
    /// The most tiles that fit in it.
    pub max_tiles: u64,
}

/// The budget for a machine with `ram` bytes of memory.
pub fn budget_for(ram: u64) -> Budget {
    let bytes = (ram as f64 * SHARE) as u64;
    Budget { ram, bytes, max_tiles: bytes / TILE_BYTES }
}

/// The budget for this machine.
pub fn system_budget() -> Budget {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    budget_for(sys.total_memory())
}
