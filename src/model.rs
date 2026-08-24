use serde::{Deserialize, Serialize};

pub const DATA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default)]
    pub topics: Vec<Topic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
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
        }
    }
}

impl Document {
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
        Ok(())
    }

    pub fn item_count(&self) -> usize {
        self.topics.iter().map(|topic| topic.items.len()).sum()
    }

    pub fn done_count(&self) -> usize {
        self.topics
            .iter()
            .flat_map(|topic| &topic.items)
            .filter(|item| item.done)
            .count()
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
        };
        assert!(document.validate().unwrap_err().contains("unsupported"));

        document.version = DATA_VERSION;
        document.topics.push(Topic {
            name: "  ".into(),
            items: Vec::new(),
        });
        assert!(document.validate().unwrap_err().contains("empty name"));
    }
}
