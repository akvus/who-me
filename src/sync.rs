use std::{
    env, fmt,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::model::Document;

const CONFIG_VERSION: u32 = 1;
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const LEGACY_GIT_NAME: &str = "who-me";
const LEGACY_GIT_EMAIL: &str = "who-me@localhost";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "config_version")]
    pub version: u32,
    pub repository: String,
    #[serde(default = "default_branch")]
    pub branch: String,
}

impl SyncConfig {
    pub fn new(repository: String) -> Self {
        Self {
            version: CONFIG_VERSION,
            repository,
            branch: default_branch(),
        }
    }
}

const fn config_version() -> u32 {
    CONFIG_VERSION
}

fn default_branch() -> String {
    "main".into()
}

#[derive(Clone, Debug)]
pub struct SyncPaths {
    pub config: PathBuf,
    pub repository: PathBuf,
    pub archives: PathBuf,
    pub conflicts: PathBuf,
}

impl SyncPaths {
    pub fn discover() -> Result<Self, SyncError> {
        let home = nonempty_env("HOME");
        let data = nonempty_env("XDG_DATA_HOME")
            .or_else(|| home.as_ref().map(|path| path.join(".local/share")))
            .ok_or(SyncError::NoHomeDirectory)?;
        let config = nonempty_env("XDG_CONFIG_HOME")
            .or_else(|| home.as_ref().map(|path| path.join(".config")))
            .map(|path| path.join("who-me/config.toml"))
            .unwrap_or_else(|| data.join("who-me/config.toml"));
        let data = data.join("who-me");
        Ok(Self {
            config,
            repository: data.join("github-sync"),
            archives: data.join("sync-archives"),
            conflicts: data.join("conflicts"),
        })
    }

    #[cfg(test)]
    fn in_directory(root: &Path) -> Self {
        Self {
            config: root.join("config.toml"),
            repository: root.join("github-sync"),
            archives: root.join("archives"),
            conflicts: root.join("conflicts"),
        }
    }
}

#[derive(Debug)]
pub enum SyncError {
    NoHomeDirectory,
    Io { path: PathBuf, source: io::Error },
    InvalidConfig(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeDirectory => {
                write!(formatter, "HOME is not set; cannot determine sync paths")
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::InvalidConfig(reason) => {
                write!(formatter, "invalid sync configuration: {reason}")
            }
        }
    }
}

