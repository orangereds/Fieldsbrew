use crate::cellar;
use crate::config::Config;
use crate::link;
use anyhow::{Context, Result};

pub fn link_names(cfg: &Config, names: &[String], force: bool) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("请至少指定一个要链接的包名");
    }
    for name in names {
        let m = cellar::read_manifest(cfg, name)?
            .with_context(|| format!("{name} 未安装"))?;
        if m.keg_only && !force {
            anyhow::bail!(
                "{name} 是 keg-only，默认不链接以免覆盖系统同名文件；\n\
                 如确认要链接，请加 --force"
            );
        }
        let report = link::link_keg(cfg, &m.name, &m.version, force)?;
        println!(
            "==> 已链接 {name} {}：{} 个文件",
            m.version, report.linked
        );
        if !report.conflicts.is_empty() {
            eprintln!(
                "警告: {} 个文件因已存在而未链接（可加 --force 替换）:\n  {}",
                report.conflicts.len(),
                report
                    .conflicts
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            );
        }
    }
    Ok(())
}

pub fn unlink_names(cfg: &Config, names: &[String]) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("请至少指定一个要取消链接的包名");
    }
    for name in names {
        let m = cellar::read_manifest(cfg, name)?
            .with_context(|| format!("{name} 未安装"))?;
        let removed = link::unlink_keg(cfg, &m.name, &m.version)?;
        println!("==> 已取消 {name} 的 {removed} 个链接");
    }
    Ok(())
}
