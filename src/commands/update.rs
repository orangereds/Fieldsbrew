use crate::config::Config;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// `fb update`：对所有 git 形式的 tap 执行 `git pull --ff-only`。
/// prefix 下的 Formula 目录若本身是 git 仓库，也会一并更新。
pub fn run(cfg: &Config) -> Result<()> {
    println!("==> 更新配方仓库");
    let mut repos = cfg.tap_dirs();
    let builtin = cfg.prefix.join("Formula");
    if builtin.join(".git").exists() {
        repos.insert(0, builtin);
    }

    if repos.is_empty() {
        println!("    没有可更新的仓库。使用 `fb tap user/repo` 添加 tap。");
        return Ok(());
    }

    let mut failed = Vec::new();
    for repo in &repos {
        let label = repo.display();
        if !repo.join(".git").exists() {
            println!("    跳过（非 git 仓库）: {label}");
            continue;
        }
        print!("    git pull --ff-only: {label} ... ");
        match pull(repo) {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                println!("失败");
                eprintln!("      {e:#}");
                failed.push(label.to_string());
            }
        }
    }

    if failed.is_empty() {
        println!("==> 更新完成");
        Ok(())
    } else {
        bail!("{} 个仓库更新失败: {}", failed.len(), failed.join(", "))
    }
}

fn pull(repo: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["pull", "--ff-only"])
        .output()?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("{}", if msg.is_empty() { "git pull 失败".to_string() } else { msg });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let summary = text
        .lines()
        .find(|l| l.contains("Already up to date") || l.contains("up to date"))
        .unwrap_or("已更新");
    Ok(summary.trim().to_string())
}
