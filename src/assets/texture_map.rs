use std::collections::HashMap;
use crate::world::block::{BlockId, BlockFace, FACES};
use crate::assets::blockstate::resolve_blockstate_model;
use crate::assets::model::resolve_face_textures;
use crate::assets::reader::AssetReader;

#[derive(Clone, Debug)]
pub struct TextureTile {
    pub path: String,
    pub tile_index: u32,
}

#[derive(Clone, Debug)]
pub struct BlockTextureMapping {
    reader: AssetReader,
    entries: Vec<(BlockId, [String; 6])>,
}

fn minecraft_name(id: BlockId) -> Option<&'static str> {
    use BlockId::*;
    match id {
        Stone => Some("stone"),
        GrassBlock => Some("grass_block"),
        Dirt => Some("dirt"),
        Cobblestone => Some("cobblestone"),
        OakPlanks | OakPlanks2 => Some("oak_planks"),
        Bedrock => Some("bedrock"),
        Water => Some("water_still"),
        Sand => Some("sand"),
        Gravel => Some("gravel"),
        GoldOre => Some("gold_ore"),
        IronOre => Some("iron_ore"),
        CoalOre => Some("coal_ore"),
        OakLog | OakLog2 => Some("oak_log"),
        OakLeaves | OakLeaves2 => Some("oak_leaves"),
        Glass => Some("glass"),
        CraftingTable => Some("crafting_table"),
        Furnace => Some("furnace"),
        Chest => Some("chest"),
        OakDoor => Some("oak_door"),
        OakFence => Some("oak_planks"),
        RedstoneDust => Some("redstone_dust_dot"),
        Torch | WallTorch => Some("torch"),
        Snow | Snow2 => Some("snow"),
        Ice => Some("ice"),
        Glowstone => Some("glowstone"),
        Netherrack => Some("netherrack"),
        SoulSand => Some("soul_sand"),
        Deepslate => Some("deepslate"),
        SnowBlock => Some("snow_block"),
        CoarseDirt => Some("coarse_dirt"),
        Podzol => Some("podzol"),
        DiamondBlock => Some("diamond_block"),
        IronBlock => Some("iron_block"),
        GoldBlock => Some("gold_block"),
        EmeraldBlock => Some("emerald_block"),
        LapisBlock => Some("lapis_block"),
        RedstoneBlock => Some("redstone_block"),
        Bricks => Some("bricks"),
        Bookshelf => Some("bookshelf"),
        MossyCobblestone => Some("mossy_cobblestone"),
        Obsidian => Some("obsidian"),
        Spawner => Some("spawner"),
        Sandstone => Some("sandstone"),
        StoneBricks => Some("stone_bricks"),
        Granite => Some("granite"),
        Diorite => Some("diorite"),
        Andesite => Some("andesite"),
        Calcite => Some("calcite"),
        Tuff => Some("tuff"),
        DripstoneBlock => Some("dripstone_block"),
        CobbledDeepslate => Some("cobbled_deepslate"),
        PolishedDeepslate => Some("polished_deepslate"),
        DeepslateBricks => Some("deepslate_bricks"),
        DeepslateTiles => Some("deepslate_tiles"),
        Blackstone => Some("blackstone"),
        PolishedBlackstone => Some("polished_blackstone"),
        PolishedBlackstoneBricks => Some("polished_blackstone_bricks"),
        CrimsonNylium => Some("crimson_nylium"),
        WarpedNylium => Some("warped_nylium"),
        RedSand => Some("red_sand"),
        Sponge => Some("sponge"),
        WetSponge => Some("wet_sponge"),
        LapisOre => Some("lapis_ore"),
        RedstoneOre => Some("redstone_ore"),
        EmeraldOre => Some("emerald_ore"),
        DiamondOre => Some("diamond_ore"),
        Lava => Some("lava_still"),
        Fire => Some("fire_0"),
        SoulFire => Some("soul_fire_0"),
        SoulTorch | SoulWallTorch => Some("soul_torch"),
        RedstoneTorch | RedstoneWallTorch => Some("redstone_torch"),
        RedstoneLamp => Some("redstone_lamp"),
        JackOLantern => Some("jack_o_lantern"),
        CarvedPumpkin => Some("carved_pumpkin"),
        Pumpkin => Some("pumpkin"),
        Melon => Some("melon"),
        HayBlock => Some("hay_block"),
        SeaLantern => Some("sea_lantern"),
        Shroomlight => Some("shroomlight"),
        CryingObsidian => Some("crying_obsidian"),
        NetheriteBlock => Some("netherite_block"),
        AncientDebris => Some("ancient_debris"),
        RespawnAnchor => Some("respawn_anchor"),
        Target => Some("target"),
        Lodestone => Some("lodestone"),
        HoneyBlock => Some("honey_block"),
        HoneycombBlock => Some("honeycomb_block"),
        NetherGoldOre => Some("nether_gold_ore"),
        SoulSoil => Some("soul_soil"),
        Basalt => Some("basalt"),
        PolishedBasalt => Some("polished_basalt"),
        SmoothStone => Some("smooth_stone"),
        SmoothSandstone => Some("smooth_sandstone"),
        SmoothRedSandstone => Some("smooth_red_sandstone"),
        SmoothQuartz => Some("smooth_quartz"),
        QuartzBlock => Some("quartz_block"),
        ChiseledQuartzBlock => Some("chiseled_quartz_block"),
        QuartzPillar => Some("quartz_pillar"),
        Prismarine => Some("prismarine"),
        PrismarineBricks => Some("prismarine_bricks"),
        DarkPrismarine => Some("dark_prismarine"),
        PurpurBlock => Some("purpur_block"),
        PurpurPillar => Some("purpur_pillar"),
        EndStone => Some("end_stone"),
        EndStoneBricks => Some("end_stone_bricks"),
        BoneBlock => Some("bone_block"),
        NetherBricks => Some("nether_bricks"),
        RedNetherBricks => Some("red_nether_bricks"),
        ShulkerBox => Some("shulker_box"),
        WhiteTerracotta => Some("white_terracotta"),
        OrangeTerracotta => Some("orange_terracotta"),
        MagentaTerracotta => Some("magenta_terracotta"),
        LightBlueTerracotta => Some("light_blue_terracotta"),
        YellowTerracotta => Some("yellow_terracotta"),
        LimeTerracotta => Some("lime_terracotta"),
        PinkTerracotta => Some("pink_terracotta"),
        GrayTerracotta => Some("gray_terracotta"),
        LightGrayTerracotta => Some("light_gray_terracotta"),
        CyanTerracotta => Some("cyan_terracotta"),
        PurpleTerracotta => Some("purple_terracotta"),
        BlueTerracotta => Some("blue_terracotta"),
        BrownTerracotta => Some("brown_terracotta"),
        GreenTerracotta => Some("green_terracotta"),
        RedTerracotta => Some("red_terracotta"),
        BlackTerracotta => Some("black_terracotta"),
        Terracotta => Some("terracotta"),
        WhiteConcrete => Some("white_concrete"),
        OrangeConcrete => Some("orange_concrete"),
        MagentaConcrete => Some("magenta_concrete"),
        LightBlueConcrete => Some("light_blue_concrete"),
        YellowConcrete => Some("yellow_concrete"),
        LimeConcrete => Some("lime_concrete"),
        PinkConcrete => Some("pink_concrete"),
        GrayConcrete => Some("gray_concrete"),
        LightGrayConcrete => Some("light_gray_concrete"),
        CyanConcrete => Some("cyan_concrete"),
        PurpleConcrete => Some("purple_concrete"),
        BlueConcrete => Some("blue_concrete"),
        BrownConcrete => Some("brown_concrete"),
        GreenConcrete => Some("green_concrete"),
        RedConcrete => Some("red_concrete"),
        BlackConcrete => Some("black_concrete"),
        WhiteWool => Some("white_wool"),
        OrangeWool => Some("orange_wool"),
        MagentaWool => Some("magenta_wool"),
        LightBlueWool => Some("light_blue_wool"),
        YellowWool => Some("yellow_wool"),
        LimeWool => Some("lime_wool"),
        PinkWool => Some("pink_wool"),
        GrayWool => Some("gray_wool"),
        LightGrayWool => Some("light_gray_wool"),
        CyanWool => Some("cyan_wool"),
        PurpleWool => Some("purple_wool"),
        BlueWool => Some("blue_wool"),
        BrownWool => Some("brown_wool"),
        GreenWool => Some("green_wool"),
        RedWool => Some("red_wool"),
        BlackWool => Some("black_wool"),
        WhiteStainedGlass => Some("white_stained_glass"),
        OrangeStainedGlass => Some("orange_stained_glass"),
        MagentaStainedGlass => Some("magenta_stained_glass"),
        LightBlueStainedGlass => Some("light_blue_stained_glass"),
        YellowStainedGlass => Some("yellow_stained_glass"),
        LimeStainedGlass => Some("lime_stained_glass"),
        PinkStainedGlass => Some("pink_stained_glass"),
        GrayStainedGlass => Some("gray_stained_glass"),
        LightGrayStainedGlass => Some("light_gray_stained_glass"),
        CyanStainedGlass => Some("cyan_stained_glass"),
        PurpleStainedGlass => Some("purple_stained_glass"),
        BlueStainedGlass => Some("blue_stained_glass"),
        BrownStainedGlass => Some("brown_stained_glass"),
        GreenStainedGlass => Some("green_stained_glass"),
        RedStainedGlass => Some("red_stained_glass"),
        BlackStainedGlass => Some("black_stained_glass"),
        TintedGlass => Some("tinted_glass"),
        AmethystBlock => Some("amethyst_block"),
        BuddingAmethyst => Some("budding_amethyst"),
        CopperOre => Some("copper_ore"),
        CopperBlock => Some("copper_block"),
        ExposedCopper => Some("exposed_copper"),
        WeatheredCopper => Some("weathered_copper"),
        OxidizedCopper => Some("oxidized_copper"),
        CutCopper => Some("cut_copper"),
        ExposedCutCopper => Some("exposed_cut_copper"),
        WeatheredCutCopper => Some("weathered_cut_copper"),
        OxidizedCutCopper => Some("oxidized_cut_copper"),
        RawCopperBlock => Some("raw_copper_block"),
        RawIronBlock => Some("raw_iron_block"),
        RawGoldBlock => Some("raw_gold_block"),
        MangroveRoots => Some("mangrove_roots"),
        MuddyMangroveRoots => Some("muddy_mangrove_roots"),
        Mud => Some("mud"),
        PackedMud => Some("packed_mud"),
        MudBricks => Some("mud_bricks"),
        MossBlock => Some("moss_block"),
        DriedKelpBlock => Some("dried_kelp_block"),
        BlueIce => Some("blue_ice"),
        PackedIce => Some("packed_ice"),
        PowderSnow => Some("powder_snow"),
        SprucePlanks => Some("spruce_planks"),
        BirchPlanks => Some("birch_planks"),
        JunglePlanks => Some("jungle_planks"),
        AcaciaPlanks => Some("acacia_planks"),
        DarkOakPlanks => Some("dark_oak_planks"),
        CherryPlanks => Some("cherry_planks"),
        MangrovePlanks => Some("mangrove_planks"),
        BambooPlanks => Some("bamboo_planks"),
        BambooMosaic => Some("bamboo_mosaic"),
        SpruceLeaves => Some("spruce_leaves"),
        BirchLeaves => Some("birch_leaves"),
        JungleLeaves => Some("jungle_leaves"),
        AcaciaLeaves => Some("acacia_leaves"),
        DarkOakLeaves => Some("dark_oak_leaves"),
        CherryLeaves => Some("cherry_leaves"),
        MangroveLeaves => Some("mangrove_leaves"),
        AzaleaLeaves => Some("azalea_leaves"),
        FloweringAzaleaLeaves => Some("flowering_azalea_leaves"),
        SpruceLog => Some("spruce_log"),
        BirchLog => Some("birch_log"),
        JungleLog => Some("jungle_log"),
        AcaciaLog => Some("acacia_log"),
        DarkOakLog => Some("dark_oak_log"),
        CherryLog => Some("cherry_log"),
        MangroveLog => Some("mangrove_log"),
        StrippedOakLog => Some("stripped_oak_log"),
        StrippedSpruceLog => Some("stripped_spruce_log"),
        StrippedBirchLog => Some("stripped_birch_log"),
        StrippedJungleLog => Some("stripped_jungle_log"),
        StrippedAcaciaLog => Some("stripped_acacia_log"),
        StrippedDarkOakLog => Some("stripped_dark_oak_log"),
        StrippedCherryLog => Some("stripped_cherry_log"),
        StrippedMangroveLog => Some("stripped_mangrove_log"),
        OakWood => Some("oak_wood"),
        SpruceWood => Some("spruce_wood"),
        BirchWood => Some("birch_wood"),
        JungleWood => Some("jungle_wood"),
        AcaciaWood => Some("acacia_wood"),
        DarkOakWood => Some("dark_oak_wood"),
        CherryWood => Some("cherry_wood"),
        MangroveWood => Some("mangrove_wood"),
        StrippedOakWood => Some("stripped_oak_wood"),
        StrippedSpruceWood => Some("stripped_spruce_wood"),
        StrippedBirchWood => Some("stripped_birch_wood"),
        StrippedJungleWood => Some("stripped_jungle_wood"),
        StrippedAcaciaWood => Some("stripped_acacia_wood"),
        StrippedDarkOakWood => Some("stripped_dark_oak_wood"),
        StrippedCherryWood => Some("stripped_cherry_wood"),
        StrippedMangroveWood => Some("stripped_mangrove_wood"),
        DeepslateIronOre => Some("deepslate_iron_ore"),
        DeepslateCoalOre => Some("deepslate_coal_ore"),
        DeepslateGoldOre => Some("deepslate_gold_ore"),
        DeepslateRedstoneOre => Some("deepslate_redstone_ore"),
        DeepslateDiamondOre => Some("deepslate_diamond_ore"),
        DeepslateEmeraldOre => Some("deepslate_emerald_ore"),
        DeepslateLapisOre => Some("deepslate_lapis_ore"),
        DeepslateCopperOre => Some("deepslate_copper_ore"),
        _ => None,
    }
}

