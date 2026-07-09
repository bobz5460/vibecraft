use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct ModelFile {
    pub parent: Option<String>,
    pub textures: Option<HashMap<String, String>>,
    pub elements: Option<Vec<ModelElement>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: HashMap<String, ModelFace>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ModelFace {
    pub texture: String,
    pub cullface: Option<String>,
    #[serde(default)]
    pub tintindex: i32,
    #[serde(default)]
    pub rotation: i32,
}

const CUBE_FACE_ORDER: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

static MODEL_CACHE: std::sync::Mutex<Option<HashMap<String, ModelFile>>> = std::sync::Mutex::new(None);

pub fn load_model(model_dir: &Path, name: &str) -> Option<ModelFile> {
    {
        let cache = MODEL_CACHE.lock().unwrap();
        if let Some(cache) = cache.as_ref() {
            if let Some(m) = cache.get(name) {
                return Some(m.clone());
            }
        }
    }

    let path = model_dir.join(format!("{}.json", name));
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    let model: ModelFile = serde_json::from_str(&content).ok()?;

    {
        let mut cache = MODEL_CACHE.lock().unwrap();
        cache.get_or_insert_with(HashMap::new).insert(name.to_string(), model.clone());
    }

    Some(model)
}

fn resolve_texture_ref(texture: &str, resolved: &HashMap<String, String>) -> String {
    if let Some(stripped) = texture.strip_prefix('#') {
        resolved.get(stripped)
            .map(|s| resolve_texture_ref(s, resolved))
            .unwrap_or_else(|| texture.to_string())
    } else {
        texture.to_string()
    }
}

pub fn resolve_face_textures(model_dir: &Path, model_name: &str) -> Option<HashMap<String, String>> {
    let model = load_model(model_dir, model_name)?;
    let mut resolved = model.textures.clone().unwrap_or_default();

    if let Some(ref parent) = model.parent {
        let parent_name = parent.strip_prefix("minecraft:block/")
            .or_else(|| parent.strip_prefix("block/"))
            .unwrap_or(parent);
        if let Some(parent_textures) = resolve_face_textures(model_dir, parent_name) {
            for (k, v) in parent_textures {
                resolved.entry(k).or_insert(v);
            }
        }
    }

    let mut out = HashMap::new();
    if let Some(ref elements) = model.elements {
        // Only use the first element's base faces.
        // Skip overlay elements (used by grass_block, etc. for tintindex layers).
        if let Some(elem) = elements.first() {
            for face_name in &CUBE_FACE_ORDER {
                if let Some(face) = elem.faces.get(*face_name) {
                    let resolved_tex = resolve_texture_ref(&face.texture, &resolved);
                    let stripped = resolved_tex.strip_prefix("minecraft:block/")
                        .or_else(|| resolved_tex.strip_prefix("block/"))
                        .unwrap_or(&resolved_tex)
                        .to_string();
                    out.insert(face_name.to_string(), stripped);
                }
            }
        }
    }

    Some(out)
}
