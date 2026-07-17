//! Bounded reader and chunk-local projection for Java structure templates.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use thiserror::Error;

use crate::world::block::{Block, BlockId};
use crate::world::block_registry::registry;
use crate::world::chunk::CHUNK_SIZE;

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_BYTE_ARRAY: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;
const TAG_INT_ARRAY: u8 = 11;
const TAG_LONG_ARRAY: u8 = 12;

/// Resource limits applied before and during template decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateLimits {
    pub max_compressed_bytes: usize,
    pub max_decompressed_bytes: usize,
    pub max_depth: usize,
    pub max_tags: usize,
    pub max_collection_len: usize,
    pub max_string_bytes: usize,
    pub max_dimension: i32,
    pub max_volume: u64,
    pub max_palette_entries: usize,
    pub max_blocks: usize,
}

impl Default for TemplateLimits {
    fn default() -> Self {
        Self {
            max_compressed_bytes: 8 * 1024 * 1024,
            max_decompressed_bytes: 64 * 1024 * 1024,
            max_depth: 32,
            max_tags: 8 * 1024 * 1024,
            max_collection_len: 4 * 1024 * 1024,
            max_string_bytes: 32 * 1024,
            max_dimension: 512,
            max_volume: 64 * 1024 * 1024,
            max_palette_entries: 65_536,
            max_blocks: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("template path must be a non-empty relative path without parent components: {0}")]
    InvalidAssetPath(PathBuf),
    #[error("failed to read structure template {path}")]
    AssetIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("compressed template is {actual} bytes, exceeding the {limit}-byte limit")]
    CompressedInputTooLarge { actual: usize, limit: usize },
    #[error("decompressed template exceeds the {limit}-byte limit")]
    DecompressedInputTooLarge { limit: usize },
    #[error("invalid or truncated gzip stream")]
    Gzip(#[source] io::Error),
    #[error(transparent)]
    Nbt(#[from] NbtError),
    #[error("missing required template field {0}")]
    MissingField(&'static str),
    #[error("duplicate template field {0}")]
    DuplicateField(String),
    #[error("field {field} has NBT tag {actual}, expected {expected}")]
    WrongTag {
        field: String,
        expected: &'static str,
        actual: u8,
    },
    #[error("invalid template size {size:?}: each dimension must be in 1..={limit}")]
    InvalidSize { size: [i32; 3], limit: i32 },
    #[error("template volume exceeds the {limit}-block limit")]
    TemplateVolumeTooLarge { limit: u64 },
    #[error("palette has {actual} entries, exceeding the {limit}-entry limit")]
    PaletteTooLarge { actual: usize, limit: usize },
    #[error("palette {palette_index} has {actual} entries, expected {expected}")]
    InconsistentPaletteLength {
        palette_index: usize,
        actual: usize,
        expected: usize,
    },
    #[error("template has {actual} blocks, exceeding the {limit}-block limit")]
    BlockCountTooLarge { actual: usize, limit: usize },
    #[error("palette entry {index} has an empty Name")]
    EmptyPaletteName { index: usize },
    #[error("block {block_index} position {position:?} lies outside template size {size:?}")]
    PositionOutsideTemplate {
        block_index: usize,
        position: [i32; 3],
        size: [i32; 3],
    },
    #[error("block {block_index} references palette index {palette_index}, but the palette has {palette_len} entries")]
    InvalidPaletteIndex {
        block_index: usize,
        palette_index: i32,
        palette_len: usize,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NbtError {
    #[error("truncated NBT at byte {offset}; needed {needed} more bytes")]
    UnexpectedEof { offset: usize, needed: usize },
    #[error("invalid NBT tag {tag} at byte {offset}")]
    InvalidTag { tag: u8, offset: usize },
    #[error("negative NBT collection length {length} at byte {offset}")]
    NegativeLength { length: i32, offset: usize },
    #[error("NBT collection length {length} exceeds the {limit}-element limit")]
    CollectionTooLarge { length: usize, limit: usize },
    #[error("NBT string length {length} exceeds the {limit}-byte limit")]
    StringTooLarge { length: usize, limit: usize },
    #[error("NBT string at byte {offset} is not valid UTF-8")]
    InvalidUtf8 { offset: usize },
    #[error("NBT depth {depth} exceeds the configured limit of {limit}")]
    DepthLimit { depth: usize, limit: usize },
    #[error("NBT tag count exceeds the configured limit of {limit}")]
    TagLimit { limit: usize },
    #[error("root NBT tag is {actual}, expected a compound")]
    InvalidRoot { actual: u8 },
    #[error("trailing data begins at NBT byte {offset}")]
    TrailingData { offset: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePaletteEntry {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}

impl TemplatePaletteEntry {
    /// Maps exact Java names only. Unknown names remain visible to the caller.
    pub fn native_block_id(&self) -> Option<BlockId> {
        native_block_id(&self.name)
    }

    pub fn flatten(&self, transform: TemplateTransform) -> Result<FlattenedTemplateBlock, UnsupportedTemplateState> {
        if matches!(self.name.as_str(), "minecraft:structure_void" | "minecraft:jigsaw") {
            return Ok(FlattenedTemplateBlock::NoOp);
        }
        let id = self
            .native_block_id()
            .ok_or_else(|| UnsupportedTemplateState::Block(self.name.clone()))?;
        if self.properties.is_empty() {
            return Ok(FlattenedTemplateBlock::Place(Block::new(id)));
        }

        if matches!(id, BlockId::Water | BlockId::Lava)
            && self.properties.keys().all(|name| name == "level")
        {
            let level = self
                .properties
                .get("level")
                .and_then(|value| value.parse::<u8>().ok())
                .filter(|level| *level <= 15)
                .ok_or_else(|| UnsupportedTemplateState::State {
                    name: self.name.clone(),
                    properties: self.properties.clone(),
                })?;
            return Ok(FlattenedTemplateBlock::Place(Block::with_legacy_data(id, level)));
        }

        let properties = transformed_properties(&self.properties, transform);
        let definition = registry().definition(id);
        if !definition.properties.is_empty() {
            let state = registry()
                .state_for_properties(
                    id,
                    properties
                        .iter()
                        .map(|(name, value)| (name.as_str(), value.as_str())),
                )
                .ok_or_else(|| UnsupportedTemplateState::State {
                    name: self.name.clone(),
                    properties: self.properties.clone(),
                })?;
            let mut block = Block::new(id);
            block.state = state;
            block.data = legacy_data_for_state(id, &properties);
            return Ok(FlattenedTemplateBlock::Place(block));
        }

        if property_insensitive_full_cube(id, &properties) {
            return Ok(FlattenedTemplateBlock::Place(Block::new(id)));
        }
        Err(UnsupportedTemplateState::State {
            name: self.name.clone(),
            properties: self.properties.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlattenedTemplateBlock {
    Place(Block),
    NoOp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedTemplateState {
    Block(String),
    State {
        name: String,
        properties: BTreeMap<String, String>,
    },
}

impl fmt::Display for UnsupportedTemplateState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block(name) => write!(formatter, "unsupported block {name}"),
            Self::State { name, properties } => {
                write!(formatter, "unsupported state {name}{properties:?}")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateBlock {
    pub position: [i32; 3],
    pub palette_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureTemplate {
    pub size: [i32; 3],
    pub palettes: Vec<Vec<TemplatePaletteEntry>>,
    pub blocks: Vec<TemplateBlock>,
}

impl StructureTemplate {
    pub fn palette(&self, selected_palette: usize) -> Option<&[TemplatePaletteEntry]> {
        self.palettes.get(selected_palette).map(Vec::as_slice)
    }

    pub fn load_from_asset_root(
        asset_root: impl AsRef<Path>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, TemplateError> {
        Self::load_from_asset_root_with_limits(asset_root, relative_path, TemplateLimits::default())
    }

    pub fn load_from_asset_root_with_limits(
        asset_root: impl AsRef<Path>,
        relative_path: impl AsRef<Path>,
        limits: TemplateLimits,
    ) -> Result<Self, TemplateError> {
        let relative_path = relative_path.as_ref();
        if relative_path.as_os_str().is_empty()
            || relative_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(TemplateError::InvalidAssetPath(relative_path.to_path_buf()));
        }

        let path = asset_root.as_ref().join(relative_path);
        let metadata = fs::metadata(&path).map_err(|source| TemplateError::AssetIo {
            path: path.clone(),
            source,
        })?;
        let compressed_len = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if compressed_len > limits.max_compressed_bytes {
            return Err(TemplateError::CompressedInputTooLarge {
                actual: compressed_len,
                limit: limits.max_compressed_bytes,
            });
        }
        let bytes = fs::read(&path).map_err(|source| TemplateError::AssetIo {
            path: path.clone(),
            source,
        })?;
        Self::decode_gzip_with_limits(&bytes, limits)
    }

    pub fn decode_gzip(bytes: &[u8]) -> Result<Self, TemplateError> {
        Self::decode_gzip_with_limits(bytes, TemplateLimits::default())
    }

    pub fn decode_gzip_with_limits(
        bytes: &[u8],
        limits: TemplateLimits,
    ) -> Result<Self, TemplateError> {
        if bytes.len() > limits.max_compressed_bytes {
            return Err(TemplateError::CompressedInputTooLarge {
                actual: bytes.len(),
                limit: limits.max_compressed_bytes,
            });
        }

        let read_limit = limits.max_decompressed_bytes.saturating_add(1) as u64;
        let mut decoder = GzDecoder::new(bytes).take(read_limit);
        let mut decompressed = Vec::with_capacity(bytes.len().min(limits.max_decompressed_bytes));
        decoder
            .read_to_end(&mut decompressed)
            .map_err(TemplateError::Gzip)?;
        if decompressed.len() > limits.max_decompressed_bytes {
            return Err(TemplateError::DecompressedInputTooLarge {
                limit: limits.max_decompressed_bytes,
            });
        }

        let mut parser = NbtParser::new(&decompressed, limits);
        let template = parser.parse_template()?;
        validate_template(template, limits)
    }

    pub fn transformed_size(&self, transform: TemplateTransform) -> [i32; 3] {
        match transform.rotation {
            Rotation::Clockwise90 | Rotation::Counterclockwise90 => {
                [self.size[2], self.size[1], self.size[0]]
            }
            Rotation::None | Rotation::Clockwise180 => self.size,
        }
    }

    pub fn transform_position(&self, position: [i32; 3], transform: TemplateTransform) -> [i32; 3] {
        let [mut x, y, mut z] = position;
        if transform.mirror == Mirror::FrontBack {
            x = self.size[0] - 1 - x;
        }
        if transform.mirror == Mirror::LeftRight {
            z = self.size[2] - 1 - z;
        }
        match transform.rotation {
            Rotation::None => [x, y, z],
            Rotation::Clockwise90 => [self.size[2] - 1 - z, y, x],
            Rotation::Clockwise180 => [self.size[0] - 1 - x, y, self.size[2] - 1 - z],
            Rotation::Counterclockwise90 => [z, y, self.size[0] - 1 - x],
        }
    }

    /// Lazily projects only blocks whose transformed world X/Z lies in `target_chunk`.
    pub fn blocks_in_chunk(
        &self,
        origin: [i64; 3],
        transform: TemplateTransform,
        target_chunk: [i32; 2],
    ) -> TransformedChunkBlocks<'_> {
        let min_x = i64::from(target_chunk[0]) * CHUNK_SIZE as i64;
        let min_z = i64::from(target_chunk[1]) * CHUNK_SIZE as i64;
        TransformedChunkBlocks {
            template: self,
            palette: &self.palettes[0],
            blocks: self.blocks.iter(),
            origin,
            transform,
            min_x,
            max_x: min_x + CHUNK_SIZE as i64 - 1,
            min_z,
            max_z: min_z + CHUNK_SIZE as i64 - 1,
        }
    }

    pub fn blocks_in_chunk_with_palette(
        &self,
        origin: [i64; 3],
        transform: TemplateTransform,
        target_chunk: [i32; 2],
        selected_palette: usize,
    ) -> Option<TransformedChunkBlocks<'_>> {
        let palette = self.palettes.get(selected_palette)?;
        let min_x = i64::from(target_chunk[0]) * CHUNK_SIZE as i64;
        let min_z = i64::from(target_chunk[1]) * CHUNK_SIZE as i64;
        Some(TransformedChunkBlocks {
            template: self,
            palette,
            blocks: self.blocks.iter(),
            origin,
            transform,
            min_x,
            max_x: min_x + CHUNK_SIZE as i64 - 1,
            min_z,
            max_z: min_z + CHUNK_SIZE as i64 - 1,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TemplateTransform {
    pub mirror: Mirror,
    pub rotation: Rotation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mirror {
    #[default]
    None,
    /// Reflects local Z before rotation, matching Java's left-right mirror.
    LeftRight,
    /// Reflects local X before rotation, matching Java's front-back mirror.
    FrontBack,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformedTemplateBlock<'a> {
    pub world_position: [i64; 3],
    pub template_position: [i32; 3],
    pub palette_index: usize,
    pub block_id: BlockId,
    pub palette: &'a TemplatePaletteEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateProjectionError<'a> {
    UnsupportedPaletteName {
        palette_index: usize,
        name: &'a str,
    },
    CoordinateOverflow {
        position: [i32; 3],
        origin: [i64; 3],
    },
}

impl fmt::Display for TemplateProjectionError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPaletteName {
                palette_index,
                name,
            } => write!(
                formatter,
                "palette entry {palette_index} uses unsupported block {name}"
            ),
            Self::CoordinateOverflow { position, origin } => write!(
                formatter,
                "transformed position {position:?} overflows world origin {origin:?}"
            ),
        }
    }
}

impl std::error::Error for TemplateProjectionError<'_> {}

pub struct TransformedChunkBlocks<'a> {
    template: &'a StructureTemplate,
    palette: &'a [TemplatePaletteEntry],
    blocks: std::slice::Iter<'a, TemplateBlock>,
    origin: [i64; 3],
    transform: TemplateTransform,
    min_x: i64,
    max_x: i64,
    min_z: i64,
    max_z: i64,
}

impl<'a> Iterator for TransformedChunkBlocks<'a> {
    type Item = Result<TransformedTemplateBlock<'a>, TemplateProjectionError<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let block = self.blocks.next()?;
            let transformed = self
                .template
                .transform_position(block.position, self.transform);
            let Some(world_x) = self.origin[0].checked_add(i64::from(transformed[0])) else {
                return Some(Err(TemplateProjectionError::CoordinateOverflow {
                    position: transformed,
                    origin: self.origin,
                }));
            };
            let Some(world_y) = self.origin[1].checked_add(i64::from(transformed[1])) else {
                return Some(Err(TemplateProjectionError::CoordinateOverflow {
                    position: transformed,
                    origin: self.origin,
                }));
            };
            let Some(world_z) = self.origin[2].checked_add(i64::from(transformed[2])) else {
                return Some(Err(TemplateProjectionError::CoordinateOverflow {
                    position: transformed,
                    origin: self.origin,
                }));
            };
            if world_x < self.min_x
                || world_x > self.max_x
                || world_z < self.min_z
                || world_z > self.max_z
            {
                continue;
            }

            let palette = &self.palette[block.palette_index];
            let block_id = if matches!(palette.name.as_str(), "minecraft:structure_void" | "minecraft:jigsaw") {
                BlockId::Air
            } else if let Some(block_id) = palette.native_block_id() {
                block_id
            } else {
                return Some(Err(TemplateProjectionError::UnsupportedPaletteName {
                    palette_index: block.palette_index,
                    name: &palette.name,
                }));
            };
            return Some(Ok(TransformedTemplateBlock {
                world_position: [world_x, world_y, world_z],
                template_position: block.position,
                palette_index: block.palette_index,
                block_id,
                palette,
            }));
        }
    }
}

fn validate_template(
    mut template: ParsedTemplate,
    limits: TemplateLimits,
) -> Result<StructureTemplate, TemplateError> {
    let size = template.size.ok_or(TemplateError::MissingField("size"))?;
    if size
        .iter()
        .any(|dimension| *dimension <= 0 || *dimension > limits.max_dimension)
    {
        return Err(TemplateError::InvalidSize {
            size,
            limit: limits.max_dimension,
        });
    }
    let volume = size.iter().try_fold(1_u64, |volume, dimension| {
        volume.checked_mul(*dimension as u64)
    });
    if volume.is_none_or(|volume| volume > limits.max_volume) {
        return Err(TemplateError::TemplateVolumeTooLarge {
            limit: limits.max_volume,
        });
    }

    let palettes = template
        .palettes
        .take()
        .ok_or(TemplateError::MissingField("palette or palettes"))?;
    let palette_len = palettes[0].len();
    let parsed_blocks = template
        .blocks
        .take()
        .ok_or(TemplateError::MissingField("blocks"))?;
    let mut blocks = Vec::with_capacity(parsed_blocks.len());
    for (block_index, block) in parsed_blocks.into_iter().enumerate() {
        if block.palette_index < 0 || block.palette_index as usize >= palette_len {
            return Err(TemplateError::InvalidPaletteIndex {
                block_index,
                palette_index: block.palette_index,
                palette_len,
            });
        }
        if block
            .position
            .iter()
            .zip(size)
            .any(|(coordinate, dimension)| *coordinate < 0 || *coordinate >= dimension)
        {
            return Err(TemplateError::PositionOutsideTemplate {
                block_index,
                position: block.position,
                size,
            });
        }
        blocks.push(TemplateBlock {
            position: block.position,
            palette_index: block.palette_index as usize,
        });
    }

    Ok(StructureTemplate {
        size,
        palettes,
        blocks,
    })
}

#[derive(Default)]
struct ParsedTemplate {
    size: Option<[i32; 3]>,
    palettes: Option<Vec<Vec<TemplatePaletteEntry>>>,
    blocks: Option<Vec<ParsedBlock>>,
}

struct ParsedBlock {
    position: [i32; 3],
    palette_index: i32,
}

struct NbtParser<'a> {
    bytes: &'a [u8],
    offset: usize,
    tags: usize,
    limits: TemplateLimits,
}

impl<'a> NbtParser<'a> {
    fn new(bytes: &'a [u8], limits: TemplateLimits) -> Self {
        Self {
            bytes,
            offset: 0,
            tags: 0,
            limits,
        }
    }

    fn parse_template(&mut self) -> Result<ParsedTemplate, TemplateError> {
        let root_tag = self.read_u8()?;
        if root_tag != TAG_COMPOUND {
            return Err(NbtError::InvalidRoot { actual: root_tag }.into());
        }
        self.read_string()?;
        self.ensure_depth(1)?;

        let mut template = ParsedTemplate::default();
        loop {
            let tag = self.read_u8()?;
            if tag == TAG_END {
                break;
            }
            self.validate_tag(tag)?;
            self.claim_tags(1)?;
            let name = self.read_string()?;
            match name.as_str() {
                "size" => {
                    expect_tag(&name, tag, TAG_LIST, "list")?;
                    if template.size.is_some() {
                        return Err(TemplateError::DuplicateField(name));
                    }
                    template.size = Some(self.read_int_triplet("size", 2)?);
                }
                "palette" => {
                    expect_tag(&name, tag, TAG_LIST, "list")?;
                    if template.palettes.is_some() {
                        return Err(TemplateError::DuplicateField(name));
                    }
                    template.palettes = Some(vec![self.read_palette(2)?]);
                }
                "palettes" => {
                    expect_tag(&name, tag, TAG_LIST, "list")?;
                    if template.palettes.is_some() {
                        return Err(TemplateError::DuplicateField(name));
                    }
                    template.palettes = Some(self.read_palettes(2)?);
                }
                "blocks" => {
                    expect_tag(&name, tag, TAG_LIST, "list")?;
                    if template.blocks.is_some() {
                        return Err(TemplateError::DuplicateField(name));
                    }
                    template.blocks = Some(self.read_blocks(2)?);
                }
                _ => self.skip_payload(tag, 2)?,
            }
        }
        if self.offset != self.bytes.len() {
            return Err(NbtError::TrailingData {
                offset: self.offset,
            }
            .into());
        }
        Ok(template)
    }

    fn read_palette(&mut self, depth: usize) -> Result<Vec<TemplatePaletteEntry>, TemplateError> {
        self.ensure_depth(depth)?;
        let element_tag = self.read_u8()?;
        expect_tag("palette elements", element_tag, TAG_COMPOUND, "compound")?;
        let len = self.read_collection_len()?;
        if len > self.limits.max_palette_entries {
            return Err(TemplateError::PaletteTooLarge {
                actual: len,
                limit: self.limits.max_palette_entries,
            });
        }
        self.claim_tags(len)?;
        let mut palette = Vec::with_capacity(len);
        for index in 0..len {
            self.ensure_depth(depth + 1)?;
            let mut name = None;
            let mut properties = BTreeMap::new();
            let mut has_properties = false;
            loop {
                let tag = self.read_u8()?;
                if tag == TAG_END {
                    break;
                }
                self.validate_tag(tag)?;
                self.claim_tags(1)?;
                let field = self.read_string()?;
                match field.as_str() {
                    "Name" => {
                        expect_tag(&field, tag, TAG_STRING, "string")?;
                        if name.is_some() {
                            return Err(TemplateError::DuplicateField(field));
                        }
                        name = Some(self.read_string()?);
                    }
                    "Properties" => {
                        expect_tag(&field, tag, TAG_COMPOUND, "compound")?;
                        if has_properties {
                            return Err(TemplateError::DuplicateField(field));
                        }
                        has_properties = true;
                        properties = self.read_properties(depth + 2)?;
                    }
                    _ => self.skip_payload(tag, depth + 2)?,
                }
            }
            let name = name.ok_or(TemplateError::MissingField("palette[].Name"))?;
            if name.is_empty() {
                return Err(TemplateError::EmptyPaletteName { index });
            }
            palette.push(TemplatePaletteEntry { name, properties });
        }
        Ok(palette)
    }

    fn read_palettes(&mut self, depth: usize) -> Result<Vec<Vec<TemplatePaletteEntry>>, TemplateError> {
        self.ensure_depth(depth)?;
        let element_tag = self.read_u8()?;
        expect_tag("palettes elements", element_tag, TAG_LIST, "list")?;
        let palette_count = self.read_collection_len()?;
        if palette_count > self.limits.max_palette_entries {
            return Err(TemplateError::PaletteTooLarge {
                actual: palette_count,
                limit: self.limits.max_palette_entries,
            });
        }
        self.claim_tags(palette_count)?;
        let mut palettes = Vec::with_capacity(palette_count);
        let mut expected_len = None;
        let mut total_entries = 0usize;
        for palette_index in 0..palette_count {
            let palette = self.read_palette(depth + 1)?;
            let expected = *expected_len.get_or_insert(palette.len());
            if palette.len() != expected {
                return Err(TemplateError::InconsistentPaletteLength {
                    palette_index,
                    actual: palette.len(),
                    expected,
                });
            }
            total_entries = total_entries.saturating_add(palette.len());
            if total_entries > self.limits.max_palette_entries {
                return Err(TemplateError::PaletteTooLarge {
                    actual: total_entries,
                    limit: self.limits.max_palette_entries,
                });
            }
            palettes.push(palette);
        }
        if palettes.is_empty() {
            return Err(TemplateError::MissingField("palettes[]"));
        }
        Ok(palettes)
    }

    fn read_properties(&mut self, depth: usize) -> Result<BTreeMap<String, String>, TemplateError> {
        self.ensure_depth(depth)?;
        let mut properties = BTreeMap::new();
        loop {
            let tag = self.read_u8()?;
            if tag == TAG_END {
                break;
            }
            self.validate_tag(tag)?;
            self.claim_tags(1)?;
            let name = self.read_string()?;
            expect_tag(&name, tag, TAG_STRING, "string")?;
            let value = self.read_string()?;
            if properties.insert(name.clone(), value).is_some() {
                return Err(TemplateError::DuplicateField(name));
            }
        }
        Ok(properties)
    }

    fn read_blocks(&mut self, depth: usize) -> Result<Vec<ParsedBlock>, TemplateError> {
        self.ensure_depth(depth)?;
        let element_tag = self.read_u8()?;
        expect_tag("blocks elements", element_tag, TAG_COMPOUND, "compound")?;
        let len = self.read_collection_len()?;
        if len > self.limits.max_blocks {
            return Err(TemplateError::BlockCountTooLarge {
                actual: len,
                limit: self.limits.max_blocks,
            });
        }
        self.claim_tags(len)?;
        let mut blocks = Vec::with_capacity(len);
        for _ in 0..len {
            self.ensure_depth(depth + 1)?;
            let mut position = None;
            let mut palette_index = None;
            loop {
                let tag = self.read_u8()?;
                if tag == TAG_END {
                    break;
                }
                self.validate_tag(tag)?;
                self.claim_tags(1)?;
                let field = self.read_string()?;
                match field.as_str() {
                    "pos" => {
                        expect_tag(&field, tag, TAG_LIST, "list")?;
                        if position.is_some() {
                            return Err(TemplateError::DuplicateField(field));
                        }
                        position = Some(self.read_int_triplet("blocks[].pos", depth + 2)?);
                    }
                    "state" => {
                        expect_tag(&field, tag, TAG_INT, "int")?;
                        if palette_index.is_some() {
                            return Err(TemplateError::DuplicateField(field));
                        }
                        palette_index = Some(self.read_i32()?);
                    }
                    _ => self.skip_payload(tag, depth + 2)?,
                }
            }
            blocks.push(ParsedBlock {
                position: position.ok_or(TemplateError::MissingField("blocks[].pos"))?,
                palette_index: palette_index
                    .ok_or(TemplateError::MissingField("blocks[].state"))?,
            });
        }
        Ok(blocks)
    }

    fn read_int_triplet(&mut self, field: &str, depth: usize) -> Result<[i32; 3], TemplateError> {
        self.ensure_depth(depth)?;
        let element_tag = self.read_u8()?;
        expect_tag(field, element_tag, TAG_INT, "int list")?;
        let len = self.read_collection_len()?;
        if len != 3 {
            return Err(TemplateError::WrongTag {
                field: field.to_string(),
                expected: "list of exactly three ints",
                actual: TAG_LIST,
            });
        }
        self.claim_tags(3)?;
        Ok([self.read_i32()?, self.read_i32()?, self.read_i32()?])
    }

    fn skip_payload(&mut self, tag: u8, depth: usize) -> Result<(), TemplateError> {
        self.ensure_depth(depth)?;
        match tag {
            TAG_BYTE => self.skip(1)?,
            TAG_SHORT => self.skip(2)?,
            TAG_INT | TAG_FLOAT => self.skip(4)?,
            TAG_LONG | TAG_DOUBLE => self.skip(8)?,
            TAG_BYTE_ARRAY => {
                let len = self.read_collection_len()?;
                self.skip(len)?;
            }
            TAG_STRING => {
                self.read_string()?;
            }
            TAG_LIST => {
                let element_tag = self.read_u8()?;
                self.validate_list_element_tag(element_tag)?;
                let len = self.read_collection_len()?;
                self.claim_tags(len)?;
                for _ in 0..len {
                    self.skip_payload(element_tag, depth + 1)?;
                }
            }
            TAG_COMPOUND => loop {
                let child_tag = self.read_u8()?;
                if child_tag == TAG_END {
                    break;
                }
                self.validate_tag(child_tag)?;
                self.claim_tags(1)?;
                self.read_string()?;
                self.skip_payload(child_tag, depth + 1)?;
            },
            TAG_INT_ARRAY => {
                let len = self.read_collection_len()?;
                self.skip(len.checked_mul(4).ok_or(NbtError::CollectionTooLarge {
                    length: len,
                    limit: self.limits.max_collection_len,
                })?)?;
            }
            TAG_LONG_ARRAY => {
                let len = self.read_collection_len()?;
                self.skip(len.checked_mul(8).ok_or(NbtError::CollectionTooLarge {
                    length: len,
                    limit: self.limits.max_collection_len,
                })?)?;
            }
            _ => {
                return Err(NbtError::InvalidTag {
                    tag,
                    offset: self.offset.saturating_sub(1),
                }
                .into())
            }
        }
        Ok(())
    }

    fn read_collection_len(&mut self) -> Result<usize, NbtError> {
        let length_offset = self.offset;
        let length = self.read_i32()?;
        if length < 0 {
            return Err(NbtError::NegativeLength {
                length,
                offset: length_offset,
            });
        }
        let length = length as usize;
        if length > self.limits.max_collection_len {
            return Err(NbtError::CollectionTooLarge {
                length,
                limit: self.limits.max_collection_len,
            });
        }
        Ok(length)
    }

    fn read_string(&mut self) -> Result<String, NbtError> {
        let length = self.read_u16()? as usize;
        if length > self.limits.max_string_bytes {
            return Err(NbtError::StringTooLarge {
                length,
                limit: self.limits.max_string_bytes,
            });
        }
        let start = self.offset;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| NbtError::InvalidUtf8 { offset: start })
    }

    fn read_u8(&mut self) -> Result<u8, NbtError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, NbtError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn read_i32(&mut self) -> Result<i32, NbtError> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], NbtError> {
        let Some(end) = self.offset.checked_add(len) else {
            return Err(NbtError::UnexpectedEof {
                offset: self.offset,
                needed: len,
            });
        };
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(NbtError::UnexpectedEof {
                offset: self.offset,
                needed: end.saturating_sub(self.bytes.len()),
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn skip(&mut self, len: usize) -> Result<(), NbtError> {
        self.take(len).map(|_| ())
    }

    fn ensure_depth(&self, depth: usize) -> Result<(), NbtError> {
        if depth > self.limits.max_depth {
            return Err(NbtError::DepthLimit {
                depth,
                limit: self.limits.max_depth,
            });
        }
        Ok(())
    }

    fn claim_tags(&mut self, count: usize) -> Result<(), NbtError> {
        self.tags = self.tags.checked_add(count).ok_or(NbtError::TagLimit {
            limit: self.limits.max_tags,
        })?;
        if self.tags > self.limits.max_tags {
            return Err(NbtError::TagLimit {
                limit: self.limits.max_tags,
            });
        }
        Ok(())
    }

    fn validate_tag(&self, tag: u8) -> Result<(), NbtError> {
        if (TAG_BYTE..=TAG_LONG_ARRAY).contains(&tag) {
            Ok(())
        } else {
            Err(NbtError::InvalidTag {
                tag,
                offset: self.offset.saturating_sub(1),
            })
        }
    }

    fn validate_list_element_tag(&self, tag: u8) -> Result<(), NbtError> {
        if tag == TAG_END || (TAG_BYTE..=TAG_LONG_ARRAY).contains(&tag) {
            Ok(())
        } else {
            Err(NbtError::InvalidTag {
                tag,
                offset: self.offset.saturating_sub(1),
            })
        }
    }
}

fn expect_tag(
    field: &str,
    actual: u8,
    expected_tag: u8,
    expected: &'static str,
) -> Result<(), TemplateError> {
    if actual == expected_tag {
        Ok(())
    } else {
        Err(TemplateError::WrongTag {
            field: field.to_string(),
            expected,
            actual,
        })
    }
}

fn transformed_properties(
    properties: &BTreeMap<String, String>,
    transform: TemplateTransform,
) -> BTreeMap<String, String> {
    let mut transformed = BTreeMap::new();
    for (name, value) in properties {
        let mut property_name = name.as_str();
        let mut property_value = value.as_str();
        if matches!(property_name, "north" | "east" | "south" | "west") {
            property_name = transform_direction(property_name, transform);
        } else if property_name == "facing" && matches!(property_value, "north" | "east" | "south" | "west") {
            property_value = transform_direction(property_value, transform);
        } else if property_name == "axis"
            && matches!(transform.rotation, Rotation::Clockwise90 | Rotation::Counterclockwise90)
        {
            property_value = match property_value {
                "x" => "z",
                "z" => "x",
                other => other,
            };
        } else if property_name == "hinge" && transform.mirror != Mirror::None {
            property_value = match property_value {
                "left" => "right",
                "right" => "left",
                other => other,
            };
        } else if property_name == "shape" && transform.mirror != Mirror::None {
            property_value = match property_value {
                "inner_left" => "inner_right",
                "inner_right" => "inner_left",
                "outer_left" => "outer_right",
                "outer_right" => "outer_left",
                other => other,
            };
        }
        transformed.insert(property_name.to_owned(), property_value.to_owned());
    }
    transformed
}

fn transform_direction(direction: &str, transform: TemplateTransform) -> &'static str {
    let mut direction = match (transform.mirror, direction) {
        (Mirror::FrontBack, "east") => "west",
        (Mirror::FrontBack, "west") => "east",
        (Mirror::LeftRight, "north") => "south",
        (Mirror::LeftRight, "south") => "north",
        (_, "north") => "north",
        (_, "east") => "east",
        (_, "south") => "south",
        (_, "west") => "west",
        _ => return "north",
    };
    let turns = match transform.rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 1,
        Rotation::Clockwise180 => 2,
        Rotation::Counterclockwise90 => 3,
    };
    for _ in 0..turns {
        direction = match direction {
            "north" => "east",
            "east" => "south",
            "south" => "west",
            "west" => "north",
            _ => unreachable!(),
        };
    }
    direction
}

fn legacy_data_for_state(id: BlockId, properties: &BTreeMap<String, String>) -> u8 {
    match id {
        BlockId::Water | BlockId::Lava => properties
            .get("level")
            .and_then(|level| level.parse().ok())
            .unwrap_or(0),
        BlockId::StoneSlab | BlockId::OakSlab => u8::from(
            properties.get("type").is_some_and(|slab_type| slab_type == "top"),
        ),
        BlockId::StoneStairs | BlockId::OakStairs => {
            let facing = match properties.get("facing").map(String::as_str) {
                Some("west") => 1,
                Some("north") => 2,
                Some("east") => 3,
                _ => 0,
            };
            let half = if properties.get("half").is_some_and(|half| half == "top") {
                4
            } else {
                0
            };
            facing | half
        }
        _ => 0,
    }
}

fn property_insensitive_full_cube(
    id: BlockId,
    properties: &BTreeMap<String, String>,
) -> bool {
    let allowed = match id {
        BlockId::GrassBlock | BlockId::Podzol | BlockId::Mycelium => &["snowy"][..],
        BlockId::Snow => &["layers"][..],
        BlockId::OakLog
        | BlockId::SpruceLog
        | BlockId::BirchLog
        | BlockId::JungleLog
        | BlockId::AcaciaLog
        | BlockId::DarkOakLog
        | BlockId::CherryLog
        | BlockId::MangroveLog
        | BlockId::OakWood
        | BlockId::SpruceWood
        | BlockId::BirchWood
        | BlockId::JungleWood
        | BlockId::AcaciaWood
        | BlockId::DarkOakWood
        | BlockId::CherryWood
        | BlockId::MangroveWood
        | BlockId::StrippedOakLog
        | BlockId::StrippedSpruceLog
        | BlockId::StrippedBirchLog
        | BlockId::StrippedJungleLog
        | BlockId::StrippedAcaciaLog
        | BlockId::StrippedDarkOakLog
        | BlockId::StrippedCherryLog
        | BlockId::StrippedMangroveLog
        | BlockId::StrippedOakWood
        | BlockId::StrippedSpruceWood
        | BlockId::StrippedBirchWood
        | BlockId::StrippedJungleWood
        | BlockId::StrippedAcaciaWood
        | BlockId::StrippedDarkOakWood
        | BlockId::StrippedCherryWood
        | BlockId::StrippedMangroveWood
        | BlockId::BoneBlock
        | BlockId::HayBlock
        | BlockId::Basalt => &["axis"][..],
        BlockId::OakLeaves
        | BlockId::SpruceLeaves
        | BlockId::BirchLeaves
        | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves
        | BlockId::DarkOakLeaves
        | BlockId::CherryLeaves
        | BlockId::MangroveLeaves => &["distance", "persistent", "waterlogged"][..],
        _ => return false,
    };
    properties.keys().all(|name| allowed.contains(&name.as_str()))
}

fn native_block_id(name: &str) -> Option<BlockId> {
    Some(match name {
        "minecraft:air" => BlockId::Air,
        "minecraft:stone" => BlockId::Stone,
        "minecraft:grass_block" => BlockId::GrassBlock,
        "minecraft:dirt" => BlockId::Dirt,
        "minecraft:coarse_dirt" => BlockId::CoarseDirt,
        "minecraft:podzol" => BlockId::Podzol,
        "minecraft:mycelium" => BlockId::Mycelium,
        "minecraft:cobblestone" => BlockId::Cobblestone,
        "minecraft:mossy_cobblestone" => BlockId::MossyCobblestone,
        "minecraft:stone_bricks" => BlockId::StoneBricks,
        "minecraft:bricks" => BlockId::Bricks,
        "minecraft:bedrock" => BlockId::Bedrock,
        "minecraft:sand" => BlockId::Sand,
        "minecraft:red_sand" => BlockId::RedSand,
        "minecraft:gravel" => BlockId::Gravel,
        "minecraft:sandstone" => BlockId::Sandstone,
        "minecraft:smooth_sandstone" => BlockId::SmoothSandstone,
        "minecraft:smooth_red_sandstone" => BlockId::SmoothRedSandstone,
        "minecraft:granite" => BlockId::Granite,
        "minecraft:diorite" => BlockId::Diorite,
        "minecraft:andesite" => BlockId::Andesite,
        "minecraft:deepslate" => BlockId::Deepslate,
        "minecraft:cobbled_deepslate" => BlockId::CobbledDeepslate,
        "minecraft:polished_deepslate" => BlockId::PolishedDeepslate,
        "minecraft:deepslate_bricks" => BlockId::DeepslateBricks,
        "minecraft:deepslate_tiles" => BlockId::DeepslateTiles,
        "minecraft:calcite" => BlockId::Calcite,
        "minecraft:tuff" => BlockId::Tuff,
        "minecraft:dripstone_block" => BlockId::DripstoneBlock,
        "minecraft:water" => BlockId::Water,
        "minecraft:lava" => BlockId::Lava,
        "minecraft:ice" => BlockId::Ice,
        "minecraft:packed_ice" => BlockId::PackedIce,
        "minecraft:blue_ice" => BlockId::BlueIce,
        "minecraft:snow" => BlockId::Snow,
        "minecraft:snow_block" => BlockId::SnowBlock,
        "minecraft:powder_snow" => BlockId::PowderSnow,
        "minecraft:oak_planks" => BlockId::OakPlanks,
        "minecraft:spruce_planks" => BlockId::SprucePlanks,
        "minecraft:birch_planks" => BlockId::BirchPlanks,
        "minecraft:jungle_planks" => BlockId::JunglePlanks,
        "minecraft:acacia_planks" => BlockId::AcaciaPlanks,
        "minecraft:dark_oak_planks" => BlockId::DarkOakPlanks,
        "minecraft:cherry_planks" => BlockId::CherryPlanks,
        "minecraft:mangrove_planks" => BlockId::MangrovePlanks,
        "minecraft:bamboo_planks" => BlockId::BambooPlanks,
        "minecraft:oak_log" => BlockId::OakLog,
        "minecraft:spruce_log" => BlockId::SpruceLog,
        "minecraft:birch_log" => BlockId::BirchLog,
        "minecraft:jungle_log" => BlockId::JungleLog,
        "minecraft:acacia_log" => BlockId::AcaciaLog,
        "minecraft:dark_oak_log" => BlockId::DarkOakLog,
        "minecraft:cherry_log" => BlockId::CherryLog,
        "minecraft:mangrove_log" => BlockId::MangroveLog,
        "minecraft:oak_wood" => BlockId::OakWood,
        "minecraft:spruce_wood" => BlockId::SpruceWood,
        "minecraft:birch_wood" => BlockId::BirchWood,
        "minecraft:jungle_wood" => BlockId::JungleWood,
        "minecraft:acacia_wood" => BlockId::AcaciaWood,
        "minecraft:dark_oak_wood" => BlockId::DarkOakWood,
        "minecraft:cherry_wood" => BlockId::CherryWood,
        "minecraft:mangrove_wood" => BlockId::MangroveWood,
        "minecraft:stripped_oak_log" => BlockId::StrippedOakLog,
        "minecraft:stripped_spruce_log" => BlockId::StrippedSpruceLog,
        "minecraft:stripped_birch_log" => BlockId::StrippedBirchLog,
        "minecraft:stripped_jungle_log" => BlockId::StrippedJungleLog,
        "minecraft:stripped_acacia_log" => BlockId::StrippedAcaciaLog,
        "minecraft:stripped_dark_oak_log" => BlockId::StrippedDarkOakLog,
        "minecraft:stripped_cherry_log" => BlockId::StrippedCherryLog,
        "minecraft:stripped_mangrove_log" => BlockId::StrippedMangroveLog,
        "minecraft:stripped_oak_wood" => BlockId::StrippedOakWood,
        "minecraft:stripped_spruce_wood" => BlockId::StrippedSpruceWood,
        "minecraft:stripped_birch_wood" => BlockId::StrippedBirchWood,
        "minecraft:stripped_jungle_wood" => BlockId::StrippedJungleWood,
        "minecraft:stripped_acacia_wood" => BlockId::StrippedAcaciaWood,
        "minecraft:stripped_dark_oak_wood" => BlockId::StrippedDarkOakWood,
        "minecraft:stripped_cherry_wood" => BlockId::StrippedCherryWood,
        "minecraft:stripped_mangrove_wood" => BlockId::StrippedMangroveWood,
        "minecraft:oak_leaves" => BlockId::OakLeaves,
        "minecraft:spruce_leaves" => BlockId::SpruceLeaves,
        "minecraft:birch_leaves" => BlockId::BirchLeaves,
        "minecraft:jungle_leaves" => BlockId::JungleLeaves,
        "minecraft:acacia_leaves" => BlockId::AcaciaLeaves,
        "minecraft:dark_oak_leaves" => BlockId::DarkOakLeaves,
        "minecraft:cherry_leaves" => BlockId::CherryLeaves,
        "minecraft:mangrove_leaves" => BlockId::MangroveLeaves,
        "minecraft:glass" => BlockId::Glass,
        "minecraft:bookshelf" => BlockId::Bookshelf,
        "minecraft:crafting_table" => BlockId::CraftingTable,
        "minecraft:furnace" => BlockId::Furnace,
        "minecraft:chest" => BlockId::Chest,
        "minecraft:spawner" => BlockId::Spawner,
        "minecraft:torch" => BlockId::Torch,
        "minecraft:wall_torch" => BlockId::WallTorch,
        "minecraft:glowstone" => BlockId::Glowstone,
        "minecraft:sea_lantern" => BlockId::SeaLantern,
        "minecraft:shroomlight" => BlockId::Shroomlight,
        "minecraft:obsidian" => BlockId::Obsidian,
        "minecraft:crying_obsidian" => BlockId::CryingObsidian,
        "minecraft:netherrack" => BlockId::Netherrack,
        "minecraft:soul_sand" => BlockId::SoulSand,
        "minecraft:soul_soil" => BlockId::SoulSoil,
        "minecraft:blackstone" => BlockId::Blackstone,
        "minecraft:basalt" => BlockId::Basalt,
        "minecraft:copper_block" => BlockId::CopperBlock,
        "minecraft:exposed_copper" => BlockId::ExposedCopper,
        "minecraft:weathered_copper" => BlockId::WeatheredCopper,
        "minecraft:oxidized_copper" => BlockId::OxidizedCopper,
        "minecraft:cut_copper" => BlockId::CutCopper,
        "minecraft:exposed_cut_copper" => BlockId::ExposedCutCopper,
        "minecraft:weathered_cut_copper" => BlockId::WeatheredCutCopper,
        "minecraft:oxidized_cut_copper" => BlockId::OxidizedCutCopper,
        "minecraft:waxed_copper_block" => BlockId::WaxedCopperBlock,
        "minecraft:waxed_exposed_copper" => BlockId::WaxedExposedCopper,
        "minecraft:waxed_weathered_copper" => BlockId::WaxedWeatheredCopper,
        "minecraft:waxed_oxidized_copper" => BlockId::WaxedOxidizedCopper,
        "minecraft:waxed_cut_copper" => BlockId::WaxedCutCopper,
        "minecraft:waxed_exposed_cut_copper" => BlockId::WaxedExposedCutCopper,
        "minecraft:waxed_weathered_cut_copper" => BlockId::WaxedWeatheredCutCopper,
        "minecraft:waxed_oxidized_cut_copper" => BlockId::WaxedOxidizedCutCopper,
        "minecraft:magma_block" => BlockId::MagmaBlock,
        "minecraft:end_stone" => BlockId::EndStone,
        "minecraft:prismarine" => BlockId::Prismarine,
        "minecraft:prismarine_bricks" => BlockId::PrismarineBricks,
        "minecraft:dark_prismarine" => BlockId::DarkPrismarine,
        "minecraft:terracotta" => BlockId::Terracotta,
        "minecraft:mud" => BlockId::Mud,
        "minecraft:packed_mud" => BlockId::PackedMud,
        "minecraft:mud_bricks" => BlockId::MudBricks,
        "minecraft:moss_block" => BlockId::MossBlock,
        "minecraft:rooted_dirt" => BlockId::RootedDirt,
        "minecraft:bone_block" => BlockId::BoneBlock,
        "minecraft:hay_block" => BlockId::HayBlock,
        "minecraft:pumpkin" => BlockId::Pumpkin,
        "minecraft:carved_pumpkin" => BlockId::CarvedPumpkin,
        "minecraft:jack_o_lantern" => BlockId::JackOLantern,
        "minecraft:melon" => BlockId::Melon,
        "minecraft:cactus" => BlockId::Cactus,
        "minecraft:sugar_cane" => BlockId::SugarCane,
        "minecraft:vine" => BlockId::Vine,
        "minecraft:lily_pad" => BlockId::LilyPad,
        "minecraft:ladder" => BlockId::Ladder,
        "minecraft:oak_fence" => BlockId::OakFence,
        "minecraft:oak_door" => BlockId::OakDoor,
        "minecraft:stone_slab" => BlockId::StoneSlab,
        "minecraft:oak_slab" => BlockId::OakSlab,
        "minecraft:cobblestone_stairs" => BlockId::StoneStairs,
        "minecraft:oak_stairs" => BlockId::OakStairs,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Clone)]
    struct FixturePalette<'a> {
        name: &'a str,
        properties: &'a [(&'a str, &'a str)],
    }

    fn write_name(bytes: &mut Vec<u8>, name: &str) {
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
    }

    fn named_tag(bytes: &mut Vec<u8>, tag: u8, name: &str) {
        bytes.push(tag);
        write_name(bytes, name);
    }

    fn write_string(bytes: &mut Vec<u8>, value: &str) {
        write_name(bytes, value);
    }

    fn fixture_nbt(
        size: [i32; 3],
        palette: &[FixturePalette<'_>],
        blocks: &[([i32; 3], i32)],
    ) -> Vec<u8> {
        let mut bytes = vec![TAG_COMPOUND, 0, 0];

        named_tag(&mut bytes, TAG_LIST, "size");
        bytes.push(TAG_INT);
        bytes.extend_from_slice(&3_i32.to_be_bytes());
        for coordinate in size {
            bytes.extend_from_slice(&coordinate.to_be_bytes());
        }

        named_tag(&mut bytes, TAG_LIST, "palette");
        bytes.push(TAG_COMPOUND);
        bytes.extend_from_slice(&(palette.len() as i32).to_be_bytes());
        for entry in palette {
            named_tag(&mut bytes, TAG_STRING, "Name");
            write_string(&mut bytes, entry.name);
            if !entry.properties.is_empty() {
                named_tag(&mut bytes, TAG_COMPOUND, "Properties");
                for (name, value) in entry.properties {
                    named_tag(&mut bytes, TAG_STRING, name);
                    write_string(&mut bytes, value);
                }
                bytes.push(TAG_END);
            }
            bytes.push(TAG_END);
        }

        named_tag(&mut bytes, TAG_LIST, "blocks");
        bytes.push(TAG_COMPOUND);
        bytes.extend_from_slice(&(blocks.len() as i32).to_be_bytes());
        for (position, palette_index) in blocks {
            named_tag(&mut bytes, TAG_LIST, "pos");
            bytes.push(TAG_INT);
            bytes.extend_from_slice(&3_i32.to_be_bytes());
            for coordinate in position {
                bytes.extend_from_slice(&coordinate.to_be_bytes());
            }
            named_tag(&mut bytes, TAG_INT, "state");
            bytes.extend_from_slice(&palette_index.to_be_bytes());
            bytes.push(TAG_END);
        }
        bytes.push(TAG_END);
        bytes
    }

    fn plural_palette_fixture_nbt(
        size: [i32; 3],
        palettes: &[&[FixturePalette<'_>]],
        blocks: &[([i32; 3], i32)],
    ) -> Vec<u8> {
        let mut bytes = fixture_nbt(size, palettes[0], blocks);
        let palette_name = bytes
            .windows("palette".len())
            .position(|window| window == b"palette")
            .unwrap();
        let tag_start = palette_name - 3;
        let blocks_name = bytes
            .windows("blocks".len())
            .position(|window| window == b"blocks")
            .unwrap();
        let blocks_tag_start = blocks_name - 3;
        bytes.drain(tag_start..blocks_tag_start);

        let mut plural = Vec::new();
        named_tag(&mut plural, TAG_LIST, "palettes");
        plural.push(TAG_LIST);
        plural.extend_from_slice(&(palettes.len() as i32).to_be_bytes());
        for palette in palettes {
            plural.push(TAG_COMPOUND);
            plural.extend_from_slice(&(palette.len() as i32).to_be_bytes());
            for entry in *palette {
                named_tag(&mut plural, TAG_STRING, "Name");
                write_string(&mut plural, entry.name);
                if !entry.properties.is_empty() {
                    named_tag(&mut plural, TAG_COMPOUND, "Properties");
                    for (name, value) in entry.properties {
                        named_tag(&mut plural, TAG_STRING, name);
                        write_string(&mut plural, value);
                    }
                    plural.push(TAG_END);
                }
                plural.push(TAG_END);
            }
        }
        bytes.splice(tag_start..tag_start, plural);
        bytes
    }

    fn gzip(nbt: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(nbt).unwrap();
        encoder.finish().unwrap()
    }

    fn basic_template() -> StructureTemplate {
        StructureTemplate::decode_gzip(&gzip(&fixture_nbt(
            [3, 1, 2],
            &[FixturePalette {
                name: "minecraft:stone",
                properties: &[("variant", "smooth")],
            }],
            &[([0, 0, 0], 0), ([2, 0, 1], 0)],
        )))
        .unwrap()
    }

    #[test]
    fn decodes_gzip_template_from_asset_root() {
        static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "vibecraft-template-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let relative = Path::new("data/minecraft/structure/test.nbt");
        fs::create_dir_all(root.join("data/minecraft/structure")).unwrap();
        fs::write(
            root.join(relative),
            gzip(&fixture_nbt(
                [2, 3, 4],
                &[FixturePalette {
                    name: "minecraft:oak_log",
                    properties: &[("axis", "x")],
                }],
                &[([1, 2, 3], 0)],
            )),
        )
        .unwrap();

        let template = StructureTemplate::load_from_asset_root(&root, relative).unwrap();
        assert_eq!(template.size, [2, 3, 4]);
        assert_eq!(template.palette(0).unwrap()[0].name, "minecraft:oak_log");
        assert_eq!(template.palette(0).unwrap()[0].properties["axis"], "x");
        assert_eq!(template.blocks[0].position, [1, 2, 3]);
        assert_eq!(template.blocks[0].palette_index, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transforms_coordinates_at_negative_origins() {
        let template = basic_template();
        let position = [0, 0, 0];
        assert_eq!(
            template.transform_position(
                position,
                TemplateTransform {
                    mirror: Mirror::FrontBack,
                    rotation: Rotation::None,
                }
            ),
            [2, 0, 0]
        );
        assert_eq!(
            template.transform_position(
                position,
                TemplateTransform {
                    mirror: Mirror::LeftRight,
                    rotation: Rotation::None,
                }
            ),
            [0, 0, 1]
        );
        assert_eq!(
            template.transform_position(
                position,
                TemplateTransform {
                    mirror: Mirror::None,
                    rotation: Rotation::Clockwise90,
                }
            ),
            [1, 0, 0]
        );
        assert_eq!(
            template.transform_position(
                position,
                TemplateTransform {
                    mirror: Mirror::None,
                    rotation: Rotation::Clockwise180,
                }
            ),
            [2, 0, 1]
        );
        assert_eq!(
            template.transform_position(
                position,
                TemplateTransform {
                    mirror: Mirror::None,
                    rotation: Rotation::Counterclockwise90,
                }
            ),
            [0, 0, 2]
        );

        let projected = template
            .blocks_in_chunk([-17, -10, -1], TemplateTransform::default(), [-2, -1])
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(projected.world_position, [-17, -10, -1]);
    }

    #[test]
    fn chunk_projection_filters_without_materializing_other_chunks() {
        let template = basic_template();
        let first: Vec<_> = template
            .blocks_in_chunk([-17, 5, -1], TemplateTransform::default(), [-2, -1])
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].world_position, [-17, 5, -1]);

        let second: Vec<_> = template
            .blocks_in_chunk([-17, 5, -1], TemplateTransform::default(), [-1, 0])
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].world_position, [-15, 5, 0]);
    }

    #[test]
    fn rejects_malformed_palette_index() {
        let error = StructureTemplate::decode_gzip(&gzip(&fixture_nbt(
            [1, 1, 1],
            &[FixturePalette {
                name: "minecraft:stone",
                properties: &[],
            }],
            &[([0, 0, 0], 4)],
        )))
        .unwrap_err();
        assert!(matches!(
            error,
            TemplateError::InvalidPaletteIndex {
                block_index: 0,
                palette_index: 4,
                palette_len: 1
            }
        ));
    }

    #[test]
    fn structure_markers_project_as_no_ops() {
        let template = StructureTemplate::decode_gzip(&gzip(&fixture_nbt(
            [1, 1, 1],
            &[FixturePalette {
                name: "minecraft:structure_void",
                properties: &[],
            }],
            &[([0, 0, 0], 0)],
        )))
        .unwrap();
        let projected = template
            .blocks_in_chunk([0, 0, 0], TemplateTransform::default(), [0, 0])
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(
            projected.palette.flatten(TemplateTransform::default()).unwrap(),
            FlattenedTemplateBlock::NoOp
        );
    }

    #[test]
    fn decodes_and_selects_plural_palettes() {
        let first = [FixturePalette {
            name: "minecraft:oak_planks",
            properties: &[],
        }];
        let second = [FixturePalette {
            name: "minecraft:spruce_planks",
            properties: &[],
        }];
        let template = StructureTemplate::decode_gzip(&gzip(&plural_palette_fixture_nbt(
            [1, 1, 1],
            &[&first, &second],
            &[([0, 0, 0], 0)],
        )))
        .unwrap();
        assert_eq!(template.palettes.len(), 2);
        let projected = template
            .blocks_in_chunk_with_palette(
                [0, 0, 0],
                TemplateTransform::default(),
                [0, 0],
                1,
            )
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(projected.block_id, BlockId::SprucePlanks);
    }

    #[test]
    fn safe_state_flattening_preserves_cubes_and_rotates_native_states() {
        let log = TemplatePaletteEntry {
            name: "minecraft:oak_log".to_owned(),
            properties: BTreeMap::from([("axis".to_owned(), "x".to_owned())]),
        };
        assert_eq!(
            log.flatten(TemplateTransform::default()).unwrap(),
            FlattenedTemplateBlock::Place(Block::new(BlockId::OakLog))
        );
        for (name, properties, expected) in [
            ("minecraft:grass_block", vec![("snowy", "true")], BlockId::GrassBlock),
            ("minecraft:snow", vec![("layers", "7")], BlockId::Snow),
            ("minecraft:water", vec![("level", "0")], BlockId::Water),
        ] {
            let entry = TemplatePaletteEntry {
                name: name.to_owned(),
                properties: properties
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect(),
            };
            assert!(matches!(
                entry.flatten(TemplateTransform::default()),
                Ok(FlattenedTemplateBlock::Place(block)) if block.id == expected
            ));
        }

        let furnace = TemplatePaletteEntry {
            name: "minecraft:furnace".to_owned(),
            properties: BTreeMap::from([
                ("facing".to_owned(), "north".to_owned()),
                ("lit".to_owned(), "false".to_owned()),
            ]),
        };
        let FlattenedTemplateBlock::Place(rotated) = furnace
            .flatten(TemplateTransform {
                rotation: Rotation::Clockwise90,
                ..TemplateTransform::default()
            })
            .unwrap()
        else {
            panic!("furnace must remain representable");
        };
        let properties = registry().properties_for_state(rotated.id, rotated.state).unwrap();
        assert!(properties.contains(&("facing", "east")));

        let unsupported = TemplatePaletteEntry {
            name: "minecraft:carved_pumpkin".to_owned(),
            properties: BTreeMap::from([("facing".to_owned(), "north".to_owned())]),
        };
        assert!(matches!(
            unsupported.flatten(TemplateTransform::default()),
            Err(UnsupportedTemplateState::State { .. })
        ));
    }

    #[test]
    fn rejects_truncated_depth_and_size_bomb_inputs() {
        let mut truncated = gzip(&fixture_nbt(
            [1, 1, 1],
            &[FixturePalette {
                name: "minecraft:stone",
                properties: &[],
            }],
            &[([0, 0, 0], 0)],
        ));
        truncated.truncate(truncated.len() - 4);
        assert!(StructureTemplate::decode_gzip(&truncated).is_err());

        let mut nested = vec![TAG_COMPOUND, 0, 0];
        for _ in 0..40 {
            named_tag(&mut nested, TAG_COMPOUND, "nested");
        }
        nested.extend(std::iter::repeat_n(TAG_END, 41));
        assert!(matches!(
            StructureTemplate::decode_gzip(&gzip(&nested)),
            Err(TemplateError::Nbt(NbtError::DepthLimit { .. }))
        ));

        assert!(matches!(
            StructureTemplate::decode_gzip(&gzip(&fixture_nbt(
                [513, 1, 1],
                &[FixturePalette {
                    name: "minecraft:stone",
                    properties: &[],
                }],
                &[],
            ))),
            Err(TemplateError::InvalidSize { .. })
        ));
    }
}
