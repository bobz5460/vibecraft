use std::path::{Path, PathBuf};

#[cfg(feature = "bundled")]
pub static EMBEDDED: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/embeds_minecraft");

#[derive(Clone, Debug)]
pub struct AssetReader {
    root: PathBuf,
}

impl AssetReader {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read(&self, rel_path: &str) -> Option<Vec<u8>> {
        #[cfg(feature = "bundled")]
        if let Some(file) = EMBEDDED.get_file(rel_path) {
            return Some(file.contents().to_vec());
        }
        std::fs::read(self.root.join(rel_path)).ok()
    }

    pub fn read_to_string(&self, rel_path: &str) -> Option<String> {
        #[cfg(feature = "bundled")]
        if let Some(file) = EMBEDDED.get_file(rel_path) {
            return String::from_utf8(file.contents().to_vec()).ok();
        }
        std::fs::read_to_string(self.root.join(rel_path)).ok()
    }

    pub fn exists(&self, rel_path: &str) -> bool {
        #[cfg(feature = "bundled")]
        if EMBEDDED.get_file(rel_path).is_some() {
            return true;
        }
        self.root.join(rel_path).exists()
    }

    pub fn read_image(&self, rel_path: &str) -> Option<image::DynamicImage> {
        let data = self.read(rel_path)?;
        image::load_from_memory(&data).ok()
    }

    pub fn read_dir(&self, rel_path: &str) -> Vec<String> {
        #[cfg(feature = "bundled")]
        {
            let mut names: Vec<String> = EMBEDDED
                .get_dir(rel_path)
                .into_iter()
                .flat_map(|d| d.files())
                .filter_map(|f| f.path().file_name())
                .map(|n| n.to_string_lossy().to_string())
                .collect();
            if !names.is_empty() {
                names.sort();
                return names;
            }
        }
        let dir_path = self.root.join(rel_path);
        let mut names: Vec<String> = std::fs::read_dir(&dir_path)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }
}
