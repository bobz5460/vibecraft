use std::collections::HashMap;
use crate::assets::reader::AssetReader;

pub struct Language {
    entries: HashMap<String, String>,
}

impl Language {
    pub fn new(reader: &AssetReader) -> Self {
        let entries = reader
            .read_to_string("lang/en_us.json")
            .and_then(|content| serde_json::from_str::<HashMap<String, String>>(&content).ok())
            .unwrap_or_default();

        Language { entries }
    }

    pub fn translate<'a>(&'a self, key: &'a str) -> &'a str {
        self.entries.get(key).map(|s| s.as_str()).unwrap_or(key)
    }
}
