use crate::cellar;
use crate::config::Config;
use crate::link;
use anyhow::{Context, Result};

pub fn run(cfg: &Config, names: &[String], force: bool) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("请至少指定一个要卸载的包名");
    }
    for name in names {
        uninstall_one(cfg, name, force)?;
    }
    Ok(())
}

fn uninstall_one(cfg: &Config, name: &str, force: bool) -> Result<()> {
    let m = cellar::read_manifest(cfg, name)?.with_context(|| format!("{name} 未安装"))?;

    // 依赖检查：先找出还依赖它的已安装包
    let mut dependents = Vec::new();
    for other in cellar::all_manifests(cfg)? {
        if other.name != m.name && other.dependencies.iter().any(|d| d == name) {
            dependents.push(other.name);
        }
    }
    if !dependents.is_empty() && !force {
        anyhow::bail!(
            "无法卸载 {name}：以下已安装包依赖它: {}\n可加 --force 强制卸载",
            dependents.join(", ")
        );
    }
    if !dependents.is_empty() {
        println!("警告: {} 仍依赖 {name}，已按 --force 卸载", dependents.join(", "));
    }

    println!("==> 卸载 {} {} ({})", m.name, m.version, m.cellar_path(cfg).display());
    remove_keg(cfg, &m)?;
    println!("==> 已卸载 {name}");
    Ok(())
}

/// 移除某个已安装版本：prefix 链接、Cellar 目录与 manifest。
/// reinstall 也会复用该函数（此时不做反向依赖检查）。
pub fn remove_keg(cfg: &Config, m: &cellar::Manifest) -> Result<()> {
    // 只删除指向该版本 keg 的符号链接
    link::unlink_keg(cfg, &m.name, &m.version)?;

    // opt 链接：该包只有一条 manifest，卸载时直接移除符号链接
    let opt = cfg.opt().join(&m.name);
    if opt.symlink_metadata().map(|s| s.file_type().is_symlink()).unwrap_or(false) {
        std::fs::remove_file(&opt)?;
    }

    let cellar_path = m.cellar_path(cfg);
    if cellar_path.exists() {
        std::fs::remove_dir_all(&cellar_path)
            .with_context(|| format!("删除目录失败: {}", cellar_path.display()))?;
    }
    let pkg_dir = cfg.cellar().join(&m.name);
    if pkg_dir.is_dir() && std::fs::read_dir(&pkg_dir)?.next().is_none() {
        std::fs::remove_dir(&pkg_dir)?;
    }
    cellar::remove_manifest(cfg, &m.name)?;
    Ok(())
}