fn face_textures_for_block(reader: &AssetReader, id: BlockId) -> Option<[String; 6]> {
    let name = minecraft_name(id)?;

    let model_name = resolve_blockstate_model(reader, name);

    let resolved = if let Some(ref mn) = model_name {
        resolve_face_textures(reader, mn)
    } else {
        None
    };

    if let Some(tex_map) = resolved {
        let get = |key: &str| -> String {
            tex_map.get(key).cloned().unwrap_or_else(|| name.to_string())
        };
        Some([
            get("up"), get("down"), get("west"),
            get("east"), get("south"), get("north"),
        ])
    } else if name == "water_still" || name == "lava_still" {
        Some(core::array::from_fn(|_| name.to_string()))
    } else if name == "torch" || name == "soul_torch" || name == "redstone_torch" {
        Some(core::array::from_fn(|_| name.to_string()))
    } else if name == "fire_0" || name == "soul_fire_0" {
        Some(core::array::from_fn(|_| name.to_string()))
    } else {
        if reader.exists(&format!("textures/block/{name}.png")) {
            Some(core::array::from_fn(|_| name.to_string()))
        } else {
            None
        }
    }
}

fn collect_all_block_ids() -> Vec<(BlockId, &'static str)> {
    use BlockId::*;
    let mut all = Vec::new();
    macro_rules! push {
        ($id:ident, $name:expr) => { all.push(($id, $name)); };
    }
    // Primary blocks
    push!(Stone, "stone");
    push!(GrassBlock, "grass_block");
    push!(Dirt, "dirt");
    push!(Cobblestone, "cobblestone");
    push!(OakPlanks, "oak_planks");
    push!(Bedrock, "bedrock");
    push!(Water, "water_still");
    push!(Sand, "sand");
    push!(Gravel, "gravel");
    push!(GoldOre, "gold_ore");
    push!(IronOre, "iron_ore");
    push!(CoalOre, "coal_ore");
    push!(OakLog, "oak_log");
    push!(OakLeaves, "oak_leaves");
    push!(Glass, "glass");
    push!(CraftingTable, "crafting_table");
    push!(Furnace, "furnace");
    push!(Chest, "chest");
    push!(OakDoor, "oak_door");
    push!(OakFence, "oak_fence");
    push!(RedstoneDust, "redstone_dust_dot");
    push!(Snow, "snow");
    push!(Ice, "ice");
    push!(Glowstone, "glowstone");
    push!(Netherrack, "netherrack");
    push!(SoulSand, "soul_sand");
    push!(Deepslate, "deepslate");
    push!(SnowBlock, "snow_block");
    push!(CoarseDirt, "coarse_dirt");
    push!(Podzol, "podzol");
    push!(DiamondBlock, "diamond_block");
    push!(IronBlock, "iron_block");
    push!(GoldBlock, "gold_block");
    push!(EmeraldBlock, "emerald_block");
    push!(LapisBlock, "lapis_block");
    push!(RedstoneBlock, "redstone_block");
    push!(Bricks, "bricks");
    push!(Bookshelf, "bookshelf");
    push!(MossyCobblestone, "mossy_cobblestone");
    push!(Obsidian, "obsidian");
    push!(Sandstone, "sandstone");
    push!(StoneBricks, "stone_bricks");
    push!(Granite, "granite");
    push!(Diorite, "diorite");
    push!(Andesite, "andesite");
    push!(Calcite, "calcite");
    push!(Tuff, "tuff");
    push!(DripstoneBlock, "dripstone_block");
    push!(CobbledDeepslate, "cobbled_deepslate");
    push!(PolishedDeepslate, "polished_deepslate");
    push!(DeepslateBricks, "deepslate_bricks");
    push!(DeepslateTiles, "deepslate_tiles");
    push!(Blackstone, "blackstone");
    push!(PolishedBlackstone, "polished_blackstone");
    push!(PolishedBlackstoneBricks, "polished_blackstone_bricks");
    push!(CrimsonNylium, "crimson_nylium");
    push!(WarpedNylium, "warped_nylium");
    push!(RedSand, "red_sand");
    push!(Sponge, "sponge");
    push!(WetSponge, "wet_sponge");
    push!(LapisOre, "lapis_ore");
    push!(RedstoneOre, "redstone_ore");
    push!(EmeraldOre, "emerald_ore");
    push!(DiamondOre, "diamond_ore");
    push!(Lava, "lava_still");
    push!(Pumpkin, "pumpkin");
    push!(CarvedPumpkin, "carved_pumpkin");
    push!(JackOLantern, "jack_o_lantern");
    push!(Melon, "melon");
    push!(HayBlock, "hay_block");
    push!(SeaLantern, "sea_lantern");
    push!(Shroomlight, "shroomlight");
    push!(CryingObsidian, "crying_obsidian");
    push!(NetheriteBlock, "netherite_block");
    push!(AncientDebris, "ancient_debris");
    push!(SoulSoil, "soul_soil");
    push!(Basalt, "basalt");
    push!(PolishedBasalt, "polished_basalt");
    push!(SmoothStone, "smooth_stone");
    push!(SmoothSandstone, "smooth_sandstone");
    push!(SmoothRedSandstone, "smooth_red_sandstone");
    push!(SmoothQuartz, "smooth_quartz");
    push!(QuartzBlock, "quartz_block");
    push!(ChiseledQuartzBlock, "chiseled_quartz_block");
    push!(QuartzPillar, "quartz_pillar");
    push!(Prismarine, "prismarine");
    push!(PrismarineBricks, "prismarine_bricks");
    push!(DarkPrismarine, "dark_prismarine");
    push!(PurpurBlock, "purpur_block");
    push!(PurpurPillar, "purpur_pillar");
    push!(EndStone, "end_stone");
    push!(EndStoneBricks, "end_stone_bricks");
    push!(BoneBlock, "bone_block");
    push!(NetherBricks, "nether_bricks");
    push!(RedNetherBricks, "red_nether_bricks");
    push!(MudBricks, "mud_bricks");
    push!(MossBlock, "moss_block");
    push!(Terracotta, "terracotta");
    push!(CopperBlock, "copper_block");
    push!(CutCopper, "cut_copper");
    push!(RawCopperBlock, "raw_copper_block");
    push!(RawIronBlock, "raw_iron_block");
    push!(RawGoldBlock, "raw_gold_block");
    push!(Mud, "mud");
    push!(PackedMud, "packed_mud");
    push!(DriedKelpBlock, "dried_kelp_block");
    push!(BlueIce, "blue_ice");
    push!(PackedIce, "packed_ice");
    push!(MangroveRoots, "mangrove_roots");
    push!(MuddyMangroveRoots, "muddy_mangrove_roots");
    push!(Target, "target");
    push!(Lodestone, "lodestone");
    push!(HoneyBlock, "honey_block");
    push!(HoneycombBlock, "honeycomb_block");
    push!(RespawnAnchor, "respawn_anchor");
    push!(NetherGoldOre, "nether_gold_ore");
    // Logs
    push!(OakLog2, "oak_log");
    push!(SpruceLog, "spruce_log");
    push!(BirchLog, "birch_log");
    push!(JungleLog, "jungle_log");
    push!(AcaciaLog, "acacia_log");
    push!(DarkOakLog, "dark_oak_log");
    push!(CherryLog, "cherry_log");
    push!(MangroveLog, "mangrove_log");
    push!(StrippedOakLog, "stripped_oak_log");
    push!(StrippedSpruceLog, "stripped_spruce_log");
    push!(StrippedBirchLog, "stripped_birch_log");
    push!(StrippedJungleLog, "stripped_jungle_log");
    push!(StrippedAcaciaLog, "stripped_acacia_log");
    push!(StrippedDarkOakLog, "stripped_dark_oak_log");
    push!(StrippedCherryLog, "stripped_cherry_log");
    push!(StrippedMangroveLog, "stripped_mangrove_log");
    // Woods (bark blocks, all faces = log side)
    push!(OakWood, "oak_log");
    push!(SpruceWood, "spruce_log");
    push!(BirchWood, "birch_log");
    push!(JungleWood, "jungle_log");
    push!(AcaciaWood, "acacia_log");
    push!(DarkOakWood, "dark_oak_log");
    push!(CherryWood, "cherry_log");
    push!(MangroveWood, "mangrove_log");
    push!(StrippedOakWood, "stripped_oak_log");
    push!(StrippedSpruceWood, "stripped_spruce_log");
    push!(StrippedBirchWood, "stripped_birch_log");
    push!(StrippedJungleWood, "stripped_jungle_log");
    push!(StrippedAcaciaWood, "stripped_acacia_log");
    push!(StrippedDarkOakWood, "stripped_dark_oak_log");
    push!(StrippedCherryWood, "stripped_cherry_log");
    push!(StrippedMangroveWood, "stripped_mangrove_log");
    // Ores
    push!(DeepslateIronOre, "deepslate_iron_ore");
    push!(DeepslateCoalOre, "deepslate_coal_ore");
    push!(DeepslateGoldOre, "deepslate_gold_ore");
    push!(DeepslateRedstoneOre, "deepslate_redstone_ore");
    push!(DeepslateDiamondOre, "deepslate_diamond_ore");
    push!(DeepslateEmeraldOre, "deepslate_emerald_ore");
    push!(DeepslateLapisOre, "deepslate_lapis_ore");
    push!(DeepslateCopperOre, "deepslate_copper_ore");
    push!(CopperOre, "copper_ore");
    // Leaves
    push!(OakLeaves2, "oak_leaves");
    push!(SpruceLeaves, "spruce_leaves");
    push!(BirchLeaves, "birch_leaves");
    push!(JungleLeaves, "jungle_leaves");
    push!(AcaciaLeaves, "acacia_leaves");
    push!(DarkOakLeaves, "dark_oak_leaves");
    push!(CherryLeaves, "cherry_leaves");
    push!(MangroveLeaves, "mangrove_leaves");
    push!(AzaleaLeaves, "azalea_leaves");
    push!(FloweringAzaleaLeaves, "flowering_azalea_leaves");
    // Planks
    push!(OakPlanks2, "oak_planks");
    push!(SprucePlanks, "spruce_planks");
    push!(BirchPlanks, "birch_planks");
    push!(JunglePlanks, "jungle_planks");
    push!(AcaciaPlanks, "acacia_planks");
    push!(DarkOakPlanks, "dark_oak_planks");
    push!(CherryPlanks, "cherry_planks");
    push!(MangrovePlanks, "mangrove_planks");
    push!(BambooPlanks, "bamboo_planks");
    push!(BambooMosaic, "bamboo_mosaic");
    // Snow
    push!(Snow2, "snow");
    push!(PowderSnow, "powder_snow");
    // Stained glass
    push!(WhiteStainedGlass, "white_stained_glass");
    push!(OrangeStainedGlass, "orange_stained_glass");
    push!(MagentaStainedGlass, "magenta_stained_glass");
    push!(LightBlueStainedGlass, "light_blue_stained_glass");
    push!(YellowStainedGlass, "yellow_stained_glass");
    push!(LimeStainedGlass, "lime_stained_glass");
    push!(PinkStainedGlass, "pink_stained_glass");
    push!(GrayStainedGlass, "gray_stained_glass");
    push!(LightGrayStainedGlass, "light_gray_stained_glass");
    push!(CyanStainedGlass, "cyan_stained_glass");
    push!(PurpleStainedGlass, "purple_stained_glass");
    push!(BlueStainedGlass, "blue_stained_glass");
    push!(BrownStainedGlass, "brown_stained_glass");
    push!(GreenStainedGlass, "green_stained_glass");
    push!(RedStainedGlass, "red_stained_glass");
    push!(BlackStainedGlass, "black_stained_glass");
    push!(TintedGlass, "tinted_glass");
    // Wool
    push!(WhiteWool, "white_wool");
    push!(OrangeWool, "orange_wool");
    push!(MagentaWool, "magenta_wool");
    push!(LightBlueWool, "light_blue_wool");
    push!(YellowWool, "yellow_wool");
    push!(LimeWool, "lime_wool");
    push!(PinkWool, "pink_wool");
    push!(GrayWool, "gray_wool");
    push!(LightGrayWool, "light_gray_wool");
    push!(CyanWool, "cyan_wool");
    push!(PurpleWool, "purple_wool");
    push!(BlueWool, "blue_wool");
    push!(BrownWool, "brown_wool");
    push!(GreenWool, "green_wool");
    push!(RedWool, "red_wool");
    push!(BlackWool, "black_wool");
    // Terracotta
    push!(WhiteTerracotta, "white_terracotta");
    push!(OrangeTerracotta, "orange_terracotta");
    push!(MagentaTerracotta, "magenta_terracotta");
    push!(LightBlueTerracotta, "light_blue_terracotta");
    push!(YellowTerracotta, "yellow_terracotta");
    push!(LimeTerracotta, "lime_terracotta");
    push!(PinkTerracotta, "pink_terracotta");
    push!(GrayTerracotta, "gray_terracotta");
    push!(LightGrayTerracotta, "light_gray_terracotta");
    push!(CyanTerracotta, "cyan_terracotta");
    push!(PurpleTerracotta, "purple_terracotta");
    push!(BlueTerracotta, "blue_terracotta");
    push!(BrownTerracotta, "brown_terracotta");
    push!(GreenTerracotta, "green_terracotta");
    push!(RedTerracotta, "red_terracotta");
    push!(BlackTerracotta, "black_terracotta");
    // Concrete
    push!(WhiteConcrete, "white_concrete");
    push!(OrangeConcrete, "orange_concrete");
    push!(MagentaConcrete, "magenta_concrete");
    push!(LightBlueConcrete, "light_blue_concrete");
    push!(YellowConcrete, "yellow_concrete");
    push!(LimeConcrete, "lime_concrete");
    push!(PinkConcrete, "pink_concrete");
    push!(GrayConcrete, "gray_concrete");
    push!(LightGrayConcrete, "light_gray_concrete");
    push!(CyanConcrete, "cyan_concrete");
    push!(PurpleConcrete, "purple_concrete");
    push!(BlueConcrete, "blue_concrete");
    push!(BrownConcrete, "brown_concrete");
    push!(GreenConcrete, "green_concrete");
    push!(RedConcrete, "red_concrete");
    push!(BlackConcrete, "black_concrete");
    // Copper variants
    push!(ExposedCopper, "exposed_copper");
    push!(WeatheredCopper, "weathered_copper");
    push!(OxidizedCopper, "oxidized_copper");
    push!(ExposedCutCopper, "exposed_cut_copper");
    push!(WeatheredCutCopper, "weathered_cut_copper");
    push!(OxidizedCutCopper, "oxidized_cut_copper");
    // Slabs
    push!(StoneSlab, "stone");
    push!(OakSlab, "oak_planks");
    // Stairs
    push!(StoneStairs, "cobblestone");
    push!(OakStairs, "oak_planks");
    // Fungus blocks
    push!(Mycelium, "mycelium_top");
    // Mushroom blocks
    push!(MushroomStem, "mushroom_stem");
    push!(RedMushroomBlock, "red_mushroom_block");
    push!(BrownMushroomBlock, "brown_mushroom_block");
    // Shulker boxes
    push!(ShulkerBox, "shulker_box");
    push!(WhiteShulkerBox, "white_shulker_box");
    push!(OrangeShulkerBox, "orange_shulker_box");
    push!(MagentaShulkerBox, "magenta_shulker_box");
    push!(LightBlueShulkerBox, "light_blue_shulker_box");
    push!(YellowShulkerBox, "yellow_shulker_box");
    push!(LimeShulkerBox, "lime_shulker_box");
    push!(PinkShulkerBox, "pink_shulker_box");
    push!(GrayShulkerBox, "gray_shulker_box");
    push!(LightGrayShulkerBox, "light_gray_shulker_box");
    push!(CyanShulkerBox, "cyan_shulker_box");
    push!(PurpleShulkerBox, "purple_shulker_box");
    push!(BlueShulkerBox, "blue_shulker_box");
    push!(BrownShulkerBox, "brown_shulker_box");
    push!(GreenShulkerBox, "green_shulker_box");
    push!(RedShulkerBox, "red_shulker_box");
    push!(BlackShulkerBox, "black_shulker_box");
    all
}

