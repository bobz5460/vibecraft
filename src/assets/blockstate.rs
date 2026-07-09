use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct BlockstateFile {
    pub variants: Option<HashMap<String, VariantValue>>,
    pub multipart: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum VariantValue {
    Single(Variant),
    Array(Vec<Variant>),
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Variant {
    pub model: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub uvlock: bool,
}

pub fn resolve_blockstate_model(blockstate_dir: &Path, name: &str) -> Option<String> {
    let path = blockstate_dir.join(format!("{}.json", name));
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    let bs: BlockstateFile = serde_json::from_str(&content).ok()?;

    if let Some(variants) = bs.variants {
        let mut keys: Vec<&String> = variants.keys().collect();
        keys.sort();
        for key in keys {
            let val = &variants[key];
            let model = match val {
                VariantValue::Single(v) => v.model.clone(),
                VariantValue::Array(arr) => arr.first()?.model.clone(),
            };
            let model_name = model.strip_prefix("minecraft:block/")
                .or_else(|| model.strip_prefix("block/"))
                .unwrap_or(&model)
                .to_string();
            return Some(model_name);
        }
    }

    // Handle multipart blockstate - use the first "apply" model
    if let Some(multipart_val) = &bs.multipart {
        if let Some(arr) = multipart_val.as_array() {
            if let Some(first) = arr.first() {
                if let Some(apply) = first.get("apply") {
                    if let Some(model) = apply.get("model").and_then(|m| m.as_str()) {
                        let model_name = model.strip_prefix("minecraft:block/")
                            .or_else(|| model.strip_prefix("block/"))
                            .unwrap_or(model)
                            .to_string();
                        return Some(model_name);
                    }
                }
            }
        }
    }

    None
}
