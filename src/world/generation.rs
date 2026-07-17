//! Immutable per-world generation behavior selections.
//!
//! These profiles are independent from public world-coordinate mapping. They
//! preserve the generator behavior that created an existing world's chunks so
//! later streaming cannot create seams at old/new chunk boundaries.

use serde::{Deserialize, Serialize};

/// Selects immutable generation behavior for newly generated chunks.
///
/// The profile is persisted with each native level and must not change after
/// world creation. It controls the X-slice swap in the density interpolation
/// loop and the explicitly versioned post-terrain feature stage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldGenerationProfile {
    /// Reproduces the pre-correction interpolation behavior for existing worlds.
    #[serde(rename = "legacy_pre_corrected_interpolation")]
    LegacyPreCorrectedInterpolation,
    /// Uses the corrected Minecraft 26 base interpolation behavior for new worlds.
    #[serde(rename = "minecraft26_base")]
    Minecraft26Base,
    /// Uses the corrected base plus bounded native decoration preview features.
    ///
    /// This is intentionally not a Java feature-index or placed-feature
    /// compatibility mode. It is only selected for newly created worlds.
    #[serde(rename = "minecraft26_native_decoration_preview")]
    Minecraft26NativeDecorationPreview,
    /// Uses the corrected base plus the bounded Minecraft 26.2-oriented
    /// placed-feature geometry stage.
    #[serde(rename = "minecraft26_geometry")]
    Minecraft26Geometry,
}

impl WorldGenerationProfile {
    pub const fn new_world() -> Self {
        Self::Minecraft26Geometry
    }

    pub const fn legacy() -> Self {
        Self::LegacyPreCorrectedInterpolation
    }

    pub const fn uses_corrected_interpolation(self) -> bool {
        matches!(
            self,
            Self::Minecraft26Base
                | Self::Minecraft26NativeDecorationPreview
                | Self::Minecraft26Geometry
        )
    }

    pub const fn uses_native_decoration_preview(self) -> bool {
        matches!(self, Self::Minecraft26NativeDecorationPreview)
    }

    pub const fn uses_minecraft26_geometry(self) -> bool {
        matches!(self, Self::Minecraft26Geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_migrated_world_profiles_select_their_intended_interpolation() {
        assert!(WorldGenerationProfile::new_world().uses_corrected_interpolation());
        assert!(!WorldGenerationProfile::new_world().uses_native_decoration_preview());
        assert!(WorldGenerationProfile::new_world().uses_minecraft26_geometry());
        assert!(!WorldGenerationProfile::legacy().uses_corrected_interpolation());
        assert!(!WorldGenerationProfile::Minecraft26Base.uses_native_decoration_preview());
        assert!(!WorldGenerationProfile::Minecraft26Base.uses_minecraft26_geometry());
    }

    #[test]
    fn geometry_profile_has_a_stable_serde_name() {
        assert_eq!(
            serde_json::to_string(&WorldGenerationProfile::Minecraft26Geometry).unwrap(),
            "\"minecraft26_geometry\""
        );
        assert_eq!(
            serde_json::from_str::<WorldGenerationProfile>("\"minecraft26_geometry\"").unwrap(),
            WorldGenerationProfile::Minecraft26Geometry
        );
    }
}
