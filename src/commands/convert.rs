use crate::cask::Cask;
use crate::config::Config;
use crate::convert::{self, Kind};
use crate::formula::Formula;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// `fb convert <path>`：把 Homebrew Ruby formula/cask 转成 Fieldsbrew TOML。
/// 目录模式默认写入 `<prefix>/Formula` 或 `<prefix>/Casks`，绝不写回 Homebrew 目录。
pub fn run(cfg: &Config, path: &Path, out: Option<&Path>, force_cask: bool) -> Result<()> {
    if !path.exists() {
        bail!("路径不存在: {}", path.display());
    }
    if path.is_dir() {
        convert_dir(cfg, path, out, force_cask)
    } else {
        convert_file(path, out, force_cask)
    }
}

fn convert_file(path: &Path, out: Option<&Path>, force_cask: bool) -> Result<()> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("读取文件失败: {}", path.display()))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("formula");
    let c = if force_cask {
        convert::convert_as(&src, stem, Kind::Cask)
    } else {
        convert::convert(&src, stem)
    }
    .with_context(|| format!("转换失败: {}", path.display()))?;
    validate(&c)?;
    report_warnings(&c, path);

    match out {
        Some(dir) => {
            warn_if_homebrew(dir);
            std::fs::create_dir_all(dir)?;
            let target = dir.join(format!("{}.toml", c.name));
            std::fs::write(&target, &c.toml)?;
            println!("已写入 {}", target.display());
        }
        None => {
            println!("{}", c.toml);
        }
    }
    Ok(())
}

fn convert_dir(cfg: &Config, dir: &Path, out: Option<&Path>, force_cask: bool) -> Result<()> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "rb").unwrap_or(false))
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("目录中没有 .rb 文件: {}", dir.display());
    }
    if let Some(o) = out {
        warn_if_homebrew(o);
    }

    let mut ok = 0;
    let mut failed = 0;
    for file in &files {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("formula");
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("跳过 {}: 读取失败: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let c = match if force_cask {
            convert::convert_as(&src, stem, Kind::Cask)
        } else {
            convert::convert(&src, stem)
        } {
            Ok(c) => c,
            Err(e) => {
                eprintln!("跳过 {}: {e:#}", file.display());
                failed += 1;
                continue;
            }
        };
        if let Err(e) = validate(&c) {
            eprintln!("跳过 {}: {e:#}", file.display());
            failed += 1;
            continue;
        }
        report_warnings(&c, file);

        let target_dir = out
            .map(Path::to_path_buf)
            .unwrap_or_else(|| default_out(cfg, c.kind));
        std::fs::create_dir_all(&target_dir)?;
        let target = target_dir.join(format!("{}.toml", c.name));
        std::fs::write(&target, &c.toml)?;
        println!("已转换 {:<28} -> {}", file.display(), target.display());
        ok += 1;
    }
    println!("==> 完成: {ok} 个成功, {failed} 个失败");
    if failed > 0 {
        bail!("{failed} 个文件转换失败");
    }
    Ok(())
}

fn default_out(cfg: &Config, kind: Kind) -> PathBuf {
    match kind {
        Kind::Formula => cfg.prefix.join("Formula"),
        Kind::Cask => cfg.prefix.join("Casks"),
    }
}

fn validate(c: &convert::Converted) -> Result<()> {
    match c.kind {
        Kind::Formula => {
            Formula::parse(&c.toml).context("生成的 TOML 无法被 formula 解析器读取")?;
        }
        Kind::Cask => {
            Cask::parse(&c.toml).context("生成的 TOML 无法被 cask 解析器读取")?;
        }
    }
    Ok(())
}

fn report_warnings(c: &convert::Converted, path: &Path) {
    for w in &c.warnings {
        eprintln!("    警告 {}: {w}", path.display());
    }
}

/// 避免误把转换结果写进 Homebrew 目录（可能被 brew 覆盖）
fn warn_if_homebrew(out: &Path) {
    if let Some(brew) = crate::homebrew::prefix() {
        if out.starts_with(&brew) {
            eprintln!(
                "警告: 输出目录 {} 位于 Homebrew 前缀内，建议改用 Fieldsbrew 目录",
                out.display()
            );
        }
    }
}
