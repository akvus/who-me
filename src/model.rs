use std::collections::HashSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const DATA_VERSION: u32 = 2;
pub const LEGACY_DATA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub calendar: Calendar,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    #[serde(default)]
    pub days: Vec<CalendarDay>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarDay {
    pub date: NaiveDate,
    #[serde(default)]
    pub entries: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub name: String,
    #[serde(default)]
    pub status: IdentityStatus,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentityStatus {
    Aspiring,
    #[default]
    Active,
    Former,
}

impl IdentityStatus {
    pub const ALL: [Self; 3] = [Self::Aspiring, Self::Active, Self::Former];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Aspiring => "Aspiring",
            Self::Active => "Active",
            Self::Former => "Former",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

const fn current_version() -> u32 {
    DATA_VERSION
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: DATA_VERSION,
            topics: Vec::new(),
            calendar: Calendar::default(),
        }
    }
}

impl Document {
    pub fn is_empty(&self) -> bool {
        self.topics.is_empty() && self.calendar.days.is_empty()
    }

    pub fn upgrade(&mut self) -> Result<bool, String> {
        match self.version {
            DATA_VERSION => Ok(false),
            LEGACY_DATA_VERSION => {
                self.version = DATA_VERSION;
                Ok(true)
            }
            version => Err(format!(
                "unsupported data version {version} (this build supports version {DATA_VERSION})"
            )),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != DATA_VERSION {
            return Err(format!(
                "unsupported data version {} (this build supports version {DATA_VERSION})",
                self.version
            ));
        }

        for (topic_index, topic) in self.topics.iter().enumerate() {
            if topic.name.trim().is_empty() {
                return Err(format!("topic {} has an empty name", topic_index + 1));
            }
            for (item_index, item) in topic.items.iter().enumerate() {
                if item.text.trim().is_empty() {
                    return Err(format!(
                        "item {} in topic {:?} is empty",
                        item_index + 1,
                        topic.name
                    ));
                }
            }
        }

        let mut dates = HashSet::new();
        for day in &self.calendar.days {
            if !dates.insert(day.date) {
                return Err(format!("calendar date {} appears more than once", day.date));
            }
            if day.entries.is_empty() {
                return Err(format!("calendar date {} has no entries", day.date));
            }
            for (entry_index, entry) in day.entries.iter().enumerate() {
                if entry.text.trim().is_empty() {
                    return Err(format!(
                        "entry {} on calendar date {} is empty",
                        entry_index + 1,
                        day.date
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn calendar_day(&self, date: NaiveDate) -> Option<&CalendarDay> {
        self.calendar.days.iter().find(|day| day.date == date)
    }

    pub fn calendar_day_mut(&mut self, date: NaiveDate) -> Option<&mut CalendarDay> {
        self.calendar.days.iter_mut().find(|day| day.date == date)
    }

    pub fn ensure_calendar_day(&mut self, date: NaiveDate) -> &mut CalendarDay {
        if let Some(index) = self.calendar.days.iter().position(|day| day.date == date) {
            return &mut self.calendar.days[index];
        }
        self.calendar.days.push(CalendarDay {
            date,
            entries: Vec::new(),
        });
        self.calendar.days.sort_by_key(|day| day.date);
        let index = self
            .calendar
            .days
            .iter()
            .position(|day| day.date == date)
            .expect("new calendar day was inserted");
        &mut self.calendar.days[index]
    }

    pub fn remove_calendar_day_if_empty(&mut self, date: NaiveDate) {
        if let Some(index) = self
            .calendar
            .days
            .iter()
            .position(|day| day.date == date && day.entries.is_empty())
        {
            self.calendar.days.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_document_is_valid_and_versioned() {
        let document = Document::default();
        assert_eq!(document.version, DATA_VERSION);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn validation_rejects_bad_version_and_blank_text() {
        let mut document = Document {
            version: 99,
            topics: Vec::new(),
            calendar: Calendar::default(),
        };
        assert!(document.validate().unwrap_err().contains("unsupported"));

        document.version = DATA_VERSION;
        document.topics.push(Topic {
            name: "  ".into(),
            status: IdentityStatus::Active,
            items: Vec::new(),
        });
        assert!(document.validate().unwrap_err().contains("empty name"));
    }

    #[test]
    fn upgrades_v1_and_validates_calendar_days() {
        let mut legacy: Document = toml::from_str("version = 1\n").unwrap();
        assert!(legacy.upgrade().unwrap());
        assert_eq!(legacy.version, DATA_VERSION);

        let date = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        legacy.ensure_calendar_day(date).entries.push(Item {
            text: "Plan launch".into(),
            done: false,
        });
        assert!(legacy.validate().is_ok());
        legacy.calendar.days.push(legacy.calendar.days[0].clone());
        assert!(legacy.validate().unwrap_err().contains("more than once"));
    }

    #[test]
    fn calendar_validation_rejects_empty_days_and_blank_entries() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let mut document = Document::default();
        document.calendar.days.push(CalendarDay {
            date,
            entries: Vec::new(),
        });
        assert!(document.validate().unwrap_err().contains("has no entries"));

        document.calendar.days[0].entries.push(Item {
            text: "  ".into(),
            done: false,
        });
        assert!(document.validate().unwrap_err().contains("is empty"));
    }

    #[test]
    fn identity_status_defaults_and_round_trips() {
        let legacy: Document = toml::from_str(
            r#"
version = 1

[[topics]]
name = "Developer"
"#,
        )
        .unwrap();
        assert_eq!(legacy.topics[0].status, IdentityStatus::Active);

        let mut document = legacy;
        for status in IdentityStatus::ALL {
            document.topics[0].status = status;
            let serialized = toml::to_string(&document).unwrap();
            assert!(serialized.contains(&format!("status = {:?}", status.label().to_lowercase())));
            let restored: Document = toml::from_str(&serialized).unwrap();
            assert_eq!(restored.topics[0].status, status);
        }
    }
}