impl std::error::Error for SyncError {}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Option<SyncConfig>, SyncError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let source = fs::read_to_string(&self.path).map_err(|source| SyncError::Io {
            path: self.path.clone(),
            source,
        })?;
        let config: SyncConfig =
            toml::from_str(&source).map_err(|error| SyncError::InvalidConfig(error.to_string()))?;
        validate_config(&config)?;
        Ok(Some(config))
    }

    pub fn save(&self, config: &SyncConfig) -> Result<(), SyncError> {
        validate_config(config)?;
        let source = toml::to_string_pretty(config)
            .map_err(|error| SyncError::InvalidConfig(error.to_string()))?;
        write_private_atomic(&self.path, source.as_bytes())
    }

    pub fn clear(&self) -> Result<(), SyncError> {
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|source| SyncError::Io {
                path: self.path.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

fn validate_config(config: &SyncConfig) -> Result<(), SyncError> {
    if config.version != CONFIG_VERSION {
        return Err(SyncError::InvalidConfig(format!(
            "unsupported version {}",
            config.version
        )));
    }
    validate_github_url(&config.repository).map_err(SyncError::InvalidConfig)?;
    if config.branch.is_empty()
        || config.branch.starts_with('-')
        || config.branch.contains("..")
        || config.branch.contains("@{")
        || !config
            .branch
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._-".contains(character))
    {
        return Err(SyncError::InvalidConfig("invalid Git branch name".into()));
    }
    Ok(())
}

fn write_private_atomic(path: &Path, contents: &[u8]) -> Result<(), SyncError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| SyncError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let temporary = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml"),
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|source| SyncError::Io {
            path: temporary.clone(),
            source,
        })?;
        file.write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|source| SyncError::Io {
                path: temporary.clone(),
                source,
            })?;
        fs::rename(&temporary, path).map_err(|source| SyncError::Io {
            path: path.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn validate_github_url(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.contains('@') && value.starts_with("https://") {
        return Err("URLs containing credentials are not allowed".into());
    }
    let path = value
        .strip_prefix("https://github.com/")
        .or_else(|| value.strip_prefix("git@github.com:"))
        .or_else(|| value.strip_prefix("ssh://git@github.com/"))
        .ok_or_else(|| "use a GitHub HTTPS or SSH repository URL".to_string())?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next().unwrap_or_default();
    let repository = parts.next().unwrap_or_default();
    if owner.is_empty()
        || repository.is_empty()
        || parts.next().is_some()
        || path.contains(['?', '#', ' '])
    {
        return Err("expected a repository URL like https://github.com/owner/repository".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SyncStatus {
    #[default]
    LocalOnly,
    Connecting,
    Syncing,
    Synced,
    Pending,
    Offline(String),
    Error(String),
    Conflict(String),
}

impl SyncStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LocalOnly => "Local",
            Self::Connecting => "Connecting",
            Self::Syncing => "Syncing",
            Self::Synced => "Synced",
            Self::Pending => "Pending",
            Self::Offline(_) => "Offline",
            Self::Error(_) => "Error",
            Self::Conflict(_) => "Conflict",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Offline(message) | Self::Error(message) | Self::Conflict(message) => {
                Some(message)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictChoice {
    KeepLocal,
    UseRemote,
}

#[derive(Debug)]
enum SyncCommand {
    Configure(SyncConfig, Document, u64),
    Synchronize(Document, u64),
    AcceptPrepared,
    RetryPrepared(Document, u64),
    Resolve(ConflictChoice, Document, u64),
    Disconnect,
}

#[derive(Debug)]
pub enum SyncEvent {
    Status(SyncStatus),
    Configured(SyncConfig),
    Prepared { document: Document, revision: u64 },
    Disconnected,
}

pub struct SyncService {
    commands: Sender<SyncCommand>,
    events: Receiver<SyncEvent>,
}

impl SyncService {
    pub fn start(paths: SyncPaths) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        thread::spawn(move || Worker::new(paths, event_sender).run(command_receiver));
        Self {
            commands: command_sender,
            events: event_receiver,
        }
    }

    pub fn configure(&self, config: SyncConfig, document: Document, revision: u64) {
        let _ = self
            .commands
            .send(SyncCommand::Configure(config, document, revision));
    }

    pub fn synchronize(&self, document: Document, revision: u64) {
        let _ = self
            .commands
            .send(SyncCommand::Synchronize(document, revision));
    }

    pub fn accept_prepared(&self) {
        let _ = self.commands.send(SyncCommand::AcceptPrepared);
    }

    pub fn retry_prepared(&self, document: Document, revision: u64) {
        let _ = self
            .commands
            .send(SyncCommand::RetryPrepared(document, revision));
    }

    pub fn resolve(&self, choice: ConflictChoice, document: Document, revision: u64) {
        let _ = self
            .commands
            .send(SyncCommand::Resolve(choice, document, revision));
    }

    pub fn disconnect(&self) {
        let _ = self.commands.send(SyncCommand::Disconnect);
    }

    pub fn try_recv(&self) -> Option<SyncEvent> {
        self.events.try_recv().ok()
    }
}

#[derive(Debug)]
struct PreparedState {
    base: Option<String>,
    branch: String,
    needs_push: bool,
}

struct Worker {
    paths: SyncPaths,
    events: Sender<SyncEvent>,
    config: Option<SyncConfig>,
    prepared: Option<PreparedState>,
    conflict_remote: Option<Document>,
}

impl Worker {
    fn new(paths: SyncPaths, events: Sender<SyncEvent>) -> Self {
        Self {
            paths,
            events,
            config: None,
            prepared: None,
            conflict_remote: None,
        }
    }

    fn run(mut self, commands: Receiver<SyncCommand>) {
        while let Ok(command) = commands.recv() {
            match command {
                SyncCommand::Configure(config, document, revision) => {
                    self.handle_configure(config, document, revision)
                }
                SyncCommand::Synchronize(document, revision) => {
                    self.prepare(document, revision, false)
                }
                SyncCommand::AcceptPrepared => self.accept_prepared(),
                SyncCommand::RetryPrepared(document, revision) => {
                    self.reject_prepared();
                    self.prepare(document, revision, false);
                }
                SyncCommand::Resolve(choice, document, revision) => {
                    self.resolve(choice, document, revision)
                }
                SyncCommand::Disconnect => self.disconnect(),
            }
        }
    }

    fn emit(&self, event: SyncEvent) {
        let _ = self.events.send(event);
    }

    fn status(&self, status: SyncStatus) {
        self.emit(SyncEvent::Status(status));
    }

    fn handle_configure(&mut self, mut config: SyncConfig, document: Document, revision: u64) {
        self.status(SyncStatus::Connecting);
        if self
            .config
            .as_ref()
            .is_some_and(|current| current.repository != config.repository)
            && let Err(error) = archive_directory(&self.paths.repository, &self.paths.archives)
        {
            self.status(SyncStatus::Error(error));
            return;
        }
        self.config = Some(config.clone());
        match self.ensure_repository() {
            Ok(fresh) => {
                if let Ok(branch) = current_branch(&self.paths.repository) {
                    config.branch = branch;
                    self.config = Some(config.clone());
                    self.emit(SyncEvent::Configured(config));
                }
                self.prepare(document, revision, fresh);
            }
            Err(error) => self.status(git_failure_status(error)),
        }
    }

    fn ensure_repository(&self) -> Result<bool, String> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| "sync is not configured".to_string())?;
        if self.paths.repository.join(".git").is_dir() {
            let origin = git_text(&self.paths.repository, &["remote", "get-url", "origin"]);
            if origin
                .as_deref()
                .is_ok_and(|origin| origin.trim() == config.repository)
            {
                return Ok(false);
            }
            archive_directory(&self.paths.repository, &self.paths.archives)?;
        }
        let parent = self
            .paths
            .repository
            .parent()
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = self
            .paths
            .repository
            .with_file_name(format!("github-sync.tmp-{}", std::process::id()));
        if temporary.exists() {
            fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
        }
        let destination = temporary.to_string_lossy().into_owned();
        let result = git(None, &["clone", "--", &config.repository, &destination]);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }
        fs::rename(&temporary, &self.paths.repository).map_err(|error| error.to_string())?;
        remove_legacy_identity(&self.paths.repository)?;
        Ok(true)
    }

    fn prepare(&mut self, document: Document, revision: u64, first_connection: bool) {
        if self.config.is_none() {
            self.status(SyncStatus::LocalOnly);
            return;
        }
        self.status(SyncStatus::Syncing);
        let mut first_connection = first_connection;
        if !self.paths.repository.join(".git").is_dir() {
            match self.ensure_repository() {
                Ok(fresh) => {
                    first_connection |= fresh;
                    if let Ok(branch) = current_branch(&self.paths.repository) {
                        let configured = self.config.as_mut().map(|config| {
                            config.branch = branch;
                            config.clone()
                        });
                        if let Some(configured) = configured {
                            self.emit(SyncEvent::Configured(configured));
                        }
                    }
                }
                Err(error) => {
                    self.status(git_failure_status(error));
                    return;
                }
            }
        }
        let result = self.prepare_inner(&document, first_connection);
        match result {
            Ok((candidate, prepared)) => {
                self.prepared = Some(prepared);
                self.emit(SyncEvent::Prepared {
                    document: candidate,
                    revision,
                });
            }
            Err(PrepareError::Conflict(remote)) => {
                self.conflict_remote = Some(remote);
                self.status(SyncStatus::Conflict(
                    "Local and GitHub data both changed; choose which copy to keep".into(),
                ));
            }
            Err(PrepareError::Failure(error)) => self.status(git_failure_status(error)),
        }
    }

    fn prepare_inner(
        &self,
        document: &Document,
        first_connection: bool,
    ) -> Result<(Document, PreparedState), PrepareError> {
        let config = self.config.as_ref().expect("checked above");
        let repository = &self.paths.repository;
        remove_legacy_identity(repository).map_err(PrepareError::Failure)?;
        git(Some(repository.as_path()), &["fetch", "origin"]).map_err(PrepareError::Failure)?;
        let branch = current_branch(repository).unwrap_or_else(|_| config.branch.clone());
        let remote_reference = format!("origin/{branch}");
        let remote_exists = git(
            Some(repository.as_path()),
            &["rev-parse", "--verify", &remote_reference],
        )
        .is_ok();
        let remote = if remote_exists {
            read_git_document(repository, &format!("{remote_reference}:data.toml"))
                .map_err(PrepareError::Failure)?
        } else {
            None
        };

        if first_connection
            && let Some(remote) = &remote
            && !document.topics.is_empty()
            && !remote.topics.is_empty()
            && remote != document
        {
            return Err(PrepareError::Conflict(remote.clone()));
        }

        if first_connection
            && document.topics.is_empty()
            && remote
                .as_ref()
                .is_some_and(|remote| !remote.topics.is_empty())
        {
            return Ok((
                remote.expect("checked above"),
                PreparedState {
                    base: head(repository),
                    branch,
                    needs_push: false,
                },
            ));
        }

        let base = head(repository);
        write_repository_document(repository, document).map_err(PrepareError::Failure)?;
        commit_if_changed(repository).map_err(PrepareError::Failure)?;
        if remote_exists
            && let Err(error) = git(Some(repository.as_path()), &["rebase", &remote_reference])
        {
            let _ = git(Some(repository.as_path()), &["rebase", "--abort"]);
            let remote = remote.ok_or_else(|| PrepareError::Failure(error.clone()))?;
            return Err(PrepareError::Conflict(remote));
        }
        let candidate = read_worktree_document(repository)
            .map_err(|error| PrepareError::Failure(format!("invalid GitHub merge: {error}")))?;
        Ok((
            candidate,
            PreparedState {
                base,
                branch,
                needs_push: true,
            },
        ))
    }

    fn reject_prepared(&mut self) {
        if let Some(prepared) = self.prepared.take()
            && let Some(base) = prepared.base
        {
            let _ = git(
                Some(self.paths.repository.as_path()),
                &["reset", "--hard", &base],
            );
        }
    }

    fn accept_prepared(&mut self) {
        let Some(prepared) = self.prepared.take() else {
            return;
        };
        if !prepared.needs_push {
            self.status(SyncStatus::Synced);
            return;
        }
        let target = format!("HEAD:{}", prepared.branch);
        match git(
            Some(self.paths.repository.as_path()),
            &["push", "-u", "origin", &target],
        ) {
            Ok(()) => self.status(SyncStatus::Synced),
            Err(error) => self.status(git_failure_status(error)),
        }
    }

    fn resolve(&mut self, choice: ConflictChoice, document: Document, revision: u64) {
        let Some(previous_remote) = self.conflict_remote.take() else {
            return;
        };
        self.status(SyncStatus::Syncing);
        let remote = match self.refresh_remote() {
            Ok(remote) => remote,
            Err(error) => {
                self.conflict_remote = Some(previous_remote);
                self.status(SyncStatus::Conflict(format!(
                    "Could not refresh GitHub before resolving: {error}"
                )));
                return;
            }
        };
        if let Err(error) = backup_conflict(&self.paths.conflicts, &document, &remote) {
            self.conflict_remote = Some(remote);
            self.status(SyncStatus::Error(error));
            return;
        }
        match choice {
            ConflictChoice::KeepLocal => match self.keep_local(&document) {
                Ok(()) => self.status(SyncStatus::Synced),
                Err(error) => {
                    self.conflict_remote = Some(remote);
                    self.status(SyncStatus::Conflict(format!(
                        "Could not publish the local copy: {error}"
                    )));
                }
            },
            ConflictChoice::UseRemote => {
                if let Err(error) = self.reset_to_remote() {
                    self.conflict_remote = Some(remote);
                    self.status(SyncStatus::Conflict(format!(
                        "Could not prepare the GitHub copy: {error}"
                    )));
                    return;
                }
                self.prepared = Some(PreparedState {
                    base: head(&self.paths.repository),
                    branch: self
                        .config
                        .as_ref()
                        .map(|config| config.branch.clone())
                        .unwrap_or_else(default_branch),
                    needs_push: false,
                });
                self.emit(SyncEvent::Prepared {
                    document: remote,
                    revision,
                });
            }
        }
    }

    fn refresh_remote(&self) -> Result<Document, String> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| "sync is not configured".to_string())?;
        git(Some(self.paths.repository.as_path()), &["fetch", "origin"])?;
        let remote_reference = format!("origin/{}", config.branch);
        if git(
            Some(self.paths.repository.as_path()),
            &["rev-parse", "--verify", &remote_reference],
        )
        .is_err()
        {
            return Ok(Document::default());
        }
        read_git_document(
            &self.paths.repository,
            &format!("{remote_reference}:data.toml"),
        )
        .map(|document| document.unwrap_or_default())
    }

    fn reset_to_remote(&self) -> Result<(), String> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| "sync is not configured".to_string())?;
        let remote_reference = format!("origin/{}", config.branch);
        if git(
            Some(self.paths.repository.as_path()),
            &["rev-parse", "--verify", &remote_reference],
        )
        .is_ok()
        {
            git(
                Some(self.paths.repository.as_path()),
                &["reset", "--hard", &remote_reference],
            )?;
        }
        Ok(())
    }

    fn keep_local(&self, document: &Document) -> Result<(), String> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| "sync is not configured".to_string())?;
        let remote_reference = format!("origin/{}", config.branch);
        git(Some(self.paths.repository.as_path()), &["fetch", "origin"])?;
        if git(
            Some(self.paths.repository.as_path()),
            &["rev-parse", "--verify", &remote_reference],
        )
        .is_ok()
        {
            git(
                Some(self.paths.repository.as_path()),
                &["reset", "--hard", &remote_reference],
            )?;
        }
        write_repository_document(&self.paths.repository, document)?;
        commit_if_changed(&self.paths.repository)?;
        let target = format!("HEAD:{}", config.branch);
        git(
            Some(self.paths.repository.as_path()),
            &["push", "-u", "origin", &target],
        )
    }

    fn disconnect(&mut self) {
        self.prepared = None;
        self.conflict_remote = None;
        self.config = None;
        match archive_directory(&self.paths.repository, &self.paths.archives) {
            Ok(()) => {
                self.emit(SyncEvent::Disconnected);
            }
            Err(error) => self.status(SyncStatus::Error(error)),
        }
    }
}