pub fn build_texture_mapping(reader: &AssetReader) -> BlockTextureMapping {
    let mut entries = Vec::new();
    let all_blocks = collect_all_block_ids();

    for &(block_id, name) in &all_blocks {
        let faces = face_textures_for_block(reader, block_id)
            .unwrap_or_else(|| core::array::from_fn(|_| name.to_string()));
        entries.push((block_id, faces));
    }

    entries.sort_by_key(|e| e.0 as u32);
    BlockTextureMapping {
        reader: reader.clone(),
        entries,
    }
}

impl BlockTextureMapping {
    pub fn build_tile_list(&self, extra_textures: &[String]) -> Vec<TextureTile> {
        let mut seen = std::collections::HashSet::new();
        let mut tiles = Vec::new();

        for (_, faces) in &self.entries {
            for tex_name in faces {
                if seen.insert(tex_name.clone()) {
                    tiles.push(TextureTile {
                        path: tex_name.clone(),
                        tile_index: tiles.len() as u32,
                    });
                }
            }
        }

        // Ensure crossed-block textures are loaded even if not referenced by any face entry
        let crossed_textures = [
            "dandelion", "poppy", "blue_orchid", "allium", "azure_bluet",
            "red_tulip", "orange_tulip", "white_tulip", "pink_tulip",
            "oxeye_daisy", "cornflower", "lily_of_the_valley", "wither_rose",
            "short_grass", "fern", "dead_bush",
            "brown_mushroom", "red_mushroom",
            "crimson_fungus", "warped_fungus", "crimson_roots", "warped_roots", "nether_sprouts",
            "oak_sapling", "spruce_sapling", "birch_sapling", "jungle_sapling",
            "acacia_sapling", "dark_oak_sapling", "cherry_sapling",
            "cactus_side", "sugar_cane", "lily_pad",
            "vine", "weeping_vines", "twisting_vines", "glow_lichen",
            "spore_blossom", "azalea_plant", "flowering_azalea_side",
        ];
        for &tex in &crossed_textures {
            if seen.insert(tex.to_string()) {
                tiles.push(TextureTile {
                    path: tex.to_string(),
                    tile_index: tiles.len() as u32,
                });
            }
        }

        for tex in extra_textures {
            if seen.insert(tex.clone()) {
                tiles.push(TextureTile {
                    path: tex.clone(),
                    tile_index: tiles.len() as u32,
                });
            }
        }

        tiles
    }

