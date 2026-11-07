use crate::config::Config;
use anyhow::{Context, Result};
use std::path::{Component, Path, PathBuf};

/// 参与 prefix 链接的顶层目录（与 Homebrew 的链接范围一致）
pub const LINK_DIRS: &[&str] = &["bin", "sbin", "lib", "include", "share", "etc", "libexec"];

#[derive(Debug, Default)]
pub struct LinkReport {
    pub linked: usize,
    pub conflicts: Vec<PathBuf>,
}

/// 把某个 keg 的文件链接到 prefix 下，语义对齐 `brew link`：
/// - 只有符号链接会被覆盖（通常是同名包的其他版本）
/// - 普通文件默认不覆盖，记为冲突；force=true 时才替换
/// - keg 内的隐藏目录（如 .brew）不链接
pub fn link_keg(cfg: &Config, name: &str, version: &str, force: bool) -> Result<LinkReport> {
    let keg = cfg.cellar().join(name).join(version);
    if !keg.is_dir() {
        anyhow::bail!("keg 不存在: {}", keg.display());
    }
    let mut report = LinkReport::default();
    for top in LINK_DIRS {
        let dir = keg.join(top);
        if dir.is_dir() {
            link_tree(cfg, &keg, &dir, force, &mut report)?;
        }
    }
    Ok(report)
}

fn link_tree(
    cfg: &Config,
    keg: &Path,
    dir: &Path,
    force: bool,
    report: &mut LinkReport,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            // 跳过 Homebrew bottle 的 .brew 元数据目录
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            link_tree(cfg, keg, &path, force, report)?;
            continue;
        }
        let rel = path.strip_prefix(keg)?.to_path_buf();
        let link = cfg.prefix.join(&rel);
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if link.symlink_metadata().is_ok() {
            let meta = link.symlink_metadata()?;
            if meta.file_type().is_symlink() {
                if symlink_points_to(&link, &path) {
                    report.linked += 1;
                    continue;
                }
                // 指向其他位置的符号链接（通常是旧版本）直接替换
                std::fs::remove_file(&link)
                    .with_context(|| format!("移除旧链接失败: {}", link.display()))?;
            } else if meta.is_dir() {
                report.conflicts.push(link.clone());
                continue;
            } else if force {
                std::fs::remove_file(&link)
                    .with_context(|| format!("移除冲突文件失败: {}", link.display()))?;
            } else {
                report.conflicts.push(link.clone());
                continue;
            }
        }

        let target = relative_path(
            link.parent().expect("链接必须有父目录"),
            &path,
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)
            .with_context(|| format!("创建链接失败: {}", link.display()))?;
        report.linked += 1;
    }
    Ok(())
}

/// 删除所有指向指定 keg 的 prefix 链接，返回删除数量
pub fn unlink_keg(cfg: &Config, name: &str, version: &str) -> Result<usize> {
    let keg = cfg.cellar().join(name).join(version);
    if !keg.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for top in LINK_DIRS {
        let dir = keg.join(top);
        if dir.is_dir() {
            removed += unlink_tree(cfg, &keg, &dir)?;
        }
    }
    Ok(removed)
}

fn unlink_tree(cfg: &Config, keg: &Path, dir: &Path) -> Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            removed += unlink_tree(cfg, keg, &path)?;
            continue;
        }
        let rel = path.strip_prefix(keg)?;
        let link = cfg.prefix.join(rel);
        if link.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false)
            && symlink_points_to(&link, &path)
        {
            std::fs::remove_file(&link)
                .with_context(|| format!("移除链接失败: {}", link.display()))?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// 判断 link 是否指向 expected（优先 canonicalize，失败时退回路径字符串判断）
fn symlink_points_to(link: &Path, expected: &Path) -> bool {
    if let Ok(target) = std::fs::canonicalize(link) {
        if let Ok(want) = std::fs::canonicalize(expected) {
            return target == want;
        }
    }
    match std::fs::read_link(link) {
        Ok(t) => {
            let abs = link
                .parent()
                .map(|p| p.join(&t))
                .unwrap_or_else(|| t.clone());
            let s = abs.to_string_lossy();
            let want = expected.to_string_lossy();
            s == want
        }
        Err(_) => false,
    }
}

/// 计算从 from 目录到 to 的绝对路径之间的相对路径，生成 Homebrew 风格的相对链接
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let mut common = 0;
    while common < from.len() && common < to.len() && from[common] == to[common] {
        common += 1;
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to[common..] {
        out.push(c.as_os_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::relative_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_relative_path() {
        assert_eq!(
            relative_path(Path::new("/opt/fieldsbrew/bin"), Path::new("/opt/fieldsbrew/Cellar/hello/1.0/bin/hello")),
            PathBuf::from("../Cellar/hello/1.0/bin/hello")
        );
        assert_eq!(
            relative_path(
                Path::new("/opt/fieldsbrew/share/man/man1"),
                Path::new("/opt/fieldsbrew/Cellar/hello/1.0/share/man/man1/hello.1")
            ),
            PathBuf::from("../../../Cellar/hello/1.0/share/man/man1/hello.1")
        );
    }
}
