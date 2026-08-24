use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{Document, Item, Topic};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Editing(Editor),
    Searching(Editor),
    ConfirmDelete(DeleteTarget),
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Editor {
    pub kind: EditKind,
    pub input: String,
    pub cursor: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditKind {
    NewTopic,
    RenameTopic(usize),
    AddItem(usize),
    EditItem(usize, usize),
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteTarget {
    Topic(usize),
    Item(usize, usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandleResult {
    pub quit: bool,
    pub changed: bool,
}

#[derive(Clone, Debug)]
pub struct App {
    pub document: Document,
    pub selected_topic: usize,
    pub selected_item: Option<usize>,
    pub mode: Mode,
    pub query: String,
    pub status: Option<String>,
    pub scroll: u16,
}

impl App {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            selected_topic: 0,
            selected_item: None,
            mode: Mode::Normal,
            query: String::new(),
            status: None,
            scroll: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> HandleResult {
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Editing(mut editor) => {
                if key.code == KeyCode::Esc {
                    return HandleResult::default();
                }
                if key.code == KeyCode::Enter {
                    let changed = self.commit_editor(&editor);
                    if !changed {
                        self.mode = Mode::Editing(editor);
                    }
                    return HandleResult {
                        changed,
                        ..HandleResult::default()
                    };
                }
                edit_text(&mut editor, key);
                self.mode = Mode::Editing(editor);
                HandleResult::default()
            }
            Mode::Searching(mut editor) => {
                match key.code {
                    KeyCode::Esc => {
                        self.query.clear();
                        self.ensure_visible_selection();
                    }
                    KeyCode::Enter => {}
                    _ => {
                        edit_text(&mut editor, key);
                        self.query.clone_from(&editor.input);
                        self.ensure_visible_selection();
                        self.mode = Mode::Searching(editor);
                    }
                }
                HandleResult::default()
            }
            Mode::ConfirmDelete(target) => match key.code {
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                    self.delete(target);
                    HandleResult {
                        changed: true,
                        ..HandleResult::default()
                    }
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => HandleResult::default(),
                _ => {
                    self.mode = Mode::ConfirmDelete(target);
                    HandleResult::default()
                }
            },
            Mode::Help => {
                if !matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
                ) {
                    self.mode = Mode::Help;
                }
                HandleResult::default()
            }
        }
    }

    pub fn visible_topics(&self) -> Vec<usize> {
        if self.query.trim().is_empty() {
            return (0..self.document.topics.len()).collect();
        }
        (0..self.document.topics.len())
            .filter(|&index| {
                let topic = &self.document.topics[index];
                contains_case_insensitive(&topic.name, &self.query)
                    || topic
                        .items
                        .iter()
                        .any(|item| contains_case_insensitive(&item.text, &self.query))
            })
            .collect()
    }

    pub fn visible_items(&self, topic_index: usize) -> Vec<usize> {
        let Some(topic) = self.document.topics.get(topic_index) else {
            return Vec::new();
        };
        if self.query.trim().is_empty() || contains_case_insensitive(&topic.name, &self.query) {
            return (0..topic.items.len()).collect();
        }
        topic
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                contains_case_insensitive(&item.text, &self.query).then_some(index)
            })
            .collect()
    }

    pub fn selected_topic(&self) -> Option<&Topic> {
        self.document.topics.get(self.selected_topic)
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    fn handle_normal(&mut self, key: KeyEvent) -> HandleResult {
        self.mode = Mode::Normal;
        self.status = None;

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let changed = match key.code {
                KeyCode::Up => self.move_item(-1),
                KeyCode::Down => self.move_item(1),
                KeyCode::Left => self.move_topic(-1),
                KeyCode::Right => self.move_topic(1),
                _ => false,
            };
            return HandleResult {
                changed,
                ..HandleResult::default()
            };
        }

        match key.code {
            KeyCode::Char('q') => HandleResult {
                quit: true,
                ..HandleResult::default()
            },
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                HandleResult::default()
            }
            KeyCode::Char('/') => {
                let input = self.query.clone();
                self.mode = Mode::Searching(Editor {
                    kind: EditKind::Search,
                    cursor: input.chars().count(),
                    input,
                });
                HandleResult::default()
            }
            KeyCode::Char('t') => {
                self.mode = Mode::Editing(Editor {
                    kind: EditKind::NewTopic,
                    input: String::new(),
                    cursor: 0,
                });
                HandleResult::default()
            }
            KeyCode::Char('a') if self.selected_topic().is_some() => {
                self.mode = Mode::Editing(Editor {
                    kind: EditKind::AddItem(self.selected_topic),
                    input: String::new(),
                    cursor: 0,
                });
                HandleResult::default()
            }
            KeyCode::Enter => {
                if let Some(item) = self.selected_item {
                    if let Some(value) = self
                        .document
                        .topics
                        .get(self.selected_topic)
                        .and_then(|topic| topic.items.get(item))
                    {
                        let input = value.text.clone();
                        self.mode = Mode::Editing(Editor {
                            kind: EditKind::EditItem(self.selected_topic, item),
                            cursor: input.chars().count(),
                            input,
                        });
                    }
                } else if let Some(topic) = self.selected_topic() {
                    let input = topic.name.clone();
                    self.mode = Mode::Editing(Editor {
                        kind: EditKind::RenameTopic(self.selected_topic),
                        cursor: input.chars().count(),
                        input,
                    });
                }
                HandleResult::default()
            }
            KeyCode::Char(' ') => {
                let changed = self.toggle_selected();
                HandleResult {
                    changed,
                    ..HandleResult::default()
                }
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if self.selected_topic().is_some() {
                    let target = self
                        .selected_item
                        .map(|item| DeleteTarget::Item(self.selected_topic, item))
                        .unwrap_or(DeleteTarget::Topic(self.selected_topic));
                    self.mode = Mode::ConfirmDelete(target);
                }
                HandleResult::default()
            }
            KeyCode::Up => {
                self.select_up();
                HandleResult::default()
            }
            KeyCode::Down => {
                self.select_down();
                HandleResult::default()
            }
            KeyCode::Left | KeyCode::BackTab => {
                self.select_topic(-1);
                HandleResult::default()
            }
            KeyCode::Right | KeyCode::Tab => {
                self.select_topic(1);
                HandleResult::default()
            }
            KeyCode::Esc => {
                self.query.clear();
                self.ensure_visible_selection();
                HandleResult::default()
            }
            _ => HandleResult::default(),
        }
    }

    fn commit_editor(&mut self, editor: &Editor) -> bool {
        let value = editor.input.trim();
        if value.is_empty() {
            self.status = Some("Text cannot be empty".into());
            return false;
        }

        match editor.kind {
            EditKind::NewTopic => {
                self.document.topics.push(Topic {
                    name: value.into(),
                    items: Vec::new(),
                });
                self.selected_topic = self.document.topics.len() - 1;
                self.selected_item = None;
            }
            EditKind::RenameTopic(topic) => {
                let Some(target) = self.document.topics.get_mut(topic) else {
                    return false;
                };
                target.name = value.into();
            }
            EditKind::AddItem(topic) => {
                let Some(target) = self.document.topics.get_mut(topic) else {
                    return false;
                };
                target.items.push(Item {
                    text: value.into(),
                    done: false,
                });
                self.selected_topic = topic;
                self.selected_item = Some(target.items.len() - 1);
            }
            EditKind::EditItem(topic, item) => {
                let Some(target) = self
                    .document
                    .topics
                    .get_mut(topic)
                    .and_then(|topic| topic.items.get_mut(item))
                else {
                    return false;
                };
                target.text = value.into();
            }
            EditKind::Search => return false,
        }
        self.query.clear();
        self.status = Some("Saved".into());
        true
    }

    fn toggle_selected(&mut self) -> bool {
        let Some(item_index) = self.selected_item else {
            return false;
        };
        let Some(item) = self
            .document
            .topics
            .get_mut(self.selected_topic)
            .and_then(|topic| topic.items.get_mut(item_index))
        else {
            return false;
        };
        item.done = !item.done;
        self.status = Some(if item.done { "Checked" } else { "Unchecked" }.into());
        true
    }

    fn delete(&mut self, target: DeleteTarget) {
        match target {
            DeleteTarget::Topic(topic) if topic < self.document.topics.len() => {
                self.document.topics.remove(topic);
                self.selected_topic = topic.min(self.document.topics.len().saturating_sub(1));
                self.selected_item = None;
                self.status = Some("Topic deleted".into());
            }
            DeleteTarget::Item(topic, item)
                if self
                    .document
                    .topics
                    .get(topic)
                    .is_some_and(|topic| item < topic.items.len()) =>
            {
                let items = &mut self.document.topics[topic].items;
                items.remove(item);
                self.selected_topic = topic;
                self.selected_item = if items.is_empty() {
                    None
                } else {
                    Some(item.min(items.len() - 1))
                };
                self.status = Some("Entry deleted".into());
            }
            _ => {}
        }
        self.ensure_visible_selection();
    }

    fn select_up(&mut self) {
        if let Some(item) = self.selected_item {
            let visible = self.visible_items(self.selected_topic);
            let position = visible.iter().position(|&index| index == item).unwrap_or(0);
            self.selected_item = position.checked_sub(1).map(|position| visible[position]);
        }
    }

    fn select_down(&mut self) {
        let visible = self.visible_items(self.selected_topic);
        self.selected_item = match self.selected_item {
            None => visible.first().copied(),
            Some(item) => visible
                .iter()
                .position(|&index| index == item)
                .and_then(|position| visible.get(position + 1).copied())
                .or(Some(item)),
        };
    }

    fn select_topic(&mut self, direction: isize) {
        let visible = self.visible_topics();
        if visible.is_empty() {
            return;
        }
        let position = visible
            .iter()
            .position(|&index| index == self.selected_topic)
            .unwrap_or(0);
        let next = if direction < 0 {
            position.saturating_sub(1)
        } else {
            (position + 1).min(visible.len() - 1)
        };
        self.selected_topic = visible[next];
        self.selected_item = None;
    }

    fn move_item(&mut self, direction: isize) -> bool {
        let Some(index) = self.selected_item else {
            return false;
        };
        let Some(topic) = self.document.topics.get_mut(self.selected_topic) else {
            return false;
        };
        let destination = if direction < 0 {
            index.checked_sub(1)
        } else {
            (index + 1 < topic.items.len()).then_some(index + 1)
        };
        let Some(destination) = destination else {
            return false;
        };
        topic.items.swap(index, destination);
        self.selected_item = Some(destination);
        self.query.clear();
        true
    }

    fn move_topic(&mut self, direction: isize) -> bool {
        if self.document.topics.is_empty() {
            return false;
        }
        let destination = if direction < 0 {
            self.selected_topic.checked_sub(1)
        } else {
            (self.selected_topic + 1 < self.document.topics.len())
                .then_some(self.selected_topic + 1)
        };
        let Some(destination) = destination else {
            return false;
        };
        self.document.topics.swap(self.selected_topic, destination);
        self.selected_topic = destination;
        self.query.clear();
        true
    }

    fn ensure_visible_selection(&mut self) {
        let visible_topics = self.visible_topics();
        if visible_topics.is_empty() {
            self.selected_item = None;
            return;
        }
        if !visible_topics.contains(&self.selected_topic) {
            self.selected_topic = visible_topics[0];
            self.selected_item = None;
        }
        if let Some(item) = self.selected_item
            && !self.visible_items(self.selected_topic).contains(&item)
        {
            self.selected_item = None;
        }
    }
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&needle.trim().to_lowercase())
}

