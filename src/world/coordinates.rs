//! Per-world mapping between public world coordinates and chunk-local storage.

use crate::world::chunk::CHUNK_HEIGHT;
use serde::{Deserialize, Serialize};

/// A world profile is immutable after creation. Chunk cells always use local
/// Y `0..CHUNK_HEIGHT`; this selects the public coordinate system around them.
/// It does not reinterpret generated or persisted chunk cells: the active
/// Overworld generator has always mapped its Java `-64..319` samples into
/// that fixed local storage range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCoordinateProfile {
    LegacyLocal,
    JavaOverworld,
}

impl WorldCoordinateProfile {
    pub const fn new_world() -> Self {
        Self::JavaOverworld
    }

    pub const fn legacy() -> Self {
        Self::LegacyLocal
    }

    pub const fn min_y(self) -> i32 {
        match self {
            Self::LegacyLocal => 0,
            Self::JavaOverworld => -64,
        }
    }

    pub const fn max_y_exclusive(self) -> i32 {
        self.min_y() + CHUNK_HEIGHT as i32
    }

    pub const fn contains_world_y(self, y: i32) -> bool {
        y >= self.min_y() && y < self.max_y_exclusive()
    }

    pub const fn to_local_y(self, y: i32) -> Option<usize> {
        if self.contains_world_y(y) {
            Some((y - self.min_y()) as usize)
        } else {
            None
        }
    }

    pub const fn from_local_y(self, y: usize) -> Option<i32> {
        if y < CHUNK_HEIGHT {
            Some(self.min_y() + y as i32)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_map_the_same_storage_height_without_reinterpreting_it() {
        assert_eq!(WorldCoordinateProfile::JavaOverworld.to_local_y(-64), Some(0));
        assert_eq!(WorldCoordinateProfile::JavaOverworld.to_local_y(319), Some(383));
        assert_eq!(WorldCoordinateProfile::JavaOverworld.to_local_y(-65), None);
        assert_eq!(WorldCoordinateProfile::JavaOverworld.to_local_y(320), None);
        assert_eq!(WorldCoordinateProfile::LegacyLocal.to_local_y(0), Some(0));
        assert_eq!(WorldCoordinateProfile::LegacyLocal.to_local_y(383), Some(383));
        assert_eq!(WorldCoordinateProfile::LegacyLocal.to_local_y(-1), None);
        assert_eq!(WorldCoordinateProfile::LegacyLocal.to_local_y(384), None);
    }
}
