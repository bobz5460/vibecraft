use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use crate::assets::reader::AssetReader;

#[derive(Debug, Deserialize, Clone)]
pub struct ModelFile {
    pub parent: Option<String>,
    #[serde(default, deserialize_with = "deserialize_textures")]
    pub textures: Option<HashMap<String, String>>,
    pub elements: Option<Vec<ModelElement>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TextureDefinition {
    Name(String),
    Sprite { sprite: String },
}

fn deserialize_textures<'de, D>(deserializer: D) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let definitions = HashMap::<String, TextureDefinition>::deserialize(deserializer)?;
    Ok(Some(
        definitions
            .into_iter()
            .map(|(name, definition)| {
                let texture = match definition {
                    TextureDefinition::Name(texture) => texture,
                    TextureDefinition::Sprite { sprite } => sprite,
                };
                (name, texture)
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: HashMap<String, ModelFace>,
    #[serde(default)]
    pub rotation: Option<ElementRotation>,
    #[serde(default = "default_shade")]
    pub shade: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ElementRotation {
    pub origin: [f32; 3],
    pub axis: String,
    pub angle: f32,
    #[serde(default)]
    pub rescale: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelFace {
    pub texture: String,
    pub cullface: Option<String>,
    pub tintindex: Option<i32>,
    pub rotation: Option<i32>,
    pub uv: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModel {
    pub elements: Vec<ResolvedModelElement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModelElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: Vec<ResolvedModelFace>,
    pub rotation: Option<ElementRotation>,
    pub shade: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModelFace {
    pub direction: String,
    pub texture: String,
    pub cullface: Option<String>,
    pub tintindex: Option<i32>,
    pub rotation: i32,
    pub uv: Option<[f32; 4]>,
}

impl PartialEq for ElementRotation {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin
            && self.axis == other.axis
            && self.angle == other.angle
            && self.rescale == other.rescale
    }
}

const CUBE_FACE_ORDER: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

static MODEL_CACHE: std::sync::Mutex<Option<HashMap<String, ModelFile>>> =
    std::sync::Mutex::new(None);

pub fn load_model(reader: &AssetReader, name: &str) -> Option<ModelFile> {
    let cache_key = format!("models/block/{name}.json");
    {
        let cache = MODEL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(model) = cache.as_ref().and_then(|cache| cache.get(&cache_key)) {
            return Some(model.clone());
        }
    }

    let content = reader.read_to_string(&cache_key)?;
    let model: ModelFile = match serde_json::from_str(&content) {
        Ok(model) => model,
        Err(error) => {
            log::warn!("failed to parse block model {cache_key}: {error}");
            return None;
        }
    };
    let mut cache = MODEL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .get_or_insert_with(HashMap::new)
        .insert(cache_key, model.clone());
    Some(model)
}

/// Resolves parent inheritance, texture variables, every element, and per-face
/// geometry metadata. Blockstate rotations are intentionally retained by the
/// blockstate resolver rather than baked here.
pub fn resolve_model(reader: &AssetReader, model_name: &str) -> Option<ResolvedModel> {
    let (textures, elements) = effective_model(reader, model_name, &mut HashSet::new())?;
    let elements = elements
        .into_iter()
        .map(|element| ResolvedModelElement {
            from: element.from,
            to: element.to,
            faces: {
                let mut faces: Vec<_> = element.faces.into_iter().collect();
                faces.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
                faces
            }
                .into_iter()
                .map(|(direction, face)| ResolvedModelFace {
                    direction,
                    texture: normalize_texture_name(&resolve_texture_ref(&face.texture, &textures)),
                    cullface: face.cullface,
                    tintindex: face.tintindex,
                    rotation: face.rotation.unwrap_or(0).rem_euclid(360),
                    uv: face.uv,
                })
                .collect(),
            rotation: element.rotation,
            shade: element.shade,
        })
        .collect();
    Some(ResolvedModel { elements })
}

fn default_shade() -> bool {
    true
}

fn normalize_texture_name(texture: &str) -> String {
    texture
        .strip_prefix("minecraft:block/")
        .or_else(|| texture.strip_prefix("block/"))
        .unwrap_or(texture)
        .to_string()
}

fn resolve_texture_ref(texture: &str, resolved: &HashMap<String, String>) -> String {
    let mut current = texture;
    let mut visited = HashSet::new();
    while let Some(key) = current.strip_prefix('#') {
        if !visited.insert(key) {
            return texture.to_string();
        }
        match resolved.get(key) {
            Some(next) => current = next,
            None => return texture.to_string(),
        }
    }
    current.to_string()
}

fn effective_model(
    reader: &AssetReader,
    model_name: &str,
    visited: &mut HashSet<String>,
) -> Option<(HashMap<String, String>, Vec<ModelElement>)> {
    if !visited.insert(model_name.to_string()) {
        return None;
    }
    let model = load_model(reader, model_name)?;
    let (mut textures, inherited_elements) = if let Some(parent) = model.parent.as_deref() {
        let parent_name = parent
            .strip_prefix("minecraft:block/")
            .or_else(|| parent.strip_prefix("block/"))
            .unwrap_or(parent);
        effective_model(reader, parent_name, visited).unwrap_or_default()
    } else {
        (HashMap::new(), Vec::new())
    };
    textures.extend(model.textures.unwrap_or_default());
    Some((textures, model.elements.unwrap_or(inherited_elements)))
}

/// Compatibility projection for the legacy greedy cube mesher. It intentionally
/// returns only the first element's face textures; generic meshing consumes
/// `resolve_model` directly.
pub fn resolve_face_textures(reader: &AssetReader, model_name: &str) -> Option<HashMap<String, String>> {
    let model = resolve_model(reader, model_name)?;
    let mut out = HashMap::new();
    if let Some(element) = model.elements.first() {
        for direction in CUBE_FACE_ORDER {
            if let Some(face) = element.faces.iter().find(|face| face.direction == direction) {
                out.insert(direction.to_string(), face.texture.clone());
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_all_element_and_face_metadata() {
        let model: ModelFile = serde_json::from_str(
            r##"{
                "textures":{"base":"minecraft:block/grass_block_top"},
                "elements":[
                    {"from":[0,0,0],"to":[16,16,16],"shade":false,
                     "rotation":{"origin":[8,8,8],"axis":"y","angle":45,"rescale":true},
                     "faces":{"up":{"texture":"#base","uv":[0,0,16,16],"rotation":90,"tintindex":0,"cullface":"up"}}},
                    {"from":[1,1,1],"to":[15,15,15],"faces":{"north":{"texture":"block/stone"}}}
                ]
            }"##,
        )
        .unwrap();
        assert_eq!(model.elements.unwrap().len(), 2);
    }

    #[test]
    fn texture_reference_handles_cycles_without_looping() {
        let textures = HashMap::from([("a".to_string(), "#b".to_string()), ("b".to_string(), "#a".to_string())]);
        assert_eq!(resolve_texture_ref("#a", &textures), "#a");
    }

    #[test]
    fn deserializes_structured_26_2_texture_definitions() {
        let model: ModelFile = serde_json::from_str(
            r##"{
                "textures": {
                    "line": {"sprite":"block/redstone_dust_dot","force_translucent":true},
                    "particle": "block/redstone_dust_dot"
                }
            }"##,
        )
        .unwrap();
        let textures = model.textures.unwrap();
        assert_eq!(textures["line"], "block/redstone_dust_dot");
        assert_eq!(textures["particle"], "block/redstone_dust_dot");
    }
}