    pub fn resolve_face_tiles(&self, tiles: &[TextureTile]) -> HashMap<(BlockId, BlockFace), u32> {
        let mut map: HashMap<String, u32> = HashMap::new();
        for tile in tiles {
            map.insert(tile.path.clone(), tile.tile_index);
        }

        let mut result = HashMap::new();

        for (block_id, faces) in &self.entries {
            for (fi, face) in FACES.iter().enumerate() {
                if let Some(&tile_idx) = map.get(&faces[fi]) {
                    result.insert((*block_id, *face), tile_idx);
                }
            }
        }
        result
    }

    /// Compute grass/foliage biome tint from temperature and humidity using a
    /// simplified approximation of the vanilla Minecraft formula.
    /// Falls back to the default tint if temp/humidity are not available.
    pub fn compute_biome_tint(temp: f32, humidity: f32) -> [u8; 3] {
        use std::f32::consts::PI;
        let r = (85.0 + 20.0 * (temp * PI).cos()).round().clamp(0.0, 255.0) as u8;
        let g = (140.0 + 30.0 * (humidity * PI / 2.0).sin()).round().clamp(0.0, 255.0) as u8;
        let b = (40.0 + 20.0 * (temp * PI).cos()).round().clamp(0.0, 255.0) as u8;
        [r, g, b]
    }

