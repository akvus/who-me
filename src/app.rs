use chrono::{Datelike, Days, Local, Months, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    model::{
        Characteristic, Document, IdentityStatus, Item, Judgement, MoodRating, Observation,
        Sentiment, Topic,
    },
    sync::{ConflictChoice, SyncStatus, validate_github_url},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Editing(Editor),
    Searching(Editor),
    SelectingStatus(StatusPicker),
    SelectingMood(MoodPicker),
    EditingJudgement(JudgementForm),
    EditingCharacteristic(CharacteristicForm),
    ConfirmDelete(DeleteTarget),
    Settings(SettingsState),
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsState {
    pub repository: String,
    pub cursor: usize,
    pub editing: bool,
    pub confirm_disconnect: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusPicker {
    pub topic: usize,
    pub selected: IdentityStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoodPicker {
    pub date: NaiveDate,
    pub selected: MoodRating,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JudgementFocus {
    #[default]
    Judgements,
    Characteristics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JudgementField {
    #[default]
    Name,
    FollowUp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JudgementForm {
    pub target: Option<usize>,
    pub name: String,
    pub name_cursor: usize,
    pub follow_up: String,
    pub follow_up_cursor: usize,
    pub field: JudgementField,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CharacteristicField {
    #[default]
    Name,
    BeforeText,
    BeforeRating,
    AfterText,
    AfterRating,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacteristicForm {
    pub judgement: usize,
    pub target: Option<usize>,
    pub name: String,
    pub name_cursor: usize,
    pub before_text: String,
    pub before_cursor: usize,
    pub before_rating: Sentiment,
    pub after_text: String,
    pub after_cursor: usize,
    pub after_rating: Sentiment,
    pub has_after: bool,
    pub field: CharacteristicField,
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
    AddCalendarEntry(NaiveDate),
    EditCalendarEntry(NaiveDate, usize),
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteTarget {
    Topic(usize),
    Item(usize, usize),
    CalendarEntry(NaiveDate, usize),
    Judgement(usize),
    Characteristic(usize, usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Feature {
    #[default]
    Identities,
    Calendar,
    Statistics,
    Judgements,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatisticsPeriod {
    #[default]
    Month,
    Year,
    Forever,
}

impl StatisticsPeriod {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Month => "Last 30 days",
            Self::Year => "Last 365 days",
            Self::Forever => "Forever",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoodStatistics {
    pub counts: [usize; 5],
    pub rated_days: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JudgementStatistics {
    pub before_counts: [usize; 3],
    pub after_counts: [usize; 3],
    pub characteristics: usize,
    pub verified: usize,
}

impl MoodStatistics {
    pub fn average(self) -> Option<f64> {
        (self.rated_days > 0).then_some(self.total as f64 / self.rated_days as f64)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarFocus {
    #[default]
    Grid,
    Entries,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncAction {
    Configure(String),
    Synchronize,
    Resolve(ConflictChoice),
    Disconnect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandleResult {
    pub quit: bool,
    pub changed: bool,
    pub sync_action: Option<SyncAction>,
}

#[derive(Clone, Debug)]
pub struct App {
    pub document: Document,
    pub feature: Feature,
    pub selected_topic: usize,
    pub selected_item: Option<usize>,
    pub mode: Mode,
    pub query: String,
    pub status: Option<String>,
    pub scroll: u16,
    pub today: NaiveDate,
    pub displayed_month: NaiveDate,
    pub selected_date: NaiveDate,
    pub selected_calendar_entry: Option<usize>,
    pub calendar_focus: CalendarFocus,
    pub calendar_scroll: u16,
    pub statistics_period: StatisticsPeriod,
    pub selected_judgement: usize,
    pub selected_characteristic: Option<usize>,
    pub judgement_focus: JudgementFocus,
    pub judgement_scroll: u16,
    pub characteristic_scroll: u16,
    pub sync_status: SyncStatus,
    pub sync_repository: Option<String>,
    pub sync_branch: Option<String>,
}

impl App {
    pub fn new(document: Document) -> Self {
        Self::new_at(document, Local::now().date_naive())
    }

    pub fn new_at(document: Document, today: NaiveDate) -> Self {
        let displayed_month = today.with_day(1).expect("day one is always valid");
        Self {
            document,
            feature: Feature::Identities,
            selected_topic: 0,
            selected_item: None,
            mode: Mode::Normal,
            query: String::new(),
            status: None,
            scroll: 0,
            today,
            displayed_month,
            selected_date: today,
            selected_calendar_entry: None,
            calendar_focus: CalendarFocus::Grid,
            calendar_scroll: 0,
            statistics_period: StatisticsPeriod::Month,
            selected_judgement: 0,
            selected_characteristic: None,
            judgement_focus: JudgementFocus::Judgements,
            judgement_scroll: 0,
            characteristic_scroll: 0,
            sync_status: SyncStatus::LocalOnly,
            sync_repository: None,
            sync_branch: None,
        }
    }

    pub fn refresh_today(&mut self) {
        self.update_today(Local::now().date_naive());
    }

    fn update_today(&mut self, today: NaiveDate) {
        self.today = today;
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
            Mode::SelectingStatus(mut picker) => match key.code {
                KeyCode::Esc => HandleResult::default(),
                KeyCode::Enter => self.commit_status(picker),
                KeyCode::Up => {
                    picker.selected = adjacent_status(picker.selected, -1);
                    self.mode = Mode::SelectingStatus(picker);
                    HandleResult::default()
                }
                KeyCode::Down => {
                    picker.selected = adjacent_status(picker.selected, 1);
                    self.mode = Mode::SelectingStatus(picker);
                    HandleResult::default()
                }
                _ => {
                    self.mode = Mode::SelectingStatus(picker);
                    HandleResult::default()
                }
            },
            Mode::SelectingMood(mut picker) => match key.code {
                KeyCode::Esc => HandleResult::default(),
                KeyCode::Enter => self.commit_mood(picker.date, Some(picker.selected)),
                KeyCode::Char('c' | 'C') => self.commit_mood(picker.date, None),
                KeyCode::Char(character @ '1'..='5') => {
                    let rating = MoodRating::try_from(character as u8 - b'0')
                        .expect("matched mood rating range");
                    self.commit_mood(picker.date, Some(rating))
                }
                KeyCode::Up | KeyCode::Left => {
                    picker.selected = adjacent_mood(picker.selected, -1);
                    self.mode = Mode::SelectingMood(picker);
                    HandleResult::default()
                }
                KeyCode::Down | KeyCode::Right => {
                    picker.selected = adjacent_mood(picker.selected, 1);
                    self.mode = Mode::SelectingMood(picker);
                    HandleResult::default()
                }
                _ => {
                    self.mode = Mode::SelectingMood(picker);
                    HandleResult::default()
                }
            },
            Mode::EditingJudgement(mut form) => self.handle_judgement_form(key, &mut form),
            Mode::EditingCharacteristic(mut form) => {
                self.handle_characteristic_form(key, &mut form)
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
            Mode::Settings(mut settings) => self.handle_settings(key, &mut settings),
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

    pub fn selected_calendar_entries(&self) -> &[Item] {
        self.document
            .calendar_day(self.selected_date)
            .map(|day| day.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn mood_statistics(&self) -> MoodStatistics {
        let start = match self.statistics_period {
            StatisticsPeriod::Month => self.today.checked_sub_days(Days::new(29)),
            StatisticsPeriod::Year => self.today.checked_sub_days(Days::new(364)),
            StatisticsPeriod::Forever => None,
        };
        let mut statistics = MoodStatistics::default();
        for day in &self.document.calendar.days {
            if day.date > self.today || start.is_some_and(|start| day.date < start) {
                continue;
            }
            let Some(mood) = day.mood else {
                continue;
            };
            statistics.counts[mood.value() as usize - 1] += 1;
            statistics.rated_days += 1;
            statistics.total += mood.value() as usize;
        }
        statistics
    }

    pub fn selected_judgement(&self) -> Option<&Judgement> {
        self.document.judgements.get(self.selected_judgement)
    }

    pub fn selected_characteristic(&self) -> Option<&Characteristic> {
        self.selected_characteristic.and_then(|characteristic| {
            self.selected_judgement()
                .and_then(|judgement| judgement.characteristics.get(characteristic))
        })
    }

    pub fn judgement_statistics(&self) -> JudgementStatistics {
        let Some(judgement) = self.selected_judgement() else {
            return JudgementStatistics::default();
        };
        let mut statistics = JudgementStatistics {
            characteristics: judgement.characteristics.len(),
            ..JudgementStatistics::default()
        };
        for characteristic in &judgement.characteristics {
            statistics.before_counts[sentiment_index(characteristic.before.rating)] += 1;
            if let Some(after) = &characteristic.after {
                statistics.after_counts[sentiment_index(after.rating)] += 1;
                statistics.verified += 1;
            }
        }
        statistics
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    pub fn set_sync_status(&mut self, status: SyncStatus) {
        self.sync_status = status;
    }

    pub fn set_sync_repository(&mut self, repository: Option<String>) {
        self.sync_repository = repository;
    }

    pub fn set_sync_branch(&mut self, branch: Option<String>) {
        self.sync_branch = branch;
    }

    pub fn apply_document(&mut self, document: Document) {
        self.document = document;
        self.selected_topic = self
            .selected_topic
            .min(self.document.topics.len().saturating_sub(1));
        if self.document.topics.is_empty() {
            self.selected_item = None;
        } else if let Some(item) = self.selected_item
            && item >= self.document.topics[self.selected_topic].items.len()
        {
            self.selected_item = None;
        }
        self.ensure_visible_selection();
        self.ensure_calendar_selection();
        self.ensure_judgement_selection();
        self.status = Some("Updated from GitHub".into());
    }

    fn handle_normal(&mut self, key: KeyEvent) -> HandleResult {
        self.mode = Mode::Normal;
        self.status = None;

        match key.code {
            KeyCode::Char('q') => {
                return HandleResult {
                    quit: true,
                    ..HandleResult::default()
                };
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                return HandleResult::default();
            }
            KeyCode::Char('g') => {
                let repository = self.sync_repository.clone().unwrap_or_default();
                self.mode = Mode::Settings(SettingsState {
                    cursor: repository.chars().count(),
                    repository,
                    editing: false,
                    confirm_disconnect: false,
                });
                return HandleResult::default();
            }
            KeyCode::Char('1') => {
                self.feature = Feature::Identities;
                return HandleResult::default();
            }
            KeyCode::Char('2') => {
                self.feature = Feature::Calendar;
                return HandleResult::default();
            }
            KeyCode::Char('3') => {
                self.feature = Feature::Statistics;
                return HandleResult::default();
            }
            KeyCode::Char('4') => {
                self.feature = Feature::Judgements;
                return HandleResult::default();
            }
            _ => {}
        }

        match self.feature {
            Feature::Identities => self.handle_identities(key),
            Feature::Calendar => self.handle_calendar(key),
            Feature::Statistics => self.handle_statistics(key),
            Feature::Judgements => self.handle_judgements(key),
        }
    }

    fn handle_identities(&mut self, key: KeyEvent) -> HandleResult {
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
            KeyCode::Char('s') => {
                if let Some(topic) = self.selected_topic() {
                    self.mode = Mode::SelectingStatus(StatusPicker {
                        topic: self.selected_topic,
                        selected: topic.status,
                    });
                }
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
            KeyCode::Char('d') => {
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

    fn handle_calendar(&mut self, key: KeyEvent) -> HandleResult {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let changed = match key.code {
                KeyCode::Up => self.move_calendar_entry(-1),
                KeyCode::Down => self.move_calendar_entry(1),
                _ => false,
            };
            return HandleResult {
                changed,
                ..HandleResult::default()
            };
        }

        match key.code {
            KeyCode::Char('[') => {
                self.change_month(-1);
                HandleResult::default()
            }
            KeyCode::Char(']') => {
                self.change_month(1);
                HandleResult::default()
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.calendar_focus = match self.calendar_focus {
                    CalendarFocus::Grid => CalendarFocus::Entries,
                    CalendarFocus::Entries => CalendarFocus::Grid,
                };
                if self.calendar_focus == CalendarFocus::Entries
                    && self.selected_calendar_entry.is_none()
                    && !self.selected_calendar_entries().is_empty()
                {
                    self.selected_calendar_entry = Some(0);
                }
                HandleResult::default()
            }
            KeyCode::Char('a') => {
                self.mode = Mode::Editing(Editor {
                    kind: EditKind::AddCalendarEntry(self.selected_date),
                    input: String::new(),
                    cursor: 0,
                });
                HandleResult::default()
            }
            KeyCode::Char('r') => {
                let selected = self
                    .document
                    .calendar_day(self.selected_date)
                    .and_then(|day| day.mood)
                    .unwrap_or(MoodRating::NEUTRAL);
                self.mode = Mode::SelectingMood(MoodPicker {
                    date: self.selected_date,
                    selected,
                });
                HandleResult::default()
            }
            KeyCode::Enter if self.calendar_focus == CalendarFocus::Grid => {
                self.calendar_focus = CalendarFocus::Entries;
                self.selected_calendar_entry =
                    (!self.selected_calendar_entries().is_empty()).then_some(0);
                HandleResult::default()
            }
            KeyCode::Enter => {
                if let Some(index) = self.selected_calendar_entry
                    && let Some(entry) = self.selected_calendar_entries().get(index)
                {
                    let input = entry.text.clone();
                    self.mode = Mode::Editing(Editor {
                        kind: EditKind::EditCalendarEntry(self.selected_date, index),
                        cursor: input.chars().count(),
                        input,
                    });
                }
                HandleResult::default()
            }
            KeyCode::Char(' ') if self.calendar_focus == CalendarFocus::Entries => {
                let changed = self.toggle_calendar_entry();
                HandleResult {
                    changed,
                    ..HandleResult::default()
                }
            }
            KeyCode::Char('d') if self.calendar_focus == CalendarFocus::Entries => {
                if let Some(index) = self.selected_calendar_entry {
                    self.mode =
                        Mode::ConfirmDelete(DeleteTarget::CalendarEntry(self.selected_date, index));
                }
                HandleResult::default()
            }
            KeyCode::Up if self.calendar_focus == CalendarFocus::Grid => {
                self.move_selected_date(-7, false);
                HandleResult::default()
            }
            KeyCode::Down if self.calendar_focus == CalendarFocus::Grid => {
                self.move_selected_date(7, false);
                HandleResult::default()
            }
            KeyCode::Left if self.calendar_focus == CalendarFocus::Grid => {
                self.move_selected_date(-1, true);
                HandleResult::default()
            }
            KeyCode::Right if self.calendar_focus == CalendarFocus::Grid => {
                self.move_selected_date(1, true);
                HandleResult::default()
            }
            KeyCode::Up if self.calendar_focus == CalendarFocus::Entries => {
                self.select_calendar_entry(-1);
                HandleResult::default()
            }
            KeyCode::Down if self.calendar_focus == CalendarFocus::Entries => {
                self.select_calendar_entry(1);
                HandleResult::default()
            }
            KeyCode::Esc => {
                self.calendar_focus = CalendarFocus::Grid;
                HandleResult::default()
            }
            _ => HandleResult::default(),
        }
    }

    fn handle_statistics(&mut self, key: KeyEvent) -> HandleResult {
        self.statistics_period = match key.code {
            KeyCode::Char('m' | 'M') => StatisticsPeriod::Month,
            KeyCode::Char('y' | 'Y') => StatisticsPeriod::Year,
            KeyCode::Char('f' | 'F') => StatisticsPeriod::Forever,
            _ => return HandleResult::default(),
        };
        HandleResult::default()
    }

    fn handle_judgements(&mut self, key: KeyEvent) -> HandleResult {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let changed = match key.code {
                KeyCode::Up => self.move_judgement_selection(-1),
                KeyCode::Down => self.move_judgement_selection(1),
                _ => false,
            };
            return HandleResult {
                changed,
                ..HandleResult::default()
            };
        }

        match key.code {
            KeyCode::Char('n') => {
                self.mode = Mode::EditingJudgement(JudgementForm {
                    target: None,
                    name: String::new(),
                    name_cursor: 0,
                    follow_up: String::new(),
                    follow_up_cursor: 0,
                    field: JudgementField::Name,
                });
                HandleResult::default()
            }
            KeyCode::Char('a') if self.selected_judgement().is_some() => {
                self.mode = Mode::EditingCharacteristic(CharacteristicForm {
                    judgement: self.selected_judgement,
                    target: None,
                    name: String::new(),
                    name_cursor: 0,
                    before_text: String::new(),
                    before_cursor: 0,
                    before_rating: Sentiment::Neutral,
                    after_text: String::new(),
                    after_cursor: 0,
                    after_rating: Sentiment::Neutral,
                    has_after: false,
                    field: CharacteristicField::Name,
                });
                HandleResult::default()
            }
            KeyCode::Char('v') if self.selected_characteristic().is_some() => {
                self.open_characteristic_form(CharacteristicField::AfterText);
                HandleResult::default()
            }
            KeyCode::Enter => {
                match self.judgement_focus {
                    JudgementFocus::Judgements => self.open_judgement_form(),
                    JudgementFocus::Characteristics => {
                        self.open_characteristic_form(CharacteristicField::Name)
                    }
                }
                HandleResult::default()
            }
            KeyCode::Char('d') => {
                let target = match self.judgement_focus {
                    JudgementFocus::Judgements => self
                        .selected_judgement()
                        .map(|_| DeleteTarget::Judgement(self.selected_judgement)),
                    JudgementFocus::Characteristics => self
                        .selected_characteristic
                        .map(|row| DeleteTarget::Characteristic(self.selected_judgement, row)),
                };
                if let Some(target) = target {
                    self.mode = Mode::ConfirmDelete(target);
                }
                HandleResult::default()
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.judgement_focus = match self.judgement_focus {
                    JudgementFocus::Judgements => JudgementFocus::Characteristics,
                    JudgementFocus::Characteristics => JudgementFocus::Judgements,
                };
                if self.judgement_focus == JudgementFocus::Characteristics
                    && self.selected_characteristic.is_none()
                    && self
                        .selected_judgement()
                        .is_some_and(|judgement| !judgement.characteristics.is_empty())
                {
                    self.selected_characteristic = Some(0);
                }
                HandleResult::default()
            }
            KeyCode::Up => {
                self.select_judgement_row(-1);
                HandleResult::default()
            }
            KeyCode::Down => {
                self.select_judgement_row(1);
                HandleResult::default()
            }
            KeyCode::Esc => {
                self.judgement_focus = JudgementFocus::Judgements;
                HandleResult::default()
            }
            _ => HandleResult::default(),
        }
    }

    fn handle_judgement_form(&mut self, key: KeyEvent, form: &mut JudgementForm) -> HandleResult {
        match key.code {
            KeyCode::Esc => return HandleResult::default(),
            KeyCode::Tab | KeyCode::BackTab => {
                form.field = match form.field {
                    JudgementField::Name => JudgementField::FollowUp,
                    JudgementField::FollowUp => JudgementField::Name,
                };
            }
            KeyCode::Enter => {
                let Some(changed) = self.commit_judgement_form(form) else {
                    self.mode = Mode::EditingJudgement(form.clone());
                    return HandleResult::default();
                };
                return HandleResult {
                    changed,
                    ..HandleResult::default()
                };
            }
            _ => match form.field {
                JudgementField::Name => edit_value(&mut form.name, &mut form.name_cursor, key),
                JudgementField::FollowUp => {
                    edit_value(&mut form.follow_up, &mut form.follow_up_cursor, key)
                }
            },
        }
        self.mode = Mode::EditingJudgement(form.clone());
        HandleResult::default()
    }

    fn handle_characteristic_form(
        &mut self,
        key: KeyEvent,
        form: &mut CharacteristicForm,
    ) -> HandleResult {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('x') {
            form.after_text.clear();
            form.after_cursor = 0;
            form.after_rating = Sentiment::Neutral;
            form.has_after = false;
            self.mode = Mode::EditingCharacteristic(form.clone());
            return HandleResult::default();
        }
        match key.code {
            KeyCode::Esc => return HandleResult::default(),
            KeyCode::Tab => form.field = next_characteristic_field(form.field, 1),
            KeyCode::BackTab => form.field = next_characteristic_field(form.field, -1),
            KeyCode::Enter => {
                let Some(changed) = self.commit_characteristic_form(form) else {
                    self.mode = Mode::EditingCharacteristic(form.clone());
                    return HandleResult::default();
                };
                return HandleResult {
                    changed,
                    ..HandleResult::default()
                };
            }
            _ => match form.field {
                CharacteristicField::Name => edit_value(&mut form.name, &mut form.name_cursor, key),
                CharacteristicField::BeforeText => {
                    edit_value(&mut form.before_text, &mut form.before_cursor, key)
                }
                CharacteristicField::BeforeRating => {
                    update_sentiment(&mut form.before_rating, key);
                }
                CharacteristicField::AfterText => {
                    edit_value(&mut form.after_text, &mut form.after_cursor, key);
                    if !form.after_text.is_empty() {
                        form.has_after = true;
                    }
                }
                CharacteristicField::AfterRating => {
                    if update_sentiment(&mut form.after_rating, key) {
                        form.has_after = true;
                    }
                }
            },
        }
        self.mode = Mode::EditingCharacteristic(form.clone());
        HandleResult::default()
    }

    fn handle_settings(&mut self, key: KeyEvent, settings: &mut SettingsState) -> HandleResult {
        if settings.confirm_disconnect {
            match key.code {
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                    settings.confirm_disconnect = false;
                    self.mode = Mode::Settings(settings.clone());
                    return HandleResult {
                        sync_action: Some(SyncAction::Disconnect),
                        ..HandleResult::default()
                    };
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                    settings.confirm_disconnect = false;
                }
                _ => {}
            }
            self.mode = Mode::Settings(settings.clone());
            return HandleResult::default();
        }

        if settings.editing {
            match key.code {
                KeyCode::Esc => settings.editing = false,
                KeyCode::Enter => {
                    let repository = settings.repository.trim().to_owned();
                    if let Err(error) = validate_github_url(&repository) {
                        self.status = Some(format!("Could not connect: {error}"));
                    } else {
                        settings.repository.clone_from(&repository);
                        settings.cursor = settings.repository.chars().count();
                        settings.editing = false;
                        self.mode = Mode::Settings(settings.clone());
                        return HandleResult {
                            sync_action: Some(SyncAction::Configure(repository)),
                            ..HandleResult::default()
                        };
                    }
                }
                _ => edit_value(&mut settings.repository, &mut settings.cursor, key),
            }
            self.mode = Mode::Settings(settings.clone());
            return HandleResult::default();
        }

        let action = match key.code {
            KeyCode::Esc | KeyCode::Char('g') => return HandleResult::default(),
            KeyCode::Char('e') => {
                settings.editing = true;
                settings.cursor = settings.repository.chars().count();
                None
            }
            KeyCode::Char('r') if self.sync_repository.is_some() => Some(SyncAction::Synchronize),
            KeyCode::Char('x') if self.sync_repository.is_some() => {
                settings.confirm_disconnect = true;
                None
            }
            KeyCode::Char('l') if matches!(self.sync_status, SyncStatus::Conflict(_)) => {
                Some(SyncAction::Resolve(ConflictChoice::KeepLocal))
            }
            KeyCode::Char('h') if matches!(self.sync_status, SyncStatus::Conflict(_)) => {
                Some(SyncAction::Resolve(ConflictChoice::UseRemote))
            }
            _ => None,
        };
        self.mode = Mode::Settings(settings.clone());
        HandleResult {
            sync_action: action,
            ..HandleResult::default()
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
                    status: IdentityStatus::Active,
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
            EditKind::AddCalendarEntry(date) => {
                let day = self.document.ensure_calendar_day(date);
                day.entries.push(Item {
                    text: value.into(),
                    done: false,
                });
                self.selected_date = date;
                self.selected_calendar_entry = Some(day.entries.len() - 1);
                self.calendar_focus = CalendarFocus::Entries;
            }
            EditKind::EditCalendarEntry(date, entry) => {
                let Some(target) = self
                    .document
                    .calendar_day_mut(date)
                    .and_then(|day| day.entries.get_mut(entry))
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

    fn open_judgement_form(&mut self) {
        let Some(judgement) = self.selected_judgement() else {
            return;
        };
        self.mode = Mode::EditingJudgement(JudgementForm {
            target: Some(self.selected_judgement),
            name: judgement.name.clone(),
            name_cursor: judgement.name.chars().count(),
            follow_up: judgement.follow_up.clone(),
            follow_up_cursor: judgement.follow_up.chars().count(),
            field: JudgementField::Name,
        });
    }

    fn open_characteristic_form(&mut self, field: CharacteristicField) {
        let Some(characteristic) = self.selected_characteristic() else {
            return;
        };
        let after = characteristic.after.as_ref();
        self.mode = Mode::EditingCharacteristic(CharacteristicForm {
            judgement: self.selected_judgement,
            target: self.selected_characteristic,
            name: characteristic.name.clone(),
            name_cursor: characteristic.name.chars().count(),
            before_text: characteristic.before.text.clone(),
            before_cursor: characteristic.before.text.chars().count(),
            before_rating: characteristic.before.rating,
            after_text: after.map(|value| value.text.clone()).unwrap_or_default(),
            after_cursor: after.map_or(0, |value| value.text.chars().count()),
            after_rating: after.map_or(Sentiment::Neutral, |value| value.rating),
            has_after: after.is_some(),
            field,
        });
    }

    fn commit_judgement_form(&mut self, form: &JudgementForm) -> Option<bool> {
        let name = form.name.trim();
        if name.is_empty() {
            self.status = Some("Judgement name cannot be empty".into());
            return None;
        }
        let follow_up = form.follow_up.trim();
        if let Some(target) = form.target {
            let judgement = self.document.judgements.get_mut(target)?;
            if judgement.name == name && judgement.follow_up == follow_up {
                return Some(false);
            }
            judgement.name = name.into();
            judgement.follow_up = follow_up.into();
            self.selected_judgement = target;
        } else {
            self.document.judgements.push(Judgement {
                name: name.into(),
                follow_up: follow_up.into(),
                characteristics: Vec::new(),
            });
            self.selected_judgement = self.document.judgements.len() - 1;
            self.selected_characteristic = None;
        }
        self.status = Some("Judgement saved".into());
        Some(true)
    }

    fn commit_characteristic_form(&mut self, form: &CharacteristicForm) -> Option<bool> {
        let name = form.name.trim();
        let before_text = form.before_text.trim();
        if name.is_empty() {
            self.status = Some("Characteristic name cannot be empty".into());
            return None;
        }
        if before_text.is_empty() {
            self.status = Some("Before text cannot be empty".into());
            return None;
        }
        let after_text = form.after_text.trim();
        if form.has_after && after_text.is_empty() {
            self.status = Some("After text cannot be empty; Ctrl+x clears it".into());
            return None;
        }
        let characteristic = Characteristic {
            name: name.into(),
            before: Observation {
                text: before_text.into(),
                rating: form.before_rating,
            },
            after: form.has_after.then(|| Observation {
                text: after_text.into(),
                rating: form.after_rating,
            }),
        };
        let judgement = self.document.judgements.get_mut(form.judgement)?;
        if let Some(target) = form.target {
            let current = judgement.characteristics.get_mut(target)?;
            if *current == characteristic {
                return Some(false);
            }
            *current = characteristic;
            self.selected_characteristic = Some(target);
        } else {
            judgement.characteristics.push(characteristic);
            self.selected_characteristic = Some(judgement.characteristics.len() - 1);
        }
        self.selected_judgement = form.judgement;
        self.judgement_focus = JudgementFocus::Characteristics;
        self.status = Some(if form.has_after {
            "Characteristic verified".into()
        } else {
            "Characteristic saved".into()
        });
        Some(true)
    }

    fn commit_status(&mut self, picker: StatusPicker) -> HandleResult {
        let Some(topic) = self.document.topics.get_mut(picker.topic) else {
            return HandleResult::default();
        };
        if topic.status == picker.selected {
            return HandleResult::default();
        }
        topic.status = picker.selected;
        self.status = Some(format!("Identity is now {}", picker.selected.label()));
        HandleResult {
            changed: true,
            ..HandleResult::default()
        }
    }

    fn commit_mood(&mut self, date: NaiveDate, mood: Option<MoodRating>) -> HandleResult {
        let current = self.document.calendar_day(date).and_then(|day| day.mood);
        if current == mood {
            return HandleResult::default();
        }
        if let Some(mood) = mood {
            self.document.ensure_calendar_day(date).mood = Some(mood);
            self.status = Some(format!("Mood saved: {} — {}", mood.value(), mood.label()));
        } else if let Some(day) = self.document.calendar_day_mut(date) {
            day.mood = None;
            self.document.remove_calendar_day_if_empty(date);
            self.status = Some("Mood rating cleared".into());
        }
        HandleResult {
            changed: true,
            ..HandleResult::default()
        }
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

    fn toggle_calendar_entry(&mut self) -> bool {
        let Some(index) = self.selected_calendar_entry else {
            return false;
        };
        let Some(entry) = self
            .document
            .calendar_day_mut(self.selected_date)
            .and_then(|day| day.entries.get_mut(index))
        else {
            return false;
        };
        entry.done = !entry.done;
        self.status = Some(if entry.done { "Checked" } else { "Unchecked" }.into());
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
            DeleteTarget::CalendarEntry(date, entry)
                if self
                    .document
                    .calendar_day(date)
                    .is_some_and(|day| entry < day.entries.len()) =>
            {
                let entries = &mut self
                    .document
                    .calendar_day_mut(date)
                    .expect("checked above")
                    .entries;
                entries.remove(entry);
                self.selected_calendar_entry = if entries.is_empty() {
                    None
                } else {
                    Some(entry.min(entries.len() - 1))
                };
                self.document.remove_calendar_day_if_empty(date);
                self.status = Some("Calendar entry deleted".into());
            }
            DeleteTarget::Judgement(judgement) if judgement < self.document.judgements.len() => {
                self.document.judgements.remove(judgement);
                self.selected_judgement =
                    judgement.min(self.document.judgements.len().saturating_sub(1));
                self.selected_characteristic = None;
                self.judgement_focus = JudgementFocus::Judgements;
                self.status = Some("Judgement deleted".into());
            }
            DeleteTarget::Characteristic(judgement, characteristic)
                if self
                    .document
                    .judgements
                    .get(judgement)
                    .is_some_and(|value| characteristic < value.characteristics.len()) =>
            {
                let characteristics = &mut self.document.judgements[judgement].characteristics;
                characteristics.remove(characteristic);
                self.selected_judgement = judgement;
                self.selected_characteristic = if characteristics.is_empty() {
                    None
                } else {
                    Some(characteristic.min(characteristics.len() - 1))
                };
                self.status = Some("Characteristic deleted".into());
            }
            _ => {}
        }
        self.ensure_visible_selection();
        self.ensure_judgement_selection();
    }

    fn select_up(&mut self) {
        if let Some(item) = self.selected_item {
            let visible = self.visible_items(self.selected_topic);
            let position = visible.iter().position(|&index| index == item).unwrap_or(0);
            self.selected_item = position.checked_sub(1).map(|position| visible[position]);
            return;
        }

        let visible_topics = self.visible_topics();
        let Some(position) = visible_topics
            .iter()
            .position(|&index| index == self.selected_topic)
        else {
            return;
        };
        let Some(previous) = position.checked_sub(1) else {
            return;
        };
        self.selected_topic = visible_topics[previous];
        self.selected_item = self.visible_items(self.selected_topic).last().copied();
    }

    fn select_down(&mut self) {
        let visible = self.visible_items(self.selected_topic);
        let next_item = match self.selected_item {
            None => visible.first().copied(),
            Some(item) => visible
                .iter()
                .position(|&index| index == item)
                .and_then(|position| visible.get(position + 1).copied()),
        };
        if let Some(item) = next_item {
            self.selected_item = Some(item);
            return;
        }

        let visible_topics = self.visible_topics();
        let Some(position) = visible_topics
            .iter()
            .position(|&index| index == self.selected_topic)
        else {
            return;
        };
        let Some(&next_topic) = visible_topics.get(position + 1) else {
            return;
        };
        self.selected_topic = next_topic;
        self.selected_item = None;
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

    fn select_judgement_row(&mut self, direction: isize) {
        match self.judgement_focus {
            JudgementFocus::Judgements => {
                let len = self.document.judgements.len();
                if len == 0 {
                    return;
                }
                self.selected_judgement = adjacent_index(self.selected_judgement, len, direction);
                self.selected_characteristic = None;
                self.characteristic_scroll = 0;
            }
            JudgementFocus::Characteristics => {
                let len = self
                    .selected_judgement()
                    .map_or(0, |judgement| judgement.characteristics.len());
                if len == 0 {
                    self.selected_characteristic = None;
                    return;
                }
                self.selected_characteristic = Some(adjacent_index(
                    self.selected_characteristic.unwrap_or(0),
                    len,
                    direction,
                ));
            }
        }
    }

    fn move_judgement_selection(&mut self, direction: isize) -> bool {
        match self.judgement_focus {
            JudgementFocus::Judgements => {
                let len = self.document.judgements.len();
                if len == 0 {
                    return false;
                }
                let destination = bounded_destination(self.selected_judgement, len, direction);
                let Some(destination) = destination else {
                    return false;
                };
                self.document
                    .judgements
                    .swap(self.selected_judgement, destination);
                self.selected_judgement = destination;
                true
            }
            JudgementFocus::Characteristics => {
                let Some(index) = self.selected_characteristic else {
                    return false;
                };
                let Some(judgement) = self.document.judgements.get_mut(self.selected_judgement)
                else {
                    return false;
                };
                let Some(destination) =
                    bounded_destination(index, judgement.characteristics.len(), direction)
                else {
                    return false;
                };
                judgement.characteristics.swap(index, destination);
                self.selected_characteristic = Some(destination);
                true
            }
        }
    }

    fn change_month(&mut self, direction: isize) {
        let target = if direction < 0 {
            self.displayed_month.checked_sub_months(Months::new(1))
        } else {
            self.displayed_month.checked_add_months(Months::new(1))
        };
        let Some(target) = target else {
            return;
        };
        let day = self.selected_date.day().min(days_in_month(target));
        self.displayed_month = target;
        self.selected_date = target.with_day(day).expect("day is clamped to month");
        self.selected_calendar_entry = None;
        self.calendar_focus = CalendarFocus::Grid;
        self.calendar_scroll = 0;
    }

    fn move_selected_date(&mut self, offset: i64, cross_month_boundary: bool) {
        let candidate = if offset < 0 {
            self.selected_date
                .checked_sub_days(Days::new(offset.unsigned_abs()))
        } else {
            self.selected_date
                .checked_add_days(Days::new(offset as u64))
        };
        if let Some(candidate) = candidate
            && (cross_month_boundary
                || (candidate.year() == self.displayed_month.year()
                    && candidate.month() == self.displayed_month.month()))
        {
            if candidate.year() != self.displayed_month.year()
                || candidate.month() != self.displayed_month.month()
            {
                self.displayed_month = candidate.with_day(1).expect("day one is always valid");
            }
            self.selected_date = candidate;
            self.selected_calendar_entry = None;
            self.calendar_scroll = 0;
        }
    }

    fn select_calendar_entry(&mut self, direction: isize) {
        let len = self.selected_calendar_entries().len();
        if len == 0 {
            self.selected_calendar_entry = None;
            return;
        }
        self.selected_calendar_entry = Some(match (self.selected_calendar_entry, direction < 0) {
            (Some(index), true) => index.saturating_sub(1),
            (Some(index), false) => (index + 1).min(len - 1),
            (None, _) => 0,
        });
    }

    fn move_calendar_entry(&mut self, direction: isize) -> bool {
        if self.calendar_focus != CalendarFocus::Entries {
            return false;
        }
        let Some(index) = self.selected_calendar_entry else {
            return false;
        };
        let Some(day) = self.document.calendar_day_mut(self.selected_date) else {
            return false;
        };
        let destination = if direction < 0 {
            index.checked_sub(1)
        } else {
            (index + 1 < day.entries.len()).then_some(index + 1)
        };
        let Some(destination) = destination else {
            return false;
        };
        day.entries.swap(index, destination);
        self.selected_calendar_entry = Some(destination);
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

    fn ensure_calendar_selection(&mut self) {
        let entry_count = self.selected_calendar_entries().len();
        if self
            .selected_calendar_entry
            .is_some_and(|entry| entry >= entry_count)
        {
            self.selected_calendar_entry = None;
        }
    }

    fn ensure_judgement_selection(&mut self) {
        if self.document.judgements.is_empty() {
            self.selected_judgement = 0;
            self.selected_characteristic = None;
            self.judgement_focus = JudgementFocus::Judgements;
            self.judgement_scroll = 0;
            self.characteristic_scroll = 0;
            return;
        }
        self.selected_judgement = self
            .selected_judgement
            .min(self.document.judgements.len() - 1);
        let characteristic_count = self.document.judgements[self.selected_judgement]
            .characteristics
            .len();
        if characteristic_count == 0 {
            self.selected_characteristic = None;
        } else if let Some(characteristic) = self.selected_characteristic {
            self.selected_characteristic = Some(characteristic.min(characteristic_count - 1));
        }
    }
}

pub fn days_in_month(month: NaiveDate) -> u32 {
    let next = month
        .with_day(1)
        .expect("day one is always valid")
        .checked_add_months(Months::new(1))
        .expect("supported dates have a following month");
    next.checked_sub_days(Days::new(1))
        .expect("following month has a previous day")
        .day()
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&needle.trim().to_lowercase())
}

fn adjacent_status(status: IdentityStatus, direction: isize) -> IdentityStatus {
    let position = IdentityStatus::ALL
        .iter()
        .position(|candidate| *candidate == status)
        .unwrap_or(1);
    let next = if direction < 0 {
        position.saturating_sub(1)
    } else {
        (position + 1).min(IdentityStatus::ALL.len() - 1)
    };
    IdentityStatus::ALL[next]
}

fn adjacent_mood(mood: MoodRating, direction: isize) -> MoodRating {
    let position = MoodRating::ALL
        .iter()
        .position(|candidate| *candidate == mood)
        .unwrap_or(2);
    let next = if direction < 0 {
        position.saturating_sub(1)
    } else {
        (position + 1).min(MoodRating::ALL.len() - 1)
    };
    MoodRating::ALL[next]
}

fn sentiment_index(sentiment: Sentiment) -> usize {
    Sentiment::ALL
        .iter()
        .position(|candidate| *candidate == sentiment)
        .unwrap_or(1)
}

fn adjacent_index(index: usize, len: usize, direction: isize) -> usize {
    if direction < 0 {
        index.saturating_sub(1)
    } else {
        (index + 1).min(len.saturating_sub(1))
    }
}

fn bounded_destination(index: usize, len: usize, direction: isize) -> Option<usize> {
    let destination = adjacent_index(index, len, direction);
    (destination != index).then_some(destination)
}

fn next_characteristic_field(field: CharacteristicField, direction: isize) -> CharacteristicField {
    let fields = [
        CharacteristicField::Name,
        CharacteristicField::BeforeText,
        CharacteristicField::BeforeRating,
        CharacteristicField::AfterText,
        CharacteristicField::AfterRating,
    ];
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if direction < 0 {
        index.checked_sub(1).unwrap_or(fields.len() - 1)
    } else {
        (index + 1) % fields.len()
    };
    fields[next]
}

fn update_sentiment(sentiment: &mut Sentiment, key: KeyEvent) -> bool {
    let selected = match key.code {
        KeyCode::Char('+') => Some(Sentiment::Positive),
        KeyCode::Char('0') => Some(Sentiment::Neutral),
        KeyCode::Char('-') => Some(Sentiment::Negative),
        KeyCode::Left | KeyCode::Up => {
            let index = sentiment_index(*sentiment);
            Some(Sentiment::ALL[index.saturating_sub(1)])
        }
        KeyCode::Right | KeyCode::Down => {
            let index = sentiment_index(*sentiment);
            Some(Sentiment::ALL[(index + 1).min(Sentiment::ALL.len() - 1)])
        }
        _ => None,
    };
    if let Some(selected) = selected {
        *sentiment = selected;
        true
    } else {
        false
    }
}

fn edit_text(editor: &mut Editor, key: KeyEvent) {
    edit_value(&mut editor.input, &mut editor.cursor, key);
}

fn edit_value(value: &mut String, cursor: &mut usize, key: KeyEvent) {
    match key.code {
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            let byte = char_to_byte(value, *cursor);
            value.insert(byte, character);
            *cursor += 1;
        }
        KeyCode::Backspace if *cursor > 0 => {
            let start = char_to_byte(value, *cursor - 1);
            let end = char_to_byte(value, *cursor);
            value.replace_range(start..end, "");
            *cursor -= 1;
        }
        KeyCode::Delete if *cursor < value.chars().count() => {
            let start = char_to_byte(value, *cursor);
            let end = char_to_byte(value, *cursor + 1);
            value.replace_range(start..end, "");
        }
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(value.chars().count()),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = value.chars().count(),
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
    use crate::model::{Calendar, DATA_VERSION};
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn type_text(app: &mut App, value: &str) {
        for character in value.chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
    }

    fn app() -> App {
        App::new(Document {
            version: DATA_VERSION,
            topics: vec![
                Topic {
                    name: "Developer".into(),
                    status: IdentityStatus::Active,
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
                    status: IdentityStatus::Active,
                    items: vec![Item {
                        text: "Alps".into(),
                        done: false,
                    }],
                },
            ],
            calendar: Calendar::default(),
            judgements: Vec::new(),
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
    fn vertical_navigation_crosses_identity_boundaries() {
        let mut app = app();
        app.selected_item = Some(1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_topic, 1);
        assert_eq!(app.selected_item, None);

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_topic, 0);
        assert_eq!(app.selected_item, Some(1));
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
        assert_eq!(
            app.document.topics.last().unwrap().status,
            IdentityStatus::Active
        );

        app.handle_key(key(KeyCode::Char('a')));
        for character in "Essay".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.document.topics.last().unwrap().items[0].text, "Essay");

        app.handle_key(key(KeyCode::Char('d')));
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

    #[test]
    fn selects_and_commits_identity_status() {
        let mut app = app();
        app.selected_item = Some(0);
        assert!(!app.handle_key(key(KeyCode::Char('s'))).changed);
        assert!(matches!(
            app.mode,
            Mode::SelectingStatus(StatusPicker {
                selected: IdentityStatus::Active,
                ..
            })
        ));

        app.handle_key(key(KeyCode::Down));
        let result = app.handle_key(key(KeyCode::Enter));
        assert!(result.changed);
        assert_eq!(app.document.topics[0].status, IdentityStatus::Former);
        assert_eq!(app.status.as_deref(), Some("Identity is now Former"));
    }

    #[test]
    fn status_picker_cancels_and_skips_unchanged_saves() {
        let mut app = app();
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Up));
        assert!(!app.handle_key(key(KeyCode::Esc)).changed);
        assert_eq!(app.document.topics[0].status, IdentityStatus::Active);

        app.handle_key(key(KeyCode::Char('s')));
        assert!(!app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn settings_configure_and_resolve_sync() {
        let mut app = app();
        app.handle_key(key(KeyCode::Char('g')));
        app.handle_key(key(KeyCode::Char('e')));
        for character in "https://github.com/person/private".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        let result = app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            result.sync_action,
            Some(SyncAction::Configure(
                "https://github.com/person/private".into()
            ))
        );

        app.set_sync_repository(Some("https://github.com/person/private".into()));
        app.set_sync_status(SyncStatus::Conflict("both changed".into()));
        let result = app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(
            result.sync_action,
            Some(SyncAction::Resolve(ConflictChoice::KeepLocal))
        );
    }

    #[test]
    fn switches_features_and_starts_calendar_on_today() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let mut app = App::new_at(Document::default(), today);

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.feature, Feature::Calendar);
        assert_eq!(
            app.displayed_month,
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()
        );
        assert_eq!(app.selected_date, today);

        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.feature, Feature::Identities);
    }

    #[test]
    fn calendar_navigation_crosses_month_boundaries_and_month_switching_clamps() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 31).unwrap();
        let mut app = App::new_at(Document::default(), today);
        app.feature = Feature::Calendar;

        app.handle_key(key(KeyCode::Right));
        assert_eq!(
            app.displayed_month,
            NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()
        );
        assert_eq!(
            app.selected_date,
            NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()
        );
        app.handle_key(key(KeyCode::Left));
        assert_eq!(
            app.displayed_month,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
        );
        assert_eq!(app.selected_date, today);

        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(
            app.displayed_month,
            NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()
        );
        assert_eq!(
            app.selected_date,
            NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()
        );
        app.handle_key(key(KeyCode::Left));
        assert_eq!(
            app.selected_date,
            NaiveDate::from_ymd_opt(2025, 2, 27).unwrap()
        );
    }

    #[test]
    fn creates_checks_reorders_edits_and_deletes_calendar_entries() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let mut app = App::new_at(Document::default(), date);
        app.feature = Feature::Calendar;

        for text in ["First", "Second"] {
            app.handle_key(key(KeyCode::Char('a')));
            for character in text.chars() {
                app.handle_key(key(KeyCode::Char(character)));
            }
            assert!(app.handle_key(key(KeyCode::Enter)).changed);
        }
        assert_eq!(app.selected_calendar_entries().len(), 2);
        assert!(app.handle_key(key(KeyCode::Char(' '))).changed);
        assert!(app.selected_calendar_entries()[1].done);
        assert!(app.handle_key(ctrl(KeyCode::Up)).changed);
        assert_eq!(app.selected_calendar_entries()[0].text, "Second");

        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Char('!')));
        assert!(app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(app.selected_calendar_entries()[0].text, "Second!");

        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.handle_key(key(KeyCode::Char('y'))).changed);
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.handle_key(key(KeyCode::Char('y'))).changed);
        assert!(app.document.calendar.days.is_empty());
    }

    #[test]
    fn computes_leap_year_month_lengths() {
        assert_eq!(
            days_in_month(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()),
            29
        );
        assert_eq!(
            days_in_month(NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()),
            28
        );
        assert_eq!(
            days_in_month(NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()),
            31
        );
    }

    #[test]
    fn rates_and_clears_a_day_without_calendar_entries() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let mut app = App::new_at(Document::default(), date);
        app.feature = Feature::Calendar;

        app.handle_key(key(KeyCode::Char('r')));
        assert!(matches!(app.mode, Mode::SelectingMood(_)));
        assert!(app.handle_key(key(KeyCode::Char('5'))).changed);
        assert_eq!(
            app.document.calendar_day(date).unwrap().mood,
            Some(MoodRating::try_from(5).unwrap())
        );
        assert!(app.document.calendar_day(date).unwrap().entries.is_empty());

        app.handle_key(key(KeyCode::Char('3')));
        let statistics = app.mood_statistics();
        assert_eq!(statistics.counts, [0, 0, 0, 0, 1]);
        assert_eq!(statistics.rated_days, 1);

        app.handle_key(key(KeyCode::Char('2')));
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.handle_key(key(KeyCode::Char('c'))).changed);
        assert!(app.document.calendar_day(date).is_none());
    }

    #[test]
    fn statistics_refresh_after_the_day_changes_while_running() {
        let yesterday = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let today = yesterday.checked_add_days(Days::new(1)).unwrap();
        let mut app = App::new_at(Document::default(), yesterday);
        app.document.ensure_calendar_day(today).mood = Some(MoodRating::try_from(4).unwrap());

        assert_eq!(app.mood_statistics().rated_days, 0);

        app.update_today(today);

        let statistics = app.mood_statistics();
        assert_eq!(statistics.counts, [0, 0, 0, 1, 0]);
        assert_eq!(statistics.rated_days, 1);
    }

    #[test]
    fn mood_statistics_use_rolling_periods_and_exclude_future_days() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let mut app = App::new_at(Document::default(), today);
        for (offset, rating) in [(0, 5), (29, 1), (30, 2), (364, 4), (365, 3)] {
            let date = today.checked_sub_days(Days::new(offset)).unwrap();
            app.document.ensure_calendar_day(date).mood =
                Some(MoodRating::try_from(rating).unwrap());
        }
        let future = today.checked_add_days(Days::new(1)).unwrap();
        app.document.ensure_calendar_day(future).mood = Some(MoodRating::try_from(5).unwrap());

        let month = app.mood_statistics();
        assert_eq!(month.counts, [1, 0, 0, 0, 1]);
        assert_eq!(month.rated_days, 2);
        assert_eq!(month.average(), Some(3.0));

        app.statistics_period = StatisticsPeriod::Year;
        let year = app.mood_statistics();
        assert_eq!(year.counts, [1, 1, 0, 1, 1]);
        assert_eq!(year.rated_days, 4);

        app.feature = Feature::Statistics;
        app.handle_key(key(KeyCode::Char('f')));
        let forever = app.mood_statistics();
        assert_eq!(forever.counts, [1, 1, 1, 1, 1]);
        assert_eq!(forever.rated_days, 5);
        assert_eq!(forever.average(), Some(3.0));
    }

    #[test]
    fn creates_and_verifies_a_judgement_characteristic() {
        let mut app = App::new(Document::default());
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.feature, Feature::Judgements);

        app.handle_key(key(KeyCode::Char('n')));
        type_text(&mut app, "New role");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "After one year");
        assert!(app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(app.document.judgements[0].follow_up, "After one year");

        app.handle_key(key(KeyCode::Char('a')));
        type_text(&mut app, "Autonomy");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "Expected freedom");
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('+')));
        assert!(app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(
            app.document.judgements[0].characteristics[0].before.rating,
            Sentiment::Positive
        );
        assert!(
            app.document.judgements[0].characteristics[0]
                .after
                .is_none()
        );

        app.handle_key(key(KeyCode::Char('v')));
        type_text(&mut app, "Some constraints");
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('-')));
        assert!(app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(
            app.document.judgements[0].characteristics[0]
                .after
                .as_ref()
                .unwrap()
                .rating,
            Sentiment::Negative
        );

        let statistics = app.judgement_statistics();
        assert_eq!(statistics.before_counts, [1, 0, 0]);
        assert_eq!(statistics.after_counts, [0, 0, 1]);
        assert_eq!(statistics.characteristics, 1);
        assert_eq!(statistics.verified, 1);
    }

    #[test]
    fn incomplete_after_observation_is_not_committed() {
        let mut app = App::new(Document::default());
        app.document.judgements.push(Judgement {
            name: "Laptop".into(),
            follow_up: String::new(),
            characteristics: vec![Characteristic {
                name: "Battery".into(),
                before: Observation {
                    text: "Expected all day".into(),
                    rating: Sentiment::Positive,
                },
                after: None,
            }],
        });
        app.feature = Feature::Judgements;
        app.judgement_focus = JudgementFocus::Characteristics;
        app.selected_characteristic = Some(0);

        app.handle_key(key(KeyCode::Char('v')));
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('-')));
        assert!(!app.handle_key(key(KeyCode::Enter)).changed);
        assert!(matches!(app.mode, Mode::EditingCharacteristic(_)));
        assert!(
            app.document.judgements[0].characteristics[0]
                .after
                .is_none()
        );

        assert!(!app.handle_key(ctrl(KeyCode::Char('x'))).changed);
        assert!(!app.handle_key(key(KeyCode::Enter)).changed);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn judgement_statistics_exclude_unverified_rows_from_after_percentages() {
        let mut app = App::new(Document::default());
        app.document.judgements.push(Judgement {
            name: "Investigation".into(),
            follow_up: "After research".into(),
            characteristics: vec![
                Characteristic {
                    name: "First".into(),
                    before: Observation {
                        text: "Promising".into(),
                        rating: Sentiment::Positive,
                    },
                    after: Some(Observation {
                        text: "Average".into(),
                        rating: Sentiment::Neutral,
                    }),
                },
                Characteristic {
                    name: "Second".into(),
                    before: Observation {
                        text: "Concerning".into(),
                        rating: Sentiment::Negative,
                    },
                    after: None,
                },
            ],
        });

        let statistics = app.judgement_statistics();
        assert_eq!(statistics.before_counts, [1, 0, 1]);
        assert_eq!(statistics.after_counts, [0, 1, 0]);
        assert_eq!(statistics.characteristics, 2);
        assert_eq!(statistics.verified, 1);
    }

    #[test]
    fn reorders_and_deletes_judgement_content() {
        let mut app = App::new(Document::default());
        for name in ["First", "Second"] {
            app.document.judgements.push(Judgement {
                name: name.into(),
                follow_up: String::new(),
                characteristics: Vec::new(),
            });
        }
        app.feature = Feature::Judgements;

        assert!(app.handle_key(ctrl(KeyCode::Down)).changed);
        assert_eq!(app.document.judgements[1].name, "First");
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.handle_key(key(KeyCode::Char('y'))).changed);
        assert_eq!(app.document.judgements.len(), 1);
        assert_eq!(app.selected_judgement, 0);
    }
}
