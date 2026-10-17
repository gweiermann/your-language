//! Shared project discovery for the CLI and editor. Paths are relative to `yl.json`.
mod runner;
use crate::diagnostic::{CompileResult, Diagnostic, Span};
use regex::{Regex, RegexBuilder};
pub use runner::{run_example, ProjectReport, SourceReport};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub entry: String,
    pub files: Vec<String>,
    #[serde(default, rename = "expectErrors")]
    pub expect_errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Project {
    pub config_path: PathBuf,
    pub entry: PathBuf,
    patterns: Vec<Regex>,
    error_patterns: Vec<Regex>,
}

pub(crate) fn absolute(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn invalid(path: &Path, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        "yl.project",
        message,
        Span::new(&path.to_string_lossy(), 0, 0),
    )]
}

impl Project {
    pub fn parse(path: &Path, source: &str) -> CompileResult<Self> {
        let path = absolute(path);
        let config: ProjectConfig = serde_json::from_str(source)
            .map_err(|error| invalid(&path, format!("Invalid yl.json: {error}")))?;
        if config.entry.is_empty()
            || Path::new(&config.entry).is_absolute()
            || config.entry.contains('\\')
        {
            return Err(invalid(
                &path,
                "entry must be a nonempty relative path using / separators",
            ));
        }
        if config.files.is_empty() {
            return Err(invalid(
                &path,
                "files must contain at least one project-relative glob",
            ));
        }
        let patterns = compile_globs(&path, config.files)?;
        let error_patterns = compile_globs(&path, config.expect_errors)?;
        let entry = absolute(&path.parent().unwrap_or(Path::new(".")).join(config.entry));
        Ok(Self {
            config_path: path,
            entry,
            patterns,
            error_patterns,
        })
    }

    /// Load an explicit configuration, or `yl.json` in a directory.
    pub fn load(path: &Path) -> CompileResult<Self> {
        let path = if path.is_dir() {
            path.join("yl.json")
        } else {
            path.to_path_buf()
        };
        let text = std::fs::read_to_string(&path).map_err(|e| invalid(&path, e.to_string()))?;
        Self::parse(&path, &text)
    }

    /// Find configurations recursively in stable path order, without following symlinks.
    pub fn list(directory: &Path) -> CompileResult<Vec<Self>> {
        let paths = walk_files(directory)?;
        paths
            .into_iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "yl.json"))
            .map(|p| Self::load(&p))
            .collect()
    }

    /// Matching source files owned by this nearest project configuration.
    pub fn sources(&self) -> CompileResult<Vec<PathBuf>> {
        let root = self.config_path.parent().unwrap_or(Path::new("."));
        let mut result = Vec::new();
        for source in walk_files(root)? {
            if self.matches(&source)
                && Self::discover(&source)?.is_some_and(|p| p.config_path == self.config_path)
            {
                result.push(source);
            }
        }
        Ok(result)
    }

    pub fn expects_errors(&self, source: &Path) -> bool {
        let source = absolute(source);
        let root = self.config_path.parent().unwrap_or(Path::new("."));
        source.strip_prefix(root).is_ok_and(|relative| {
            let relative = relative.to_string_lossy().replace('\\', "/");
            self.error_patterns.iter().any(|p| p.is_match(&relative))
        })
    }

    /// The nearest configuration is authoritative, including when it is invalid.
    pub fn discover(source: &Path) -> CompileResult<Option<Self>> {
        Self::discover_with(source, |p| std::fs::read_to_string(p).ok())
    }

    pub(crate) fn discover_with(
        source: &Path,
        read: impl Fn(&Path) -> Option<String>,
    ) -> CompileResult<Option<Self>> {
        let source = absolute(source);
        for directory in source.parent().into_iter().flat_map(Path::ancestors) {
            let path = directory.join("yl.json");
            if let Some(text) = read(&path) {
                return Self::parse(&path, &text).map(Some);
            }
            // Do not silently fall through an existing but unreadable configuration.
            if path.exists() {
                return Err(invalid(&path, "Cannot read yl.json"));
            }
        }
        Ok(None)
    }

    pub fn matches(&self, source: &Path) -> bool {
        let source = absolute(source);
        let root = self.config_path.parent().unwrap_or(Path::new("."));
        let Ok(relative) = source.strip_prefix(root) else {
            return false;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        self.patterns
            .iter()
            .any(|pattern| pattern.is_match(&relative))
    }
}

fn compile_globs(path: &Path, globs: Vec<String>) -> CompileResult<Vec<Regex>> {
    let mut patterns = Vec::new();
    for pattern in globs {
        if pattern.is_empty()
            || pattern.starts_with('/')
            || pattern.contains(['\\', ':', '[', ']', '{', '}'])
            || pattern.split('/').any(|p| p == "..")
        {
            return Err(invalid(
                path,
                "Project globs must be relative and use /, *, ** or ? (no parent traversal)",
            ));
        }
        let mut regex = String::from("^");
        let mut chars = pattern
            .strip_prefix("./")
            .unwrap_or(&pattern)
            .chars()
            .peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        chars.next();
                        regex.push_str("(?:.*/)?");
                    } else {
                        regex.push_str(".*");
                    }
                }
                '*' => regex.push_str("[^/]*"),
                '?' => regex.push_str("[^/]"),
                ch => regex.push_str(&regex::escape(&ch.to_string())),
            }
        }
        regex.push('$');
        patterns.push(
            RegexBuilder::new(&regex)
                .case_insensitive(cfg!(windows))
                .build()
                .map_err(|e| invalid(path, e.to_string()))?,
        );
    }
    Ok(patterns)
}

fn walk_files(directory: &Path) -> CompileResult<Vec<PathBuf>> {
    let directory = absolute(directory);
    let mut pending = vec![directory];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries =
            std::fs::read_dir(&directory).map_err(|e| invalid(&directory, e.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|e| invalid(&directory, e.to_string()))?;
            let kind = entry
                .file_type()
                .map_err(|e| invalid(&entry.path(), e.to_string()))?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "target" | "node_modules")
                ) {
                    pending.push(entry.path());
                }
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}
