use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::article::Article;
use crate::dialog::Dialog;
use crate::entry::Entry;
use crate::manifest::Manifest;
use crate::SCHEMA_VERSION;

#[derive(Debug)]
pub enum PackError {
    Io(PathBuf, std::io::Error),
    Json(PathBuf, serde_json::Error),
    SchemaMismatch { found: u32, expected: u32 },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::Io(p, e) => write!(f, "读取 {} 失败: {e}", p.display()),
            PackError::Json(p, e) => write!(f, "解析 {} 失败: {e}", p.display()),
            PackError::SchemaMismatch { found, expected } => {
                write!(
                    f,
                    "pack schema 版本不兼容: 包内 {found}，程序要求 {expected}"
                )
            }
        }
    }
}

impl std::error::Error for PackError {}

/// 一个已加载的内容包。目录只读，资产按需读取。
#[derive(Debug, Clone)]
pub struct Pack {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub words: Vec<Entry>,
    pub sentences: Vec<Entry>,
    pub articles: Vec<Article>,
    pub dialogs: Vec<Dialog>,
}

impl Pack {
    /// 从目录加载。缺省的分块文件视为空表。
    pub fn load(root: impl AsRef<Path>) -> Result<Self, PackError> {
        let root = root.as_ref().to_path_buf();

        let manifest: Manifest = read_json(&root.join("manifest.json"))?;
        if manifest.schema != SCHEMA_VERSION {
            return Err(PackError::SchemaMismatch {
                found: manifest.schema,
                expected: SCHEMA_VERSION,
            });
        }

        Ok(Self {
            words: read_optional(&root.join("words.json"))?,
            sentences: read_optional(&root.join("sentences.json"))?,
            articles: read_optional(&root.join("articles.json"))?,
            dialogs: read_optional(&root.join("dialogs.json"))?,
            manifest,
            root,
        })
    }

    pub fn lang(&self) -> &str {
        &self.manifest.lang
    }

    /// 词条 + 句子，按 id 线性查找。单包规模在万级以内，够用。
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.words
            .iter()
            .chain(self.sentences.iter())
            .find(|e| e.id == id)
    }

    pub fn article(&self, id: &str) -> Option<&Article> {
        self.articles.iter().find(|a| a.id == id)
    }

    pub fn dialog(&self, id: &str) -> Option<&Dialog> {
        self.dialogs.iter().find(|d| d.id == id)
    }

    /// 素材路径解析（相对 pack 根目录）。
    pub fn asset(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// 内容指纹：manifest.json 之外所有文件按相对路径排序后逐字节吃进 sha256。
    /// 用来判断「用户手改过 pack」或「导入包与登记的不一致」。
    pub fn content_hash(&self) -> std::io::Result<String> {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_files(&self.root, &mut files)?;
        files.retain(|p| {
            p.strip_prefix(&self.root)
                .map(|r| r != Path::new("manifest.json"))
                .unwrap_or(false)
        });
        files.sort_by(|a, b| {
            let ra = a.strip_prefix(&self.root).unwrap_or(a);
            let rb = b.strip_prefix(&self.root).unwrap_or(b);
            ra.cmp(rb)
        });

        let mut hasher = Sha256::new();
        for path in files {
            let rel = path.strip_prefix(&self.root).unwrap_or(&path);
            hasher.update(rel.to_string_lossy().as_bytes());
            hasher.update([0]);
            hasher.update(std::fs::read(&path)?);
            hasher.update([0]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, PackError> {
    let bytes = std::fs::read(path).map_err(|e| PackError::Io(path.to_path_buf(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| PackError::Json(path.to_path_buf(), e))
}

fn read_optional<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, PackError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).map_err(|e| PackError::Io(path.to_path_buf(), e))?;
    // 允许两种形状：顶层数组，或 { "items": [...] }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|e| PackError::Json(path.to_path_buf(), e))?;
    match value {
        Value::Array(_) => {
            serde_json::from_value(value).map_err(|e| PackError::Json(path.to_path_buf(), e))
        }
        Value::Object(mut map) => match map.remove("items") {
            Some(Value::Array(items)) => serde_json::from_value(Value::Array(items))
                .map_err(|e| PackError::Json(path.to_path_buf(), e)),
            _ => Ok(Vec::new()),
        },
        _ => Ok(Vec::new()),
    }
}
