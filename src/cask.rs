use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Cask（GUI/二进制应用）的 TOML 描述。
/// ```toml
/// [package]
/// token = "firefox"
/// name = "Mozilla Firefox"
/// version = "132.0"
/// description = ""
/// homepage = ""
///
/// [source]
/// url = "https://example.com/Firefox.dmg"
/// sha256 = ""
///
/// [[artifacts]]
/// type = "app"       # app | binary | font | pkg
/// path = "Firefox.app"
/// # target 可选；app 默认安装到 /Applications，binary 默认到 <prefix>/bin
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct CaskPackage {
    pub token: String,
    #[serde(default)]
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaskSource {
    pub url: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cask {
    pub package: CaskPackage,
    pub source: CaskSource,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

impl Cask {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("读取 cask 失败: {}", path.display()))?;
        Self::parse(&s).with_context(|| format!("解析 cask 失败: {}", path.display()))
    }

    pub fn display_name(&self) -> &str {
        if self.package.name.is_empty() {
            &self.package.token
        } else {
            &self.package.name
        }
    }
}

/// 按 token 查找 cask 定义
pub fn find_cask(cfg: &Config, token: &str) -> Result<PathBuf> {
    let mut searched = Vec::new();
    for dir in cfg.cask_dirs() {
        let p = dir.join(format!("{token}.toml"));
        if p.exists() {
            return Ok(p);
        }
        searched.push(dir.display().to_string());
    }
    anyhow::bail!(
        "找不到 cask `{token}`，已搜索: {}",
        if searched.is_empty() {
            "（无搜索目录）".to_string()
        } else {
            searched.join(", ")
        }
    )
}

/// 加载所有 cask（按 token 去重，先出现的目录优先）
pub fn all_casks(cfg: &Config) -> Result<Vec<Cask>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in cfg.cask_dirs() {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            if p.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Ok(c) = Cask::load(&p) {
                    if seen.insert(c.package.token.clone()) {
                        out.push(c);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.package.token.cmp(&b.package.token));
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledArtifact {
    /// app / binary / font
    pub kind: String,
    /// 实际落盘路径（绝对路径字符串）
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaskManifest {
    pub token: String,
    pub name: String,
    pub version: String,
    pub installed_at: u64,
    #[serde(default)]
    pub artifacts: Vec<InstalledArtifact>,
}

pub fn manifest_path(cfg: &Config, token: &str) -> PathBuf {
    cfg.cask_db_dir().join(format!("{token}.json"))
}

pub fn read_manifest(cfg: &Config, token: &str) -> Result<Option<CaskManifest>> {
    let p = manifest_path(cfg, token);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("读取 cask 安装记录失败: {}", p.display()))?;
    Ok(Some(serde_json::from_str(&s)?))
}

pub fn write_manifest(cfg: &Config, m: &CaskManifest) -> Result<()> {
    std::fs::create_dir_all(cfg.cask_db_dir())?;
    let s = serde_json::to_string_pretty(m)?;
    std::fs::write(manifest_path(cfg, &m.token), s)?;
    Ok(())
}

pub fn remove_manifest(cfg: &Config, token: &str) -> Result<()> {
    let p = manifest_path(cfg, token);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

pub fn all_manifests(cfg: &Config) -> Result<Vec<CaskManifest>> {
    let mut out = Vec::new();
    if !cfg.cask_db_dir().exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(cfg.cask_db_dir())?.flatten() {
        let p = entry.path();
        if p.extension().map(|e| e == "json").unwrap_or(false) {
            let s = std::fs::read_to_string(&p)?;
            out.push(serde_json::from_str(&s)?);
        }
    }
    out.sort_by(|a, b| a.token.cmp(&b.token));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[package]
token = "demo"
name = "Demo App"
version = "1.0"
description = "demo"

[source]
url = "https://example.com/demo.zip"
sha256 = "abc"

[[artifacts]]
type = "app"
path = "Demo.app"

[[artifacts]]
type = "binary"
path = "demo"
target = "demo-cli"
"#;

    #[test]
    fn test_parse_cask() {
        let c = Cask::parse(SAMPLE).unwrap();
        assert_eq!(c.package.token, "demo");
        assert_eq!(c.display_name(), "Demo App");
        assert_eq!(c.artifacts.len(), 2);
        assert_eq!(c.artifacts[1].target, "demo-cli");
    }
}
