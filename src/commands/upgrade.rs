use crate::cellar;
use crate::commands::install::{self, InstallOpts};
use crate::config::Config;
use crate::deps;
use crate::formula::{find_formula, Formula};
use crate::version;
use anyhow::{Context, Result};
use std::cmp::Ordering;

pub fn run(cfg: &Config, names: &[String], opts: InstallOpts) -> Result<()> {
    cfg.ensure_dirs()?;
    let targets: Vec<String> = if names.is_empty() {
        let mut all: Vec<String> = cellar::all_manifests(cfg)?
            .into_iter()
            .map(|m| m.name)
            .collect();
        all.sort();
        all
    } else {
        names.to_vec()
    };
    if targets.is_empty() {
        println!("==> 没有已安装的包可升级");
        return Ok(());
    }
    for name in &targets {
        upgrade_one(cfg, name, opts)?;
    }
    Ok(())
}

/// 升级单个包：构建新版本成功后再切换链接并清理旧版本。
pub fn upgrade_one(cfg: &Config, name: &str, opts: InstallOpts) -> Result<()> {
    let m = cellar::read_manifest(cfg, name)?
        .with_context(|| format!("{name} 未安装，无法升级"))?;
    let f = Formula::load(&find_formula(cfg, name)?)?;
    match version::compare(&m.version, &f.package.version) {
        Ordering::Less => {}
        Ordering::Equal => {
            println!("==> {name} 已是最新版本 {}", m.version);
            return Ok(());
        }
        Ordering::Greater => {
            println!(
                "==> {name} 已安装 {}，配方版本 {} 更旧，跳过",
                m.version, f.package.version
            );
            return Ok(());
        }
    }

    // 依赖正确性：升级后的版本不能破坏反向依赖的版本约束
    check_reverse_deps(cfg, name, &f.package.version)?;

    // 新版本可能引入新依赖；先补齐
    let plan = deps::resolve_dependencies(cfg, &f)?;
    for dep in &plan.install_order {
        install::install_package(cfg, dep, opts)?;
    }

    install::install_package(cfg, &f, opts)?;
    Ok(())
}

/// 检查所有已安装的反向依赖：其 formula 中对该包的版本约束必须接受新版本
fn check_reverse_deps(cfg: &Config, name: &str, new_version: &str) -> Result<()> {
    let mut issues: Vec<String> = Vec::new();
    for other in cellar::all_manifests(cfg)? {
        if other.name == name || !other.dependencies.iter().any(|d| d == name) {
            continue;
        }
        match find_formula(cfg, &other.name).and_then(|p| Formula::load(&p)) {
            Ok(f) => {
                for d in f.deps()? {
                    if d.name == name {
                        if let Some(r) = d.req {
                            if !r.matches(new_version) {
                                issues.push(format!(
                                    "{} 要求 {} {}（新版本为 {}）",
                                    other.name,
                                    name,
                                    r.describe(),
                                    new_version
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("警告: 无法读取 {} 的 formula，跳过版本约束检查: {e:#}", other.name),
        }
    }
    if !issues.is_empty() {
        anyhow::bail!(
            "升级 {name} 到 {new_version} 会破坏以下依赖:\n  {}",
            issues.join("\n  ")
        );
    }
    Ok(())
}