    fn apply_tint(pixels: &mut Vec<u8>, tint_r: f32, tint_g: f32, tint_b: f32) {
        for i in (0..pixels.len()).step_by(4) {
            let r = pixels[i] as f32;
            let g = pixels[i + 1] as f32;
            let b = pixels[i + 2] as f32;
            pixels[i] = (r * tint_r).min(255.0) as u8;
            pixels[i + 1] = (g * tint_g).min(255.0) as u8;
            pixels[i + 2] = (b * tint_b).min(255.0) as u8;
        }
    }

    pub fn load_all_pngs(&self, tiles: &[TextureTile]) -> Vec<Vec<u8>> {
        let mut images = Vec::with_capacity(tiles.len());

        for tile in tiles {
            let png_path = format!("textures/block/{}.png", tile.path);
            let mut pixels = if self.reader.exists(&png_path) {
                match self.reader.read_image(&png_path) {
                    Some(img) => {
                        let rgba = img.into_rgba8();
                        let (w, h) = rgba.dimensions();
                        if w == 16 && h == 16 {
                            rgba.into_raw()
                        } else if w == 16 && h >= 16 && h % 16 == 0 {
                            image::imageops::crop_imm(&rgba, 0, 0, 16, 16).to_image().into_raw()
                        } else {
                            let resized = image::imageops::resize(&rgba, 16, 16, image::imageops::FilterType::Nearest);
                            resized.into_raw()
                        }
                    }
                    None => {
                        log::warn!("failed to decode texture {png_path}; using diagnostic fallback");
                        generate_fallback(&tile.path)
                    }
                }
            } else {
                log::warn!("missing texture {png_path}; using diagnostic fallback");
                generate_fallback(&tile.path)
            };

            // Apply biome-style tints to greyscale Minecraft textures.
            // These static tints approximate the default plains biome.
            // For per-vertex biome-colored tinting, call compute_biome_tint()
            // at runtime with temperature and humidity data.
            match tile.path.as_str() {
                "grass_block_top" => Self::apply_tint(&mut pixels, 0.36, 0.60, 0.18),
                "fern" | "short_grass" | "tall_grass_top" | "tall_grass_bottom"
                | "large_fern_top" | "large_fern_bottom" => Self::apply_tint(&mut pixels, 0.36, 0.60, 0.18),
                "vine" | "glow_lichen" => Self::apply_tint(&mut pixels, 0.36, 0.60, 0.18),
                "lily_pad" => Self::apply_tint(&mut pixels, 0.24, 0.52, 0.16),
                "water_still" | "water_flow" => Self::apply_tint(&mut pixels, 0.20, 0.42, 0.82),
                n if n.ends_with("leaves") => Self::apply_tint(&mut pixels, 0.27, 0.54, 0.16),
                _ => {}
            }

            images.push(pixels);
        }
        images
    }
}

