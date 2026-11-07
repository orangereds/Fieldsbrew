use crate::config::Config;
use crate::formula::all_formulas;
use anyhow::Result;

pub fn run(cfg: &Config, name: &str) -> Result<()> {
    let mut users = Vec::new();
    for f in all_formulas(cfg)? {
        if f.deps()?.iter().any(|d| d.name == name) {
            users.push(f.package.name);
        }
    }
    if users.is_empty() {
        println!("没有 formula 依赖 {name}");
    } else {
        println!("依赖 {name} 的 formula:");
        for u in users {
            println!("  {u}");
        }
    }
    Ok(())
}