#[derive(Debug)]
enum PrepareError {
    Conflict(Document),
    Failure(String),
}

fn git_failure_status(error: String) -> SyncStatus {
    let lower = error.to_lowercase();
    if lower.contains("invalid github data")
        || lower.contains("authentication")
        || lower.contains("permission denied")
        || lower.contains("repository not found")
        || lower.contains("could not read username")
        || lower.contains("could not read from remote repository")
        || lower.contains("not a git repository")
    {
        SyncStatus::Error(error)
    } else {
        SyncStatus::Offline(error)
    }
}

fn remove_legacy_identity(repository: &Path) -> Result<(), String> {
    for (key, legacy_value) in [
        ("user.name", LEGACY_GIT_NAME),
        ("user.email", LEGACY_GIT_EMAIL),
    ] {
        let output = git_output(Some(repository), &["config", "--local", "--get", key])?;
        match output.status.code() {
            Some(0) if String::from_utf8_lossy(&output.stdout).trim() == legacy_value => {
                git(Some(repository), &["config", "--local", "--unset-all", key])?;
            }
            Some(0 | 1) => {}
            _ => return Err(command_error(&output)),
        }
    }
    Ok(())
}

fn current_branch(repository: &Path) -> Result<String, String> {
    let branch = git_text(repository, &["symbolic-ref", "--short", "HEAD"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        Err("could not determine the repository branch".into())
    } else {
        Ok(branch.into())
    }
}

fn head(repository: &Path) -> Option<String> {
    git_text(repository, &["rev-parse", "HEAD"])
        .ok()
        .map(|value| value.trim().to_owned())
}

fn commit_if_changed(repository: &Path) -> Result<(), String> {
    git(repository.into(), &["add", "--", "data.toml"])?;
    let status = git_output(repository.into(), &["diff", "--cached", "--quiet"])?;
    if status.status.success() {
        return Ok(());
    }
    if status.status.code() != Some(1) {
        return Err(command_error(&status));
    }
    git(repository.into(), &["commit", "-m", "Update who-me data"])
}

fn write_repository_document(repository: &Path, document: &Document) -> Result<(), String> {
    document.validate()?;
    let source = toml::to_string_pretty(document).map_err(|error| error.to_string())?;
    fs::write(repository.join("data.toml"), source).map_err(|error| error.to_string())
}

fn read_worktree_document(repository: &Path) -> Result<Document, String> {
    let source =
        fs::read_to_string(repository.join("data.toml")).map_err(|error| error.to_string())?;
    parse_document(&source)
}

fn read_git_document(repository: &Path, object: &str) -> Result<Option<Document>, String> {
    match git_text(repository, &["show", object]) {
        Ok(source) => parse_document(&source)
            .map(Some)
            .map_err(|error| format!("invalid GitHub data: {error}")),
        Err(error) if error.contains("does not exist") || error.contains("exists on disk") => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn parse_document(source: &str) -> Result<Document, String> {
    let document: Document = toml::from_str(source).map_err(|error| error.to_string())?;
    document.validate()?;
    Ok(document)
}

fn backup_conflict(directory: &Path, local: &Document, remote: &Document) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let timestamp = timestamp();
    let local = toml::to_string_pretty(local).map_err(|error| error.to_string())?;
    let remote = toml::to_string_pretty(remote).map_err(|error| error.to_string())?;
    fs::write(directory.join(format!("{timestamp}-local.toml")), local)
        .map_err(|error| error.to_string())?;
    fs::write(directory.join(format!("{timestamp}-github.toml")), remote)
        .map_err(|error| error.to_string())
}

fn archive_directory(source: &Path, archives: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(archives).map_err(|error| error.to_string())?;
    let destination = archives.join(format!("github-sync-{}", timestamp()));
    fs::rename(source, destination).map_err(|error| error.to_string())
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn git(repository: Option<&Path>, arguments: &[&str]) -> Result<(), String> {
    let output = git_output(repository, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = git_output(Some(repository), arguments)?;
    if !output.status.success() {
        return Err(command_error(&output));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn git_output(repository: Option<&Path>, arguments: &[&str]) -> Result<GitOutput, String> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(repository) = repository {
        command.current_dir(repository);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not run git: {error}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < GIT_TIMEOUT => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git operation timed out after 30 seconds".into());
            }
            Err(error) => return Err(format!("could not wait for git: {error}")),
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_end(&mut stderr);
    }
    Ok(GitOutput {
        status,
        stdout,
        stderr,
    })
}

fn command_error(output: &GitOutput) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    lines
        .iter()
        .find(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("fatal:")
                || lower.starts_with("error:")
                || lower.contains("authentication")
                || lower.contains("permission denied")
                || lower.contains("repository not found")
                || lower.contains("could not")
        })
        .copied()
        .or_else(|| lines.first().copied())
        .unwrap_or("git command failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DATA_VERSION, IdentityStatus, Topic};
    use tempfile::tempdir;

    fn document(name: &str) -> Document {
        Document {
            version: DATA_VERSION,
            topics: vec![Topic {
                name: name.into(),
                status: IdentityStatus::Active,
                items: Vec::new(),
            }],
        }
    }

    fn configure_test_identity(repository: &Path) {
        git(Some(repository), &["config", "user.name", "who-me tests"]).unwrap();
        git(
            Some(repository),
            &["config", "user.email", "who-me-tests@localhost"],
        )
        .unwrap();
    }

    fn seeded_remote(root: &Path, expected: &Document) -> PathBuf {
        let remote = root.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(Some(&remote), &["init", "--bare", "-b", "main"]).unwrap();
        let seed = root.join("seed");
        fs::create_dir(&seed).unwrap();
        git(Some(&seed), &["init", "-b", "main"]).unwrap();
        configure_test_identity(&seed);
        write_repository_document(&seed, expected).unwrap();
        commit_if_changed(&seed).unwrap();
        git(
            Some(&seed),
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )
        .unwrap();
        git(Some(&seed), &["push", "-u", "origin", "main"]).unwrap();
        remote
    }

    #[test]
    fn validates_safe_github_urls() {
        assert!(validate_github_url("https://github.com/person/private").is_ok());
        assert!(validate_github_url("git@github.com:person/private.git").is_ok());
        assert!(validate_github_url("ssh://git@github.com/person/private").is_ok());
        assert!(validate_github_url("https://token@github.com/person/private").is_err());
        assert!(validate_github_url("https://example.com/person/private").is_err());
        assert!(validate_github_url("https://github.com/person").is_err());
    }

    #[test]
    fn config_round_trips_privately() {
        let temp = tempdir().unwrap();
        let paths = SyncPaths::in_directory(temp.path());
        let store = ConfigStore::new(paths.config.clone());
        let config = SyncConfig::new("https://github.com/person/private".into());
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), Some(config));
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn repository_helpers_commit_and_read_documents() {
        let temp = tempdir().unwrap();
        let repository = temp.path().join("repository");
        fs::create_dir(&repository).unwrap();
        git(Some(&repository), &["init", "-b", "main"]).unwrap();
        configure_test_identity(&repository);

        let expected = document("Developer");
        write_repository_document(&repository, &expected).unwrap();
        commit_if_changed(&repository).unwrap();
        assert_eq!(read_worktree_document(&repository).unwrap(), expected);
        assert_eq!(
            read_git_document(&repository, "HEAD:data.toml").unwrap(),
            Some(expected)
        );
    }

    #[test]
    fn legacy_sync_identity_is_removed_without_replacing_custom_identity() {
        let temp = tempdir().unwrap();
        git(Some(temp.path()), &["init", "-b", "main"]).unwrap();
        git(Some(temp.path()), &["config", "user.name", LEGACY_GIT_NAME]).unwrap();
        git(
            Some(temp.path()),
            &["config", "user.email", LEGACY_GIT_EMAIL],
        )
        .unwrap();

        remove_legacy_identity(temp.path()).unwrap();

        assert!(git_text(temp.path(), &["config", "--local", "--get", "user.name"]).is_err());
        assert!(git_text(temp.path(), &["config", "--local", "--get", "user.email"]).is_err());

        configure_test_identity(temp.path());
        remove_legacy_identity(temp.path()).unwrap();
        assert_eq!(
            git_text(temp.path(), &["config", "--local", "--get", "user.name"])
                .unwrap()
                .trim(),
            "who-me tests"
        );
    }

    #[test]
    fn malformed_remote_document_is_rejected() {
        let temp = tempdir().unwrap();
        git(Some(temp.path()), &["init", "-b", "main"]).unwrap();
        configure_test_identity(temp.path());
        fs::write(temp.path().join("data.toml"), "not = [valid").unwrap();
        git(Some(temp.path()), &["add", "--", "data.toml"]).unwrap();
        git(Some(temp.path()), &["commit", "-m", "Invalid data"]).unwrap();

        let error = read_git_document(temp.path(), "HEAD:data.toml").unwrap_err();
        assert!(error.contains("invalid GitHub data"));
    }

    #[test]
    fn conflict_backups_preserve_both_documents() {
        let temp = tempdir().unwrap();
        let directory = temp.path().join("conflicts");
        backup_conflict(&directory, &document("Local"), &document("Remote")).unwrap();
        let contents = fs::read_dir(directory)
            .unwrap()
            .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(contents.len(), 2);
        assert!(contents.iter().any(|source| source.contains("Local")));
        assert!(contents.iter().any(|source| source.contains("Remote")));
    }

    #[test]
    fn service_pushes_to_an_empty_remote() {
        let temp = tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(Some(&remote), &["init", "--bare", "-b", "main"]).unwrap();

        let paths = SyncPaths::in_directory(&temp.path().join("client"));
        let service = SyncService::start(paths);
        let expected = document("Developer");
        service.configure(
            SyncConfig {
                version: CONFIG_VERSION,
                repository: remote.to_string_lossy().into_owned(),
                branch: "main".into(),
            },
            expected.clone(),
            7,
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut accepted = false;
        let mut synced = false;
        while Instant::now() < deadline && !synced {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { document, revision } => {
                        assert_eq!(document, expected);
                        assert_eq!(revision, 7);
                        service.accept_prepared();
                        accepted = true;
                    }
                    SyncEvent::Status(SyncStatus::Synced) => synced = true,
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Offline(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(accepted);
        assert!(synced);

        let verifier = temp.path().join("verifier");
        git(
            None,
            &[
                "clone",
                "--",
                &remote.to_string_lossy(),
                &verifier.to_string_lossy(),
            ],
        )
        .unwrap();
        assert_eq!(read_worktree_document(&verifier).unwrap(), expected);
    }

    #[test]
    fn service_downloads_remote_data_on_first_connection() {
        let temp = tempdir().unwrap();
        let expected = document("From GitHub");
        let remote = seeded_remote(temp.path(), &expected);
        let paths = SyncPaths::in_directory(&temp.path().join("client"));
        let service = SyncService::start(paths);
        service.configure(
            SyncConfig {
                version: CONFIG_VERSION,
                repository: remote.to_string_lossy().into_owned(),
                branch: "main".into(),
            },
            Document::default(),
            3,
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut received = false;
        while Instant::now() < deadline && !received {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { document, revision } => {
                        assert_eq!(document, expected);
                        assert_eq!(revision, 3);
                        service.accept_prepared();
                        received = true;
                    }
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Offline(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(received);
    }

    #[test]
    fn first_connection_conflict_can_keep_local_with_backups() {
        let temp = tempdir().unwrap();
        let remote = seeded_remote(temp.path(), &document("From GitHub"));
        let paths = SyncPaths::in_directory(&temp.path().join("client"));
        let conflicts = paths.conflicts.clone();
        let service = SyncService::start(paths);
        let local = document("Local");
        service.configure(
            SyncConfig {
                version: CONFIG_VERSION,
                repository: remote.to_string_lossy().into_owned(),
                branch: "main".into(),
            },
            local.clone(),
            9,
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut resolving = false;
        let mut synced = false;
        while Instant::now() < deadline && !synced {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Status(SyncStatus::Conflict(_)) if !resolving => {
                        service.resolve(ConflictChoice::KeepLocal, local.clone(), 9);
                        resolving = true;
                    }
                    SyncEvent::Status(SyncStatus::Synced) => synced = true,
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Offline(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(synced);
        assert_eq!(fs::read_dir(conflicts).unwrap().count(), 2);

        let verifier = temp.path().join("verifier");
        git(
            None,
            &[
                "clone",
                "--",
                &remote.to_string_lossy(),
                &verifier.to_string_lossy(),
            ],
        )
        .unwrap();
        assert_eq!(read_worktree_document(&verifier).unwrap(), local);
    }

    #[test]
    fn service_merges_independent_changes_from_two_devices() {
        let temp = tempdir().unwrap();
        let base = Document {
            version: DATA_VERSION,
            topics: vec![
                Topic {
                    name: "Developer".into(),
                    status: IdentityStatus::Active,
                    items: Vec::new(),
                },
                Topic {
                    name: "Writer".into(),
                    status: IdentityStatus::Active,
                    items: Vec::new(),
                },
            ],
        };
        let remote = seeded_remote(temp.path(), &base);
        let paths = SyncPaths::in_directory(&temp.path().join("client"));
        let service = SyncService::start(paths);
        let config = SyncConfig {
            version: CONFIG_VERSION,
            repository: remote.to_string_lossy().into_owned(),
            branch: "main".into(),
        };
        service.configure(config, base.clone(), 0);

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut initially_synced = false;
        while Instant::now() < deadline && !initially_synced {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { .. } => service.accept_prepared(),
                    SyncEvent::Status(SyncStatus::Synced) => initially_synced = true,
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Offline(error))
                    | SyncEvent::Status(SyncStatus::Conflict(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(initially_synced);

        let mut remote_document = base.clone();
        remote_document.topics[1].status = IdentityStatus::Former;
        let seed = temp.path().join("seed");
        write_repository_document(&seed, &remote_document).unwrap();
        commit_if_changed(&seed).unwrap();
        git(Some(&seed), &["push", "origin", "main"]).unwrap();

        let mut local_document = base;
        local_document.topics[0].status = IdentityStatus::Aspiring;
        service.synchronize(local_document, 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut merged = None;
        while Instant::now() < deadline && merged.is_none() {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { document, revision } => {
                        assert_eq!(revision, 1);
                        merged = Some(document);
                    }
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Offline(error))
                    | SyncEvent::Status(SyncStatus::Conflict(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        let merged = merged.expect("sync should produce a merged document");
        assert_eq!(merged.topics[0].status, IdentityStatus::Aspiring);
        assert_eq!(merged.topics[1].status, IdentityStatus::Former);
        service.accept_prepared();
    }

    #[test]
    fn pending_local_data_syncs_after_remote_returns() {
        let temp = tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(Some(&remote), &["init", "--bare", "-b", "main"]).unwrap();
        let service = SyncService::start(SyncPaths::in_directory(&temp.path().join("client")));
        let config = SyncConfig {
            version: CONFIG_VERSION,
            repository: remote.to_string_lossy().into_owned(),
            branch: "main".into(),
        };
        service.configure(config, document("Initial"), 0);

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut initially_synced = false;
        while Instant::now() < deadline && !initially_synced {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { .. } => service.accept_prepared(),
                    SyncEvent::Status(SyncStatus::Synced) => initially_synced = true,
                    SyncEvent::Status(SyncStatus::Error(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(initially_synced);

        let unavailable = temp.path().join("remote-unavailable.git");
        fs::rename(&remote, &unavailable).unwrap();
        let pending = document("Offline edit");
        service.synchronize(pending.clone(), 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut offline = false;
        while Instant::now() < deadline && !offline {
            while let Some(event) = service.try_recv() {
                if matches!(event, SyncEvent::Status(SyncStatus::Offline(_))) {
                    offline = true;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(offline);

        fs::rename(&unavailable, &remote).unwrap();
        service.synchronize(pending.clone(), 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut synced = false;
        while Instant::now() < deadline && !synced {
            while let Some(event) = service.try_recv() {
                match event {
                    SyncEvent::Prepared { document, .. } => {
                        assert_eq!(document, pending);
                        service.accept_prepared();
                    }
                    SyncEvent::Status(SyncStatus::Synced) => synced = true,
                    SyncEvent::Status(SyncStatus::Error(error))
                    | SyncEvent::Status(SyncStatus::Conflict(error)) => panic!("{error}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(synced);
    }
}
