use anyhow::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub extension: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    pub root: PathBuf,
    pub files: HashMap<PathBuf, FileInfo>,
    pub total_files: usize,
    pub total_size: u64,
}

impl RepoIndex {
    pub fn scan(root: &Path) -> Result<Self> {
        let mut files = HashMap::new();
        let mut total_size = 0u64;

        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let path = entry.path().to_path_buf();
                let metadata = std::fs::metadata(&path)?;
                let size = metadata.len();
                let extension = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_string());

                let language = extension.as_deref().and_then(lang_from_ext);

                total_size += size;
                files.insert(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_path_buf(),
                    FileInfo {
                        path: path.clone(),
                        size,
                        extension,
                        language,
                    },
                );
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            total_files: files.len(),
            total_size,
            files,
        })
    }

    pub fn search(&self, query: &str) -> Vec<&FileInfo> {
        let query_lower = query.to_lowercase();
        self.files
            .values()
            .filter(|f| {
                f.path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query_lower)
            })
            .collect()
    }

    pub fn file_tree(&self) -> Vec<&PathBuf> {
        let mut paths: Vec<_> = self.files.keys().collect();
        paths.sort();
        paths
    }

    pub fn file_content(&self, relative: &Path) -> Result<String> {
        let info = self
            .files
            .get(relative)
            .ok_or_else(|| anyhow::anyhow!("file not found in index"))?;
        Ok(std::fs::read_to_string(&info.path)?)
    }

    pub fn stats(&self) -> RepoStats {
        let mut lang_counts: HashMap<String, usize> = HashMap::new();
        for f in self.files.values() {
            if let Some(lang) = &f.language {
                *lang_counts.entry(lang.clone()).or_insert(0) += 1;
            }
        }
        RepoStats {
            total_files: self.total_files,
            total_size: self.total_size,
            languages: lang_counts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    pub total_files: usize,
    pub total_size: u64,
    pub languages: HashMap<String, usize>,
}

fn lang_from_ext(ext: &str) -> Option<String> {
    let lang = match ext {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "jsx" | "mjs" => "JavaScript",
        "ts" | "tsx" | "mts" => "TypeScript",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "rb" => "Ruby",
        "toml" | "yaml" | "yml" | "json" | "json5" => "Config",
        "md" | "txt" => "Text",
        "sh" | "bash" | "zsh" => "Shell",
        "html" | "htm" => "HTML",
        "css" | "scss" | "less" => "CSS",
        _ => return None,
    };
    Some(lang.into())
}
