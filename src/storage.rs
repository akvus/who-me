use std::{
    env,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::model::Document;

#[derive(Debug)]
pub enum StorageError {
    NoHomeDirectory,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Serialize(toml::ser::Error),
    Invalid {
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeDirectory => write!(f, "HOME is not set; cannot determine the data path"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Parse { path, source } => {
                write!(f, "could not parse {}: {source}", path.display())
            }
            Self::Serialize(source) => write!(f, "could not serialize data: {source}"),
            Self::Invalid { path, reason } => {
                write!(f, "invalid data in {}: {reason}", path.display())
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
            Self::NoHomeDirectory | Self::Invalid { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn discover() -> Result<Self, StorageError> {
        Ok(Self::new(data_path()?))
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn backup_path(&self) -> PathBuf {
        self.path.with_file_name(format!(
            "{}.bak",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("data.toml")
        ))
    }

    pub fn load_or_create(&self) -> Result<Document, StorageError> {
        if !self.path.exists() {
            let document = Document::default();
            self.save(&document)?;
            return Ok(document);
        }

        let source = fs::read_to_string(&self.path).map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })?;
        let mut document: Document =
            toml::from_str(&source).map_err(|source| StorageError::Parse {
                path: self.path.clone(),
                source,
            })?;
        let upgraded = document.upgrade().map_err(|reason| StorageError::Invalid {
            path: self.path.clone(),
            reason,
        })?;
        document
            .validate()
            .map_err(|reason| StorageError::Invalid {
                path: self.path.clone(),
                reason,
            })?;
        if upgraded {
            self.save(&document)?;
        }
        Ok(document)
    }

    pub fn save(&self, document: &Document) -> Result<(), StorageError> {
        document
            .validate()
            .map_err(|reason| StorageError::Invalid {
                path: self.path.clone(),
                reason,
            })?;
        let serialized = toml::to_string_pretty(document).map_err(StorageError::Serialize)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| StorageError::Io {
            path: parent.to_path_buf(),
            source,
        })?;

        let temporary = self.path.with_file_name(format!(
            ".{}.tmp-{}",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("data.toml"),
            std::process::id()
        ));

        let write_result = (|| -> Result<(), StorageError> {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)
                .map_err(|source| StorageError::Io {
                    path: temporary.clone(),
                    source,
                })?;
            file.write_all(serialized.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|source| StorageError::Io {
                    path: temporary.clone(),
                    source,
                })?;

            if self.path.exists() {
                fs::copy(&self.path, self.backup_path()).map_err(|source| StorageError::Io {
                    path: self.backup_path(),
                    source,
                })?;
            }

            fs::rename(&temporary, &self.path).map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })?;

            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

fn data_path() -> Result<PathBuf, StorageError> {
    if let Some(base) = nonempty_env("XDG_DATA_HOME") {
        return Ok(base.join("who-me/data.toml"));
    }
    nonempty_env("HOME")
        .map(|home| home.join(".local/share/who-me/data.toml"))
        .ok_or(StorageError::NoHomeDirectory)
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Calendar, DATA_VERSION, IdentityStatus, Item, Topic};
    use tempfile::tempdir;

    fn sample() -> Document {
        Document {
            version: DATA_VERSION,
            topics: vec![Topic {
                name: "Developer".into(),
                status: IdentityStatus::Active,
                items: vec![Item {
                    text: "Ship useful software".into(),
                    done: true,
                }],
            }],
            calendar: Calendar::default(),
        }
    }

    #[test]
    fn creates_and_round_trips_a_document() {
        let temp = tempdir().unwrap();
        let store = Store::new(temp.path().join("nested/data.toml"));
        let empty = store.load_or_create().unwrap();
        assert_eq!(empty, Document::default());
        assert!(store.path.exists());

        store.save(&sample()).unwrap();
        assert_eq!(store.load_or_create().unwrap(), sample());
        assert!(store.backup_path().exists());
    }

    #[test]
    fn backup_contains_the_previous_valid_version() {
        let temp = tempdir().unwrap();
        let store = Store::new(temp.path().join("data.toml"));
        store.save(&Document::default()).unwrap();
        store.save(&sample()).unwrap();

        let backup = fs::read_to_string(store.backup_path()).unwrap();
        let previous: Document = toml::from_str(&backup).unwrap();
        assert_eq!(previous, Document::default());
    }

    #[test]
    fn malformed_data_is_not_overwritten() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("data.toml");
        fs::write(&path, "not = [valid").unwrap();
        let store = Store::new(path.clone());

        assert!(matches!(
            store.load_or_create(),
            Err(StorageError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "not = [valid");
    }

    #[test]
    fn legacy_data_defaults_to_active() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("data.toml");
        fs::write(&path, "version = 1\n\n[[topics]]\nname = \"Developer\"\n").unwrap();

        let document = Store::new(path).load_or_create().unwrap();
        assert_eq!(document.version, DATA_VERSION);
        assert_eq!(document.topics[0].status, IdentityStatus::Active);
    }

    #[test]
    fn legacy_upgrade_preserves_the_v1_backup() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("data.toml");
        let source = "version = 1\n\n[[topics]]\nname = \"Developer\"\n";
        fs::write(&path, source).unwrap();
        let store = Store::new(path);

        let document = store.load_or_create().unwrap();

        assert_eq!(document.version, DATA_VERSION);
        assert_eq!(fs::read_to_string(store.backup_path()).unwrap(), source);
    }

    #[test]
    fn unknown_status_is_not_overwritten() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("data.toml");
        let source = "version = 1\n\n[[topics]]\nname = \"Developer\"\nstatus = \"paused\"\n";
        fs::write(&path, source).unwrap();
        let store = Store::new(path.clone());

        assert!(matches!(
            store.load_or_create(),
            Err(StorageError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }

    #[test]
    fn invalid_calendar_date_is_not_overwritten() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("data.toml");
        let source = "version = 2\n\n[[calendar.days]]\ndate = \"2026-02-30\"\n";
        fs::write(&path, source).unwrap();
        let store = Store::new(path.clone());

        assert!(matches!(
            store.load_or_create(),
            Err(StorageError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }
}
