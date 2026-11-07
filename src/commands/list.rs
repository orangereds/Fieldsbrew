use crate::cellar;
use crate::config::Config;
use crate::homebrew;
use anyhow::Result;
use std::collections::HashSet;

pub fn run(cfg: &Config, brew: bool) -> Result<()> {
    if brew {
        return list_brew(cfg);
    }
    let mut manifests = cellar::all_manifests(cfg)?;
    if manifests.is_empty() {
        println!("（当前无已安装的包）");
        return Ok(());
    }
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    for m in manifests {
        let keg = if m.keg_only { " (keg-only)" } else { "" };
        println!("{:<24} {}{}", m.name, m.version, keg);
    }
    Ok(())
}

/// `fb list --brew`：只读展示 Homebrew Cellar，不修改任何 Homebrew 文件。
fn list_brew(cfg: &Config) -> Result<()> {
    let Some(prefix) = homebrew::prefix() else {
        println!("未检测到 Homebrew（可设置 HOMEBREW_PREFIX 指定前缀）");
        return Ok(());
    };
    println!("==> Homebrew（只读）: {}", prefix.display());

    let list = homebrew::installed();
    if list.is_empty() {
        println!("（Homebrew 中没有已安装的 formula）");
        return Ok(());
    }

    let fb_names: HashSet<String> = cellar::all_manifests(cfg)?
        .into_iter()
        .map(|m| m.name)
        .collect();
    for b in list {
        let note = if fb_names.contains(&b.name) {
            "  (Fieldsbrew 也已安装)"
        } else {
            ""
        };
        let all = if b.versions.len() > 1 {
            format!("  [全部版本: {}]", b.versions.join(", "))
        } else {
            String::new()
        };
        println!("{:<24} {}{}{}", b.name, b.current, all, note);
    }
    Ok(())
}
