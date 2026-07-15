//! Chat editing and history rules shared by the windowed UI.

pub const MAX_CHAT_CHARACTERS: usize = 256;
pub const MAX_CHAT_MESSAGES: usize = 100;

#[derive(Clone, Debug, Default)]
pub struct ChatState {
    pub open: bool,
    pub text: String,
    cursor: usize,
    selection_start: Option<usize>,
    messages: Vec<String>,
    sent_entries: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    scroll_offset: usize,
    pub unread_messages: usize,
}

impl ChatState {
    pub fn open_with(&mut self, text: impl Into<String>) {
        self.open = true;
        self.text = text.into();
        self.cursor = self.text.chars().count();
        self.selection_start = None;
        self.history_index = None;
        self.history_draft.clear();
        self.scroll_offset = 0;
        self.unread_messages = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.text.clear();
        self.cursor = 0;
        self.selection_start = None;
        self.history_index = None;
        self.history_draft.clear();
        self.scroll_offset = 0;
        self.unread_messages = 0;
    }

    pub fn add_message(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
        if self.messages.len() > MAX_CHAT_MESSAGES {
            self.messages.remove(0);
        }
        if self.scroll_offset > 0 {
            self.unread_messages = self.unread_messages.saturating_add(1);
        }
    }

    pub fn insert(&mut self, value: &str) {
        if self.selection_start.is_some() {
            self.delete_selection();
        }
        let available = MAX_CHAT_CHARACTERS.saturating_sub(self.text.chars().count());
        let inserted: String = value.chars().filter(|character| !character.is_control()).take(available).collect();
        if inserted.is_empty() {
            return;
        }
        let byte_index = byte_index_at(&self.text, self.cursor);
        self.text.insert_str(byte_index, &inserted);
        self.cursor += inserted.chars().count();
        self.selection_start = None;
    }

    pub fn backspace(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let start = byte_index_at(&self.text, self.cursor - 1);
        let end = byte_index_at(&self.text, self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
        self.selection_start = None;
    }

    pub fn delete(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor >= self.text.chars().count() {
            return;
        }
        let start = byte_index_at(&self.text, self.cursor);
        let end = byte_index_at(&self.text, self.cursor + 1);
        self.text.replace_range(start..end, "");
        self.selection_start = None;
    }

    pub fn move_cursor(&mut self, delta: i32, extend_selection: bool) {
        let length = self.text.chars().count() as i32;
        if !extend_selection {
            self.selection_start = None;
        } else if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        }
        self.cursor = (self.cursor as i32 + delta).clamp(0, length) as usize;
    }

    pub fn move_to_start(&mut self, extend_selection: bool) {
        if !extend_selection {
            self.selection_start = None;
        } else if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        }
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self, extend_selection: bool) {
        if !extend_selection {
            self.selection_start = None;
        } else if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        }
        self.cursor = self.text.chars().count();
    }

    pub fn recall_previous(&mut self) {
        if self.sent_entries.is_empty() {
            return;
        }
        let next = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.history_draft = self.text.clone();
                self.sent_entries.len() - 1
            }
        };
        self.history_index = Some(next);
        self.text = self.sent_entries[next].clone();
        self.move_to_end(false);
    }

    pub fn recall_next(&mut self) {
        let Some(index) = self.history_index else { return; };
        if index + 1 < self.sent_entries.len() {
            let next = index + 1;
            self.history_index = Some(next);
            self.text = self.sent_entries[next].clone();
        } else {
            self.history_index = None;
            self.text = self.history_draft.clone();
        }
        self.move_to_end(false);
    }

    pub fn complete(&mut self, candidates: &[String]) -> bool {
        let Some(candidate) = candidates.first() else { return false; };
        let cursor_byte = byte_index_at(&self.text, self.cursor);
        let prefix_start = self.text[..cursor_byte]
            .rfind(char::is_whitespace)
            .map_or(0, |index| index + 1);
        // Command suggestions omit the leading slash, which must remain so Enter
        // dispatches the completed input as a command instead of chat.
        let replacement_start = prefix_start
            + usize::from(self.text[prefix_start..cursor_byte].starts_with('/'));
        self.text.replace_range(replacement_start..cursor_byte, candidate);
        self.cursor = self.text[..replacement_start].chars().count() + candidate.chars().count();
        true
    }

    pub fn submit(&mut self) -> Option<String> {
        let submitted = self.text.trim().to_string();
        if submitted.is_empty() {
            return None;
        }
        if self.sent_entries.last() != Some(&submitted) {
            self.sent_entries.push(submitted.clone());
        }
        self.history_index = None;
        self.history_draft.clear();
        Some(submitted)
    }

    pub fn scroll(&mut self, amount: i32) {
        if amount > 0 {
            self.scroll_offset = (self.scroll_offset + amount as usize).min(self.messages.len());
        } else if amount < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(amount.unsigned_abs() as usize);
        }
        if self.scroll_offset == 0 {
            self.unread_messages = 0;
        }
    }

    pub fn visible_messages(&self, count: usize) -> &[String] {
        let end = self.messages.len().saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(count);
        &self.messages[start..end]
    }

    pub fn recent_messages(&self, count: usize) -> &[String] {
        let start = self.messages.len().saturating_sub(count);
        &self.messages[start..]
    }

    pub fn selected_text(&self) -> &str {
        let Some(sel) = self.selection_start else { return ""; };
        let start = sel.min(self.cursor);
        let end = sel.max(self.cursor);
        let byte_start = byte_index_at(&self.text, start);
        let byte_end = byte_index_at(&self.text, end);
        &self.text[byte_start..byte_end]
    }

    pub fn delete_selection(&mut self) {
        let Some(sel) = self.selection_start else { return; };
        let start = sel.min(self.cursor);
        let end = sel.max(self.cursor);
        let byte_start = byte_index_at(&self.text, start);
        let byte_end = byte_index_at(&self.text, end);
        self.text.replace_range(byte_start..byte_end, "");
        self.cursor = start;
        self.selection_start = None;
    }

    pub fn cursor_char_index(&self) -> usize {
        self.cursor
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_start.map(|sel| (sel.min(self.cursor), sel.max(self.cursor)))
    }

    pub fn display_text(&self) -> &str {
        &self.text
    }
}

