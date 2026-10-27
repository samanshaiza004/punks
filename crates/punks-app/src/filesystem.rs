use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg"];

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone)]
pub struct DirListing {
    pub root: PathBuf,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    NotADirectory,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::NotADirectory => write!(f, "path is not a directory"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScanError::Io(e) => Some(e),
            ScanError::NotADirectory => None,
        }
    }
}

impl From<io::Error> for ScanError {
    fn from(e: io::Error) -> Self {
        ScanError::Io(e)
    }
}

pub fn list_directory(dir: &Path) -> Result<DirListing, ScanError> {
    if !dir.is_dir() {
        return Err(ScanError::NotADirectory);
    }

    let mut dirs: Vec<FileEntry> = Vec::new();
    let mut files: Vec<FileEntry> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with('.') {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let path = entry.path();

        if metadata.is_dir() {
            dirs.push(FileEntry {
                path,
                name,
                extension: String::new(),
                size_bytes: 0,
                is_directory: true,
            });
        } else if metadata.is_file() {
            let ext = path
                .extension()
                .and_then(OsStr::to_str)
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                files.push(FileEntry {
                    name,
                    extension: ext,
                    size_bytes: metadata.len(),
                    path,
                    is_directory: false,
                });
            }
        }
    }

    let ci_sort = |a: &FileEntry, b: &FileEntry| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    };
    dirs.sort_by(ci_sort);
    files.sort_by(ci_sort);

    let mut entries = dirs;
    entries.extend(files);

    Ok(DirListing {
        root: dir.to_path_buf(),
        entries,
    })
}

pub fn search_directory(
    root: &Path,
    query: &str,
    extensions: &[&str],
) -> Result<Vec<FileEntry>, ScanError> {
    if !root.is_dir() {
        return Err(ScanError::NotADirectory);
    }

    let query_lower = query.to_ascii_lowercase();
    let ext_lower: Vec<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();

    let mut results = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_path_buf());

    while let Some(current) = queue.pop_front() {
        let entries = match std::fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') {
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            let path = entry.path();

            if file_type.is_dir() {
                queue.push_back(path);
                continue;
            }

            if !name.to_ascii_lowercase().contains(&query_lower) {
                continue;
            }

            let ext = path
                .extension()
                .and_then(OsStr::to_str)
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !ext_lower.is_empty() && !ext_lower.contains(&ext) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            results.push(FileEntry {
                name,
                extension: ext,
                size_bytes: metadata.len(),
                path,
                is_directory: false,
            });
        }
    }

    results.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    Ok(results)
}
