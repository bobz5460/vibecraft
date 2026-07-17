pub mod aquifer;
pub mod biome_source;
pub mod carver;
pub mod caves;
pub mod density_fn;
pub mod decoration;
pub mod features;
pub mod generator;
pub mod noise;
pub mod noise_router;
pub mod surface;
pub mod structures;
pub mod structure_geometry;
pub mod template;
pub mod terrain;

pub use biome_source::{OverworldBiomeSource, UnsupportedBiome};
pub use generator::VanillaWorldGenerator;

/// Overworld biomes representable by the current native chunk and renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Plains,
    Forest,
    BirchForest,
    OldGrowthBirchForest,
    PaleGarden,
    Desert,
    Savanna,
    SavannaPlateau,
    Taiga,
    SnowyTaiga,
    SnowyPlains,
    IceSpikes,
    Swamp,
    MangroveSwamp,
    Jungle,
    SparseJungle,
    DarkForest,
    Ocean,
    DeepOcean,
    WarmOcean,
    LukewarmOcean,
    ColdOcean,
    FrozenOcean,
    StonyShore,
    SnowyBeach,
    DeepLukewarmOcean,
    DeepColdOcean,
    DeepFrozenOcean,
    Beach,
    MushroomFields,
    BambooJungle,
    Badlands,
    WoodedBadlands,
    ErodedBadlands,
    River,
    FrozenRiver,
    FlowerForest,
    SunflowerPlains,
    CherryGrove,
    Meadow,
    Grove,
    SnowySlopes,
    JaggedPeaks,
    FrozenPeaks,
    StonyPeaks,
    OldGrowthPineTaiga,
    OldGrowthSpruceTaiga,
    WindsweptHills,
    WindsweptForest,
    WindsweptSavanna,
    WindsweptGravellyHills,
    DripstoneCaves,
    LushCaves,
    SulfurCaves,
    DeepDark,
}

/// The exact Overworld biome identities registered by Minecraft Java 26.2.
pub const MINECRAFT26_OVERWORLD_BIOMES: [Biome; 55] = [
    Biome::Plains,
    Biome::SunflowerPlains,
    Biome::SnowyPlains,
    Biome::IceSpikes,
    Biome::Desert,
    Biome::Swamp,
    Biome::MangroveSwamp,
    Biome::Forest,
    Biome::FlowerForest,
    Biome::BirchForest,
    Biome::DarkForest,
    Biome::PaleGarden,
    Biome::OldGrowthBirchForest,
    Biome::OldGrowthPineTaiga,
    Biome::OldGrowthSpruceTaiga,
    Biome::Taiga,
    Biome::SnowyTaiga,
    Biome::Savanna,
    Biome::SavannaPlateau,
    Biome::WindsweptHills,
    Biome::WindsweptGravellyHills,
    Biome::WindsweptForest,
    Biome::WindsweptSavanna,
    Biome::Jungle,
    Biome::SparseJungle,
    Biome::BambooJungle,
    Biome::Badlands,
    Biome::ErodedBadlands,
    Biome::WoodedBadlands,
    Biome::Meadow,
    Biome::CherryGrove,
    Biome::Grove,
    Biome::SnowySlopes,
    Biome::FrozenPeaks,
    Biome::JaggedPeaks,
    Biome::StonyPeaks,
    Biome::River,
    Biome::FrozenRiver,
    Biome::Beach,
    Biome::SnowyBeach,
    Biome::StonyShore,
    Biome::WarmOcean,
    Biome::LukewarmOcean,
    Biome::DeepLukewarmOcean,
    Biome::Ocean,
    Biome::DeepOcean,
    Biome::ColdOcean,
    Biome::DeepColdOcean,
    Biome::FrozenOcean,
    Biome::DeepFrozenOcean,
    Biome::MushroomFields,
    Biome::DripstoneCaves,
    Biome::LushCaves,
    Biome::DeepDark,
    Biome::SulfurCaves,
];
