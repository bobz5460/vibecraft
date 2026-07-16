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
/// loop and, for the explicitly named preview profile, native decorations.
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
}

impl WorldGenerationProfile {
    pub const fn new_world() -> Self {
        Self::Minecraft26Base
    }

    pub const fn legacy() -> Self {
        Self::LegacyPreCorrectedInterpolation
    }

    pub const fn uses_corrected_interpolation(self) -> bool {
        matches!(
            self,
            Self::Minecraft26Base | Self::Minecraft26NativeDecorationPreview
        )
    }

    pub const fn uses_native_decoration_preview(self) -> bool {
        matches!(self, Self::Minecraft26NativeDecorationPreview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_migrated_world_profiles_select_their_intended_interpolation() {
        assert!(WorldGenerationProfile::new_world().uses_corrected_interpolation());
        assert!(!WorldGenerationProfile::new_world().uses_native_decoration_preview());
        assert!(!WorldGenerationProfile::legacy().uses_corrected_interpolation());
        assert!(!WorldGenerationProfile::Minecraft26Base.uses_native_decoration_preview());
    }
}
