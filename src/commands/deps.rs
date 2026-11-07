use crate::cellar;
use crate::config::Config;
use crate::deps as resolver;
use crate::formula::{find_formula, Formula};
use anyhow::Result;
use std::collections::HashSet;

pub fn run(cfg: &Config, name: &str, tree: bool) -> Result<()> {
    if tree {
        print_tree(cfg, name, "", &mut HashSet::new())
    } else {
        let plan = resolver::resolve(cfg, name)?;
        for n in &plan.satisfied {
            println!("{n} (已安装)");
        }
        for f in &plan.install_order {
            println!("{} {}", f.package.name, f.package.version);
        }
        Ok(())
    }
}

fn print_tree(cfg: &Config, name: &str, prefix: &str, seen: &mut HashSet<String>) -> Result<()> {
    if !seen.insert(name.to_string()) {
        println!("{prefix}{name} (已展开)");
        return Ok(());
    }
    let installed = cellar::read_manifest(cfg, name)?.is_some();
    match find_formula(cfg, name).and_then(|p| Formula::load(&p)) {
        Ok(f) => {
            let marker = if installed { " (已安装)" } else { "" };
            println!("{prefix}{name}-{}{marker}", f.package.version);
            for d in f.deps()? {
                print_tree(cfg, &d.name, &format!("{prefix}    "), seen)?;
            }
        }
        Err(_) => println!("{prefix}{name} (找不到 formula)"),
    }
    Ok(())
}
