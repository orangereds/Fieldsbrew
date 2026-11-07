use crate::cellar;
use crate::config::Config;
use crate::formula::{find_formula, Formula};
use crate::version;
use anyhow::Result;
use std::cmp::Ordering;

pub fn run(cfg: &Config) -> Result<()> {
    let mut manifests = cellar::all_manifests(cfg)?;
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    let mut outdated = 0;
    for m in manifests {
        let Ok(f) = find_formula(cfg, &m.name).and_then(|p| Formula::load(&p)) else {
            continue;
        };
        if version::compare(&m.version, &f.package.version) == Ordering::Less {
            println!("{:<24} {} -> {}", m.name, m.version, f.package.version);
            outdated += 1;
        }
    }
    if outdated == 0 {
        println!("==> 所有已安装的包均为最新");
    } else {
        println!("==> 共 {outdated} 个包可升级，执行: fb upgrade");
    }
    Ok(())
}
