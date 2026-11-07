use crate::cellar;
use crate::config::Config;
use crate::link;
use anyhow::Result;
use std::collections::HashMap;

pub fn run(cfg: &Config) -> Result<()> {
    cfg.ensure_dirs()?;
    let current: HashMap<String, String> = cellar::all_manifests(cfg)?
        .into_iter()
        .map(|m| (m.name, m.version))
        .collect();

    let mut removed = 0;
    if cfg.cellar().is_dir() {
        for pkg in std::fs::read_dir(cfg.cellar())?.flatten() {
            let pkg_dir = pkg.path();
            if !pkg_dir.is_dir() {
                continue;
            }
            let name = pkg.file_name().to_string_lossy().to_string();
            let keep = current.get(&name).cloned();
            for ver in std::fs::read_dir(&pkg_dir)?.flatten() {
                let ver_dir = ver.path();
                if !ver_dir.is_dir() {
                    continue;
                }
                let version = ver.file_name().to_string_lossy().to_string();
                if keep.as_deref() == Some(version.as_str()) {
                    continue;
                }
                let _ = link::unlink_keg(cfg, &name, &version);
                std::fs::remove_dir_all(&ver_dir)?;
                println!("移除旧版本: {name} {version}");
                removed += 1;
            }
            if std::fs::read_dir(&pkg_dir)?.next().is_none() {
                std::fs::remove_dir(&pkg_dir)?;
            }
        }
    }

    // 清理悬空的 opt 链接（目标已不存在）
    if cfg.opt().is_dir() {
        for entry in std::fs::read_dir(cfg.opt())?.flatten() {
            let p = entry.path();
            if p.symlink_metadata().map(|s| s.file_type().is_symlink()).unwrap_or(false)
                && std::fs::metadata(&p).is_err()
            {
                std::fs::remove_file(&p)?;
                println!("移除悬空链接: {}", p.display());
            }
        }
    }

    // 清理构建临时目录
    if cfg.tmp().is_dir() {
        for entry in std::fs::read_dir(cfg.tmp())?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    println!("==> 清理完成，移除 {removed} 个旧版本");
    Ok(())
}
