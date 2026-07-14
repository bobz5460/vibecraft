use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use crate::assets::reader::AssetReader;

#[derive(Debug, Deserialize, Clone)]
pub struct BlockstateFile {
    #[serde(default)]
    pub variants: HashMap<String, VariantValue>,
    #[serde(default)]
    pub multipart: Vec<MultipartPart>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MultipartPart {
    #[serde(default)]
    pub when: Option<Value>,
    pub apply: VariantValue,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum VariantValue {
    Single(Variant),
    Array(Vec<Variant>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Variant {
    pub model: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub uvlock: bool,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBlockstateModel {
    pub model: String,
    pub x: i32,
    pub y: i32,
    pub uvlock: bool,
}

fn default_weight() -> u32 {
    1
}

/// Resolves every model that applies to a state. The weighted choice is stable
/// for a block position and property tuple, so chunk rebuild order cannot alter
/// appearance.
pub fn resolve_blockstate_models(
    reader: &AssetReader,
    name: &str,
    properties: &HashMap<String, String>,
    position: (i32, i32, i32),
) -> Option<Vec<ResolvedBlockstateModel>> {
    let content = reader.read_to_string(&format!("blockstates/{name}.json"))?;
    let blockstate: BlockstateFile = serde_json::from_str(&content).ok()?;
    Some(resolve_models(&blockstate, properties, position))
}

/// Loads a blockstate once at startup so mesh workers never perform asset I/O.
pub fn load_blockstate(reader: &AssetReader, name: &str) -> Option<BlockstateFile> {
    let content = reader.read_to_string(&format!("blockstates/{name}.json"))?;
    serde_json::from_str(&content).ok()
}

/// Legacy cube-atlas compatibility helper. New model consumers should use
/// `resolve_blockstate_models` so multipart and transforms remain available.
pub fn resolve_blockstate_model(reader: &AssetReader, name: &str) -> Option<String> {
    let models = resolve_blockstate_models(reader, name, &HashMap::new(), (0, 0, 0))?;
    models.into_iter().next().map(|model| model.model)
}

pub fn resolve_models(
    blockstate: &BlockstateFile,
    properties: &HashMap<String, String>,
    position: (i32, i32, i32),
) -> Vec<ResolvedBlockstateModel> {
    let mut resolved = Vec::new();
    let mut variant_keys: Vec<_> = blockstate.variants.keys().collect();
    // The empty key is a fallback, not a lexicographically preferred state.
    // Prefer the matching key with the most declared properties, then retain a
    // stable lexical tie-breaker for malformed/overlapping asset definitions.
    variant_keys.sort_unstable_by(|left, right| {
        right
            .matches('=')
            .count()
            .cmp(&left.matches('=').count())
            .then_with(|| left.cmp(right))
    });
    let mut matched_variant = false;
    for key in &variant_keys {
        let key = *key;
        if variant_key_matches(key, properties) {
            if let Some(variant) = choose_variant(&blockstate.variants[key], properties, position, key) {
                resolved.push(to_resolved(variant));
            }
            matched_variant = true;
            break;
        }
    }
    // Legacy atlas construction has no state context for every block family.
    // Select a stable representative instead of silently producing a diagnostic
    // texture when an otherwise valid blockstate requires a property (for
    // example grass_block's `snowy=false`). Stateful mesh consumers always
    // provide real properties and therefore take the branch above.
    if !matched_variant {
        if let Some(key) = variant_keys.first() {
            let key = *key;
            if let Some(variant) = choose_variant(&blockstate.variants[key], properties, position, key) {
                resolved.push(to_resolved(variant));
            }
        }
    }
    for (index, part) in blockstate.multipart.iter().enumerate() {
        if part.when.as_ref().is_none_or(|condition| condition_matches(condition, properties)) {
            if let Some(variant) = choose_variant(&part.apply, properties, position, &format!("multipart:{index}")) {
                resolved.push(to_resolved(variant));
            }
        }
    }
    resolved
}

fn to_resolved(variant: &Variant) -> ResolvedBlockstateModel {
    ResolvedBlockstateModel {
        model: normalize_model_name(&variant.model),
        x: variant.x.rem_euclid(360),
        y: variant.y.rem_euclid(360),
        uvlock: variant.uvlock,
    }
}

fn normalize_model_name(model: &str) -> String {
    model.strip_prefix("minecraft:block/")
        .or_else(|| model.strip_prefix("block/"))
        .unwrap_or(model)
        .to_string()
}

fn choose_variant<'a>(
    value: &'a VariantValue,
    properties: &HashMap<String, String>,
    position: (i32, i32, i32),
    salt: &str,
) -> Option<&'a Variant> {
    match value {
        VariantValue::Single(variant) => Some(variant),
        VariantValue::Array(variants) => {
            let total_weight = variants.iter().map(|variant| variant.weight.max(1)).sum::<u32>();
            if total_weight == 0 {
                return None;
            }
            let mut pick = stable_hash(properties, position, salt) % total_weight as u64;
            for variant in variants {
                let weight = variant.weight.max(1) as u64;
                if pick < weight {
                    return Some(variant);
                }
                pick -= weight;
            }
            None
        }
    }
}

fn stable_hash(properties: &HashMap<String, String>, position: (i32, i32, i32), salt: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut write = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    write(salt.as_bytes());
    for coordinate in [position.0, position.1, position.2] {
        write(&coordinate.to_le_bytes());
    }
    let mut entries: Vec<_> = properties.iter().collect();
    entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    for (key, value) in entries {
        write(key.as_bytes());
        write(&[0]);
        write(value.as_bytes());
        write(&[0xff]);
    }
    hash
}

fn variant_key_matches(key: &str, properties: &HashMap<String, String>) -> bool {
    key.is_empty()
        || key.split(',').all(|entry| {
            let Some((name, accepted)) = entry.split_once('=') else {
                return false;
            };
            property_matches(properties, name, accepted)
        })
}

fn condition_matches(condition: &Value, properties: &HashMap<String, String>) -> bool {
    let Some(object) = condition.as_object() else {
        return false;
    };
    object.iter().all(|(key, value)| match key.as_str() {
        "OR" => value
            .as_array()
            .is_some_and(|conditions| conditions.iter().any(|nested| condition_matches(nested, properties))),
        "AND" => value
            .as_array()
            .is_some_and(|conditions| conditions.iter().all(|nested| condition_matches(nested, properties))),
        property => value.as_str().is_some_and(|accepted| property_matches(properties, property, accepted)),
    })
}

fn property_matches(properties: &HashMap<String, String>, name: &str, accepted: &str) -> bool {
    properties
        .get(name)
        .is_some_and(|value| accepted.split('|').any(|candidate| candidate == value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties(values: &[(&str, &str)]) -> HashMap<String, String> {
        values.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect()
    }

    #[test]
    fn resolves_variant_and_all_matching_multipart_models_deterministically() {
        let state: BlockstateFile = serde_json::from_str(
            r#"{
                "variants": {"facing=north": {"model":"minecraft:block/north", "y": 90}, "": {"model":"block/default"}},
                "multipart": [
                    {"when": {"north":"true|false"}, "apply": {"model":"block/post"}},
                    {"when": {"OR": [{"east":"true"}, {"west":"true"}]}, "apply": [{"model":"block/side_a", "weight": 1}, {"model":"block/side_b", "weight": 3}]}
                ]
            }"#,
        )
        .unwrap();
        let props = properties(&[("facing", "north"), ("north", "true"), ("east", "true")]);
        let first = resolve_models(&state, &props, (4, 70, -2));
        assert_eq!(first, resolve_models(&state, &props, (4, 70, -2)));
        assert_eq!(first[0].model, "north");
        assert_eq!(first[0].y, 90);
        assert_eq!(first[1].model, "post");
        assert_eq!(first.len(), 3);
    }

    #[test]
    fn multipart_conditions_support_and_or_and_property_alternatives() {
        let props = properties(&[("north", "true"), ("east", "false")]);
        let condition: Value = serde_json::from_str(r#"{"AND":[{"north":"true"},{"OR":[{"east":"true"},{"north":"false|true"}]}]}"#).unwrap();
        assert!(condition_matches(&condition, &props));
    }

    #[test]
    fn missing_legacy_properties_select_a_stable_variant() {
        let state: BlockstateFile = serde_json::from_str(
            r#"{"variants":{"snowy=true":{"model":"block/snowy"},"snowy=false":{"model":"block/plain"}}}"#,
        )
        .unwrap();
        assert_eq!(resolve_models(&state, &HashMap::new(), (0, 0, 0))[0].model, "plain");
    }
}
