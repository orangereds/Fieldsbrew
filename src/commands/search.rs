use crate::config::Config;
use crate::formula::all_formulas;
use anyhow::Result;

pub fn run(cfg: &Config, query: Option<&str>) -> Result<()> {
    let formulas = all_formulas(cfg)?;
    if formulas.is_empty() {
        println!(
            "未找到任何 formula。\n提示: 可通过 FIELDSBREW_FORMULA_DIR 环境变量指定 formula 目录，\
             或把 *.toml 放到 {}/Formula",
            cfg.prefix.display()
        );
        return Ok(());
    }
    let mut found = 0;
    for f in &formulas {
        if let Some(q) = query {
            let q = q.to_lowercase();
            if !f.package.name.to_lowercase().contains(&q)
                && !f.package.description.to_lowercase().contains(&q)
            {
                continue;
            }
        }
        println!("{:<24} {:<10} {}", f.package.name, f.package.version, f.package.description);
        found += 1;
    }
    if found == 0 {
        println!("未找到匹配 \"{query:?}\" 的 formula");
    }
    Ok(())
}
