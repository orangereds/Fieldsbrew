use crate::cellar;
use crate::commands::install::{install_one, InstallOpts};
use crate::config::Config;
use crate::formula::find_formula;
use crate::homebrew;
use anyhow::{bail, Result};

/// `fb migrate <names>`：把 Homebrew 已安装的包在 Fieldsbrew 独立 prefix 中重装。
/// 只读 Homebrew Cellar 获取版本，不移动/删除 Homebrew 的任何文件。
pub fn run(cfg: &Config, names: &[String], opts: InstallOpts) -> Result<()> {
    let brew = homebrew::installed();
    if brew.is_empty() {
        println!("未检测到 Homebrew 已安装的 formula（可设置 HOMEBREW_PREFIX 指定前缀）。");
        return Ok(());
    }

    if names.is_empty() {
        return list_candidates(cfg, &brew);
    }

    let mut failed = Vec::new();
    for name in names {
        match brew.iter().find(|b| &b.name == name) {
            Some(b) => {
                if let Err(e) = migrate_one(cfg, name, &b.current, opts) {
                    println!("==> 迁移 {name} 失败: {e:#}");
                    failed.push(name.clone());
                }
            }
            None => {
                println!("==> 跳过 {name}：Homebrew 未安装该包");
                failed.push(name.clone());
            }
        }
    }

    if failed.is_empty() {
        println!(
            "提示: Homebrew 侧未做任何修改；验证命令可用后，再执行 `brew uninstall <name>` 卸载旧版本。"
        );
        Ok(())
    } else {
        bail!("{} 个包未完成迁移: {}", failed.len(), failed.join(", "))
    }
}

fn list_candidates(cfg: &Config, brew: &[homebrew::BrewInstall]) -> Result<()> {
    println!("{:<24} {:<14} {}", "Homebrew 包", "版本", "Fieldsbrew 状态");
    for b in brew {
        let has_formula = find_formula(cfg, &b.name).is_ok();
        let installed = cellar::read_manifest(cfg, &b.name)?.map(|m| m.version);
        let status = match installed {
            Some(v) => format!("已安装 {v}"),
            None if has_formula => "可迁移".to_string(),
            None => "缺少 Fieldsbrew formula（可先添加 tap）".to_string(),
        };
        println!("{:<24} {:<14} {}", b.name, b.current, status);
    }
    println!("使用 `fb migrate <name>` 执行迁移；不会自动卸载 Homebrew 侧。");
    Ok(())
}

fn migrate_one(cfg: &Config, name: &str, brew_version: &str, opts: InstallOpts) -> Result<()> {
    let formula_path = find_formula(cfg, name).map_err(|_| {
        anyhow::anyhow!(
            "缺少 {name} 的 Fieldsbrew formula；请先 `fb tap user/repo` 添加包含该 formula 的 tap"
        )
    })?;
    println!(
        "==> 迁移 {name}：Homebrew {brew_version} -> Fieldsbrew（formula: {}）",
        formula_path.display()
    );

    install_one(cfg, name, opts)?;

    let manifest = cellar::read_manifest(cfg, name)?
        .ok_or_else(|| anyhow::anyhow!("迁移后未找到 {name} 的安装记录"))?;
    let cmp = homebrew::compare_versions(&manifest.version, brew_version);
    println!("    Fieldsbrew 已安装 {name} {}", manifest.version);
    match cmp {
        std::cmp::Ordering::Equal => println!("    版本与 Homebrew 相同"),
        std::cmp::Ordering::Greater => {
            println!("    Fieldsbrew 版本高于 Homebrew（{brew_version}）")
        }
        std::cmp::Ordering::Less => {
            println!("    注意: Fieldsbrew 版本低于 Homebrew（{brew_version}），请确认 formula 是否最新")
        }
    }
    println!("    Homebrew 侧仍保留 {name} {brew_version}，未做修改");
    Ok(())
}