impl BlockTextureMapping {
    pub fn resolve_crossed_tiles(&self, tiles: &[TextureTile]) -> HashMap<BlockId, u32> {
        use BlockId::*;
        let mut map: HashMap<String, u32> = HashMap::new();
        for tile in tiles {
            map.insert(tile.path.clone(), tile.tile_index);
        }

        let mut result = HashMap::new();
        let crossed: &[(BlockId, &str)] = &[
            (Dandelion, "dandelion"),
            (Poppy, "poppy"),
            (BlueOrchid, "blue_orchid"),
            (Allium, "allium"),
            (AzureBluet, "azure_bluet"),
            (RedTulip, "red_tulip"),
            (OrangeTulip, "orange_tulip"),
            (WhiteTulip, "white_tulip"),
            (PinkTulip, "pink_tulip"),
            (OxeyeDaisy, "oxeye_daisy"),
            (Cornflower, "cornflower"),
            (LilyOfTheValley, "lily_of_the_valley"),
            (WitherRose, "wither_rose"),
            (Grass, "short_grass"),
            (Fern, "fern"),
            (DeadBush, "dead_bush"),
            (BrownMushroom, "brown_mushroom"),
            (RedMushroom, "red_mushroom"),
            (OakSapling, "oak_sapling"),
            (SpruceSapling, "spruce_sapling"),
            (BirchSapling, "birch_sapling"),
            (JungleSapling, "jungle_sapling"),
            (AcaciaSapling, "acacia_sapling"),
            (DarkOakSapling, "dark_oak_sapling"),
            (CherrySapling, "cherry_sapling"),
            (Cactus, "cactus_side"),
            (SugarCane, "sugar_cane"),
            (LilyPad, "lily_pad"),
            (Vine, "vine"),
            (WeepingVines, "weeping_vines"),
            (TwistingVines, "twisting_vines"),
            (CrimsonFungus, "crimson_fungus"),
            (WarpedFungus, "warped_fungus"),
            (CrimsonRoots, "crimson_roots"),
            (WarpedRoots, "warped_roots"),
            (NetherSprouts, "nether_sprouts"),
            (GlowLichen, "glow_lichen"),
            (SporeBlossom, "spore_blossom"),
            (Azalea, "azalea_plant"),
            (FloweringAzalea, "flowering_azalea_side"),
        ];
        for &(id, tex) in crossed {
            if let Some(&tile) = map.get(tex) {
                result.insert(id, tile);
            }
        }
        result
    }
}

fn generate_fallback(name: &str) -> Vec<u8> {
    let mut pixels = vec![255u8; 16 * 16 * 4];
    let hash: u32 = name.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let r = (hash & 0xFF) as u8;
    let g = ((hash >> 8) & 0xFF) as u8;
    let b = ((hash >> 16) & 0xFF) as u8;

    for py in 0..16u32 {
        for px in 0..16u32 {
            let i = ((py * 16 + px) * 4) as usize;
            let noise = ((px.wrapping_mul(7) + py.wrapping_mul(13) + hash.wrapping_mul(11)) % 30) as u8;
            pixels[i] = r.saturating_sub(noise);
            pixels[i + 1] = g.saturating_sub(noise / 2);
            pixels[i + 2] = b.saturating_sub(noise / 2);
            pixels[i + 3] = 255;
        }
    }
    pixels
}