fn edit_text(editor: &mut Editor, key: KeyEvent) {
    match key.code {
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            let byte = char_to_byte(&editor.input, editor.cursor);
            editor.input.insert(byte, character);
            editor.cursor += 1;
        }
        KeyCode::Backspace if editor.cursor > 0 => {
            let start = char_to_byte(&editor.input, editor.cursor - 1);
            let end = char_to_byte(&editor.input, editor.cursor);
            editor.input.replace_range(start..end, "");
            editor.cursor -= 1;
        }
        KeyCode::Delete if editor.cursor < editor.input.chars().count() => {
            let start = char_to_byte(&editor.input, editor.cursor);
            let end = char_to_byte(&editor.input, editor.cursor + 1);
            editor.input.replace_range(start..end, "");
        }
        KeyCode::Left => editor.cursor = editor.cursor.saturating_sub(1),
        KeyCode::Right => editor.cursor = (editor.cursor + 1).min(editor.input.chars().count()),
        KeyCode::Home => editor.cursor = 0,
        KeyCode::End => editor.cursor = editor.input.chars().count(),
        _ => {}
    }
}

fn char_to_byte(value: &str, character_index: usize) -> usize {
    value
        .char_indices()
        .nth(character_index)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DATA_VERSION;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn app() -> App {
        App::new(Document {
            version: DATA_VERSION,
            topics: vec![
                Topic {
                    name: "Developer".into(),
                    items: vec![
                        Item {
                            text: "Rust".into(),
                            done: false,
                        },
                        Item {
                            text: "Interfaces".into(),
                            done: false,
                        },
                    ],
                },
                Topic {
                    name: "Mountaineer".into(),
                    items: vec![Item {
                        text: "Alps".into(),
                        done: false,
                    }],
                },
            ],
        })
    }

    #[test]
    fn navigates_and_toggles_an_item() {
        let mut app = app();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_item, Some(0));
        let result = app.handle_key(key(KeyCode::Char(' ')));
        assert!(result.changed);
        assert!(app.document.topics[0].items[0].done);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_item, None);
    }

    #[test]
    fn creates_edits_and_deletes_content() {
        let mut app = app();
        app.handle_key(key(KeyCode::Char('t')));
        for character in "Writer".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        assert!(app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(app.document.topics.last().unwrap().name, "Writer");

        app.handle_key(key(KeyCode::Char('a')));
        for character in "Essay".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.document.topics.last().unwrap().items[0].text, "Essay");

        app.handle_key(key(KeyCode::Delete));
        assert!(matches!(
            app.mode,
            Mode::ConfirmDelete(DeleteTarget::Item(_, _))
        ));
        assert!(app.handle_key(key(KeyCode::Char('y'))).changed);
        assert!(app.document.topics.last().unwrap().items.is_empty());
    }

    #[test]
    fn filters_topics_and_items_case_insensitively() {
        let mut app = app();
        app.query = "ALP".into();
        assert_eq!(app.visible_topics(), vec![1]);
        assert_eq!(app.visible_items(1), vec![0]);

        app.query = "developer".into();
        assert_eq!(app.visible_topics(), vec![0]);
        assert_eq!(app.visible_items(0), vec![0, 1]);
    }

    #[test]
    fn reorders_items_and_topics() {
        let mut app = app();
        app.selected_item = Some(0);
        assert!(app.handle_key(ctrl(KeyCode::Down)).changed);
        assert_eq!(app.document.topics[0].items[1].text, "Rust");
        assert!(app.handle_key(ctrl(KeyCode::Right)).changed);
        assert_eq!(app.document.topics[1].name, "Developer");
    }

    #[test]
    fn editor_handles_unicode_by_character() {
        let mut editor = Editor {
            kind: EditKind::Search,
            input: "é界".into(),
            cursor: 2,
        };
        edit_text(&mut editor, key(KeyCode::Backspace));
        assert_eq!(editor.input, "é");
        edit_text(&mut editor, key(KeyCode::Home));
        edit_text(&mut editor, key(KeyCode::Char('✓')));
        assert_eq!(editor.input, "✓é");
    }
}
