use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 安装数据库中的一条记录：记录版本、依赖关系与文件清单，支撑精确卸载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub keg_only: bool,
    pub installed_at: u64,
    #[serde(default)]
    pub files: Vec<String>,
}

impl Manifest {
    pub fn cellar_path(&self, cfg: &Config) -> PathBuf {
        cfg.cellar().join(&self.name).join(&self.version)
    }
}

pub fn db_file(cfg: &Config, name: &str) -> PathBuf {
    cfg.db_dir().join(format!("{name}.json"))
}

pub fn read_manifest(cfg: &Config, name: &str) -> Result<Option<Manifest>> {
    let p = db_file(cfg, name);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("读取安装记录失败: {}", p.display()))?;
    Ok(Some(serde_json::from_str(&s)?))
}

pub fn all_manifests(cfg: &Config) -> Result<Vec<Manifest>> {
    let mut out = Vec::new();
    if !cfg.db_dir().exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(cfg.db_dir())?.flatten() {
        let p = entry.path();
        if p.extension().map(|e| e == "json").unwrap_or(false) {
            let s = std::fs::read_to_string(&p)?;
            out.push(serde_json::from_str(&s)?);
        }
    }
    Ok(out)
}

pub fn write_manifest(cfg: &Config, m: &Manifest) -> Result<()> {
    std::fs::create_dir_all(cfg.db_dir())?;
    let s = serde_json::to_string_pretty(m)?;
    std::fs::write(db_file(cfg, &m.name), s)?;
    Ok(())
}

pub fn remove_manifest(cfg: &Config, name: &str) -> Result<()> {
    let p = db_file(cfg, name);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// 递归收集 root 下所有文件的相对路径（作为卸载依据的文件清单）
pub fn collect_files(root: &Path) -> Result<Vec<String>> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<()> {
        for e in std::fs::read_dir(dir)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out)?;
            } else {
                out.push(p.strip_prefix(root)?.display().to_string());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

/// 创建 opt/<name> -> ../Cellar/<name>/<version> 符号链接（keg-only 也会创建）
pub fn create_opt_link(cfg: &Config, name: &str, version: &str) -> Result<()> {
    std::fs::create_dir_all(cfg.opt())?;
    let link = cfg.opt().join(name);
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(format!("../Cellar/{name}/{version}"), &link)?;
    Ok(())
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