pub fn command_suggestions(input: &str) -> Vec<String> {
    let command_names = [
        "clear", "difficulty", "effect", "experience", "fill", "gamemode", "gamerule", "give",
        "help", "kill", "seed", "setblock", "setworldspawn", "teleport", "time",
        "tp", "weather", "xp",
    ];
    let input = input.strip_prefix('/').unwrap_or(input);
    let words: Vec<&str> = input.split_whitespace().collect();
    let trailing_space = input.ends_with(char::is_whitespace);
    if words.is_empty() || (!trailing_space && words.len() == 1) {
        let prefix = words.first().copied().unwrap_or("");
        return matches(prefix, command_names.iter().copied());
    }
    let command = words[0];
    let argument_index = words.len() - 1 + usize::from(trailing_space);
    let prefix = if trailing_space { "" } else { words.last().copied().unwrap_or("") };
    let options: &[&str] = match (command, argument_index) {
        ("gamemode", 1) => &["survival", "creative", "adventure", "spectator"],
        ("difficulty", 1) => &["peaceful", "easy", "normal", "hard"],
        ("weather", 1) => &["clear", "rain", "thunder"],
        ("gamerule", 1) => &["doDaylightCycle", "keepInventory"],
        ("gamerule", 2) => &["true", "false"],
        ("time", 1) => &["set", "query"],
        ("time", 2) if words.get(1) == Some(&"set") => &["day", "noon", "night", "midnight"],
        ("time", 2) if words.get(1) == Some(&"query") => &["daytime", "gametime"],
        ("effect", 1) => &["clear", "speed", "slowness", "haste", "strength", "regeneration", "resistance", "poison", "wither"],
        ("fill", 7) | ("setblock", 4) | ("give", 1) => &["air", "stone", "dirt", "grass_block", "cobblestone", "oak_planks", "glass", "torch", "water", "lava"],
        _ => &[],
    };
    matches(prefix, options.iter().copied())
}

fn matches<'a>(prefix: &str, values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    values
        .filter(|value| value.to_ascii_lowercase().starts_with(&prefix))
        .map(str::to_string)
        .collect()
}

fn byte_index_at(value: &str, character_index: usize) -> usize {
    value
        .char_indices()
        .nth(character_index)
        .map_or(value.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_limits_input_and_edits_at_the_cursor() {
        let mut chat = ChatState::default();
        chat.open_with("ab");
        chat.move_cursor(-1, false);
        chat.insert("X");
        assert_eq!(chat.text, "aXb");
        chat.backspace();
        assert_eq!(chat.text, "ab");
        chat.insert(&"z".repeat(MAX_CHAT_CHARACTERS));
        assert_eq!(chat.text.chars().count(), MAX_CHAT_CHARACTERS);
    }

    #[test]
    fn history_recall_restores_the_unsent_draft() {
        let mut chat = ChatState::default();
        chat.open_with("first");
        chat.submit();
        chat.open_with("second");
        chat.submit();
        chat.open_with("draft");
        chat.recall_previous();
        assert_eq!(chat.text, "second");
        chat.recall_next();
        assert_eq!(chat.text, "draft");
    }

    #[test]
    fn scroll_retains_unread_messages_until_returning_to_bottom() {
        let mut chat = ChatState::default();
        chat.add_message("one");
        chat.scroll(1);
        chat.add_message("two");
        assert_eq!(chat.unread_messages, 1);
        chat.scroll(-1);
        assert_eq!(chat.unread_messages, 0);
    }

    #[test]
    fn command_completion_is_contextual() {
        assert_eq!(command_suggestions("/gam"), vec!["gamemode", "gamerule"]);
        assert_eq!(command_suggestions("/time set n"), vec!["noon", "night"]);
    }

    #[test]
    fn command_completion_retains_the_leading_slash() {
        let mut chat = ChatState::default();
        chat.open_with("/gam");

        assert!(chat.complete(&command_suggestions(&chat.text)));
        assert_eq!(chat.text, "/gamemode");
    }
}
