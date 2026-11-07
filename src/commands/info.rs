use crate::cellar;
use crate::config::Config;
use crate::formula::{find_formula, Formula};
use anyhow::Result;

pub fn run(cfg: &Config, name: &str) -> Result<()> {
    let path = find_formula(cfg, name)?;
    let f = Formula::load(&path)?;
    println!("{} {}", f.package.name, f.package.version);
    if !f.package.description.is_empty() {
        println!("  描述: {}", f.package.description);
    }
    if !f.package.homepage.is_empty() {
        println!("  主页: {}", f.package.homepage);
    }
    println!("  源码: {}", f.source.url);
    let system = if f.build.system.is_empty() {
        "autotools"
    } else {
        f.build.system.as_str()
    };
    println!("  构建系统: {system}");
    let deps = f.deps()?;
    if deps.is_empty() {
        println!("  依赖: 无");
    } else {
        println!("  依赖: {}", deps.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    if f.install.keg_only {
        println!("  keg-only: 是");
    }
    if let Some(b) = &f.bottle {
        if !b.url.trim().is_empty() {
            println!("  bottle: {}", b.url);
        }
    }
    match cellar::read_manifest(cfg, name)? {
        Some(m) => {
            println!("  已安装: {} (安装时间 unix={})", m.version, m.installed_at);
            if crate::version::compare(&m.version, &f.package.version) == std::cmp::Ordering::Less {
                println!("  可升级: {} -> {}", m.version, f.package.version);
            }
        }
        None => println!("  已安装: 否"),
    }
    Ok(())
}
