use rodio::{Decoder, OutputStream, OutputStreamHandle, Source};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::assets::reader::AssetReader;

pub struct AudioEngine {
    stream_keep: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sounds: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    reader: AssetReader,
}

impl AudioEngine {
    pub fn new(reader: AssetReader) -> Self {
        let (stream, stream_handle) = OutputStream::try_default()
            .map(|(s, h)| (Some(s), Some(h)))
            .unwrap_or_else(|e| {
                log::warn!("Failed to create audio stream: {}. Audio disabled.", e);
                (None, None)
            });

        AudioEngine {
            stream_keep: stream,
            stream_handle,
            sounds: Mutex::new(HashMap::new()),
            reader,
        }
    }

    /// Load a sound file from the minecraft-assets sounds directory
    pub fn load_sound(&self, path: &str) {
        let Some(data) = self.reader.read(path) else {
            return;
        };
        let mut sounds = self.sounds.lock().unwrap();
        sounds.insert(path.to_string(), Arc::new(data));
    }

    /// Play a sound at a given volume (0.0 - 1.0)
    pub fn play(&self, path: &str, volume: f32) {
        let Some(ref handle) = self.stream_handle else { return };
        let data = {
            let mut sounds = self.sounds.lock().unwrap();
            if !sounds.contains_key(path) {
                if let Some(d) = self.reader.read(path) {
                    sounds.insert(path.to_string(), Arc::new(d));
                }
            }
            sounds.get(path).cloned()
        };
        if let Some(data) = data {
            let cursor = std::io::Cursor::new(data.to_vec());
            if let Ok(source) = Decoder::new(cursor) {
                let source = source.amplify(volume);
                if let Err(e) = handle.play_raw(source.convert_samples::<f32>()) {
                    log::warn!("Failed to play sound {}: {}", path, e);
                }
            }
        }
    }

    /// Play a block sound from the minecraft sounds directory
    pub fn play_block_sound(&self, block_type: &str, action: &str) {
        let templates = [
            format!("sounds/block/{}/{}.ogg", block_type, action),
            format!("sounds/dig/{}.ogg", block_type),
            format!("sounds/step/{}.ogg", block_type),
            format!("sounds/random/{}.ogg", action),
        ];
        for path in &templates {
            if self.reader.exists(path) {
                self.load_sound(path);
                self.play(path, 0.5);
                return;
            }
        }
    }

    /// Load a batch of sound files for common game events
    pub fn load_common_sounds(&self) {
        // UI sounds
        for s in &[
            "sounds/ui/button/click.ogg",
            "sounds/ui/button/click_off.ogg",
            "sounds/ui/toast/in.ogg",
            "sounds/ui/toast/out.ogg",
        ] {
            self.load_sound(s);
        }
        // Block sounds (a few common types)
        for block in &["stone", "wood", "grass", "gravel", "sand", "glass", "metal"] {
            for action in &["break", "step", "hit", "place"] {
                let path = format!("sounds/block/{}/{}.ogg", block, action);
                if self.reader.exists(&path) {
                    self.load_sound(&path);
                }
            }
        }
        // Player hurt
        for s in &[
            "sounds/damage/hurt1.ogg",
            "sounds/damage/hurt2.ogg",
            "sounds/damage/hurt3.ogg",
        ] {
            self.load_sound(s);
        }
        // Ambient cave sounds
        for i in 1..=19 {
            let path = format!("sounds/ambient/cave/cave{}.ogg", i);
            if self.reader.exists(&path) {
                self.load_sound(&path);
            }
        }
    }
}
