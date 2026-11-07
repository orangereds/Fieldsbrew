use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::process::Command;

/// `fb tap` 入口：
/// - 无参数：列出已添加的 taps
/// - `--remove`：移除指定 tap
/// - 其他：克隆 `user/repo`（默认映射到 GitHub 的 `user/homebrew-repo`）
pub fn run(cfg: &Config, name: Option<&str>, url: Option<&str>, remove: bool) -> Result<()> {
    match name {
        None => list(cfg),
        Some(n) if remove => untap(cfg, n),
        Some(n) => add(cfg, n, url),
    }
}

fn add(cfg: &Config, name: &str, url: Option<&str>) -> Result<()> {
    let (user, repo, remote) = match url {
        Some(u) => {
            let (user, repo) = parse_tap_name(name)?;
            let remote = if u.contains("://") || u.starts_with("git@") {
                u.to_string()
            } else {
                format!("https://github.com/{user}/{repo}.git")
            };
            (user, repo, remote)
        }
        None if name.contains("://") || name.starts_with("git@") => {
            let (user, repo) = parse_remote(name)?;
            (user, repo, name.to_string())
        }
        None => {
            let (user, repo) = parse_tap_name(name)?;
            let remote = if repo.starts_with("homebrew-") {
                format!("https://github.com/{user}/{repo}.git")
            } else {
                format!("https://github.com/{user}/homebrew-{repo}.git")
            };
            (user, repo, remote)
        }
    };

    let dir = cfg.taps_root().join(&user).join(&repo);
    if dir.exists() {
        println!("==> tap 已存在: {user}/{}", display_repo(&repo));
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("==> 克隆 {remote}");
    let status = Command::new("git")
        .args(["clone", &remote])
        .arg(&dir)
        .status()
        .with_context(|| "无法执行 git clone（请确认已安装 git）")?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        bail!("克隆 tap 失败: {remote}");
    }

    let count = count_formulas(&dir);
    println!("==> 已添加 tap: {user}/{}（{count} 个 formula）", display_repo(&repo));
    if count == 0 {
        println!("    提示: tap 中未找到 Formula/*.toml、HomebrewFormula/*.toml 或根目录 *.toml");
    }
    Ok(())
}

fn untap(cfg: &Config, name: &str) -> Result<()> {
    let (user, repo) = parse_tap_name(name)?;
    let dir = cfg.taps_root().join(&user).join(&repo);
    if !dir.exists() {
        bail!("未添加该 tap: {user}/{}", display_repo(&repo));
    }
    std::fs::remove_dir_all(&dir)
        .with_context(|| format!("移除 tap 失败: {}", dir.display()))?;
    let user_dir = cfg.taps_root().join(&user);
    if user_dir.is_dir() && std::fs::read_dir(&user_dir)?.next().is_none() {
        let _ = std::fs::remove_dir(&user_dir);
    }
    println!("==> 已移除 tap: {user}/{}", display_repo(&repo));
    Ok(())
}

fn list(cfg: &Config) -> Result<()> {
    let taps = cfg.tap_dirs();
    if taps.is_empty() {
        println!("没有已添加的 tap。使用 `fb tap user/repo` 添加。");
        return Ok(());
    }
    for tap in taps {
        let user = tap
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let repo = tap
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        println!("{user}/{}", display_repo(&repo));
    }
    Ok(())
}

/// 解析 `user/repo`；repo 部分若省略 `homebrew-` 前缀则补全，用于目录名。
fn parse_tap_name(name: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("tap 名称应为 user/repo 形式，例如: fb tap user/homebrew-tools");
    }
    let user = validate_part(parts[0])?;
    let repo = validate_part(parts[1])?;
    let repo = if repo.starts_with("homebrew-") {
        repo
    } else {
        format!("homebrew-{repo}")
    };
    Ok((user, repo))
}

/// 从 git URL 提取 user/repo（用于 `fb tap <url>`）
fn parse_remote(url: &str) -> Result<(String, String)> {
    let cleaned = url.trim_end_matches('/').trim_end_matches(".git");
    let path = if let Some((_, rest)) = cleaned.split_once("://") {
        rest.split('/').skip(1).collect::<Vec<_>>().join("/")
    } else if let Some((_, rest)) = cleaned.split_once(':') {
        rest.to_string()
    } else {
        cleaned.to_string()
    };
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        bail!("无法从 URL 推导 tap 名称: {url}（请改用 user/repo 形式）");
    }
    let user = validate_part(parts[parts.len() - 2])?;
    let repo = validate_part(parts[parts.len() - 1])?;
    let repo = if repo.starts_with("homebrew-") {
        repo
    } else {
        format!("homebrew-{repo}")
    };
    Ok((user, repo))
}

fn validate_part(s: &str) -> Result<String> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("非法的 tap 名称片段 `{s}`（仅允许字母、数字、- _ .）");
    }
    Ok(s.to_string())
}

fn display_repo(repo: &str) -> &str {
    repo.strip_prefix("homebrew-").unwrap_or(repo)
}

fn count_formulas(dir: &std::path::Path) -> usize {
    let mut count = 0;
    for sub in ["Formula", "HomebrewFormula", ""] {
        let d = if sub.is_empty() { dir.to_path_buf() } else { dir.join(sub) };
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        count += entries
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "toml").unwrap_or(false))
            .count();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tap_name() {
        assert_eq!(
            parse_tap_name("user/tools").unwrap(),
            ("user".to_string(), "homebrew-tools".to_string())
        );
        assert_eq!(
            parse_tap_name("user/homebrew-tools").unwrap(),
            ("user".to_string(), "homebrew-tools".to_string())
        );
        assert!(parse_tap_name("user").is_err());
        assert!(parse_tap_name("user/tools/extra").is_err());
        assert!(parse_tap_name("user/tool$").is_err());
    }

    #[test]
    fn test_parse_remote() {
        assert_eq!(
            parse_remote("https://github.com/user/homebrew-tools.git").unwrap(),
            ("user".to_string(), "homebrew-tools".to_string())
        );
        assert_eq!(
            parse_remote("git@github.com:user/tools.git").unwrap(),
            ("user".to_string(), "homebrew-tools".to_string())
        );
        assert!(parse_remote("https://github.com/user").is_err());
    }
}
