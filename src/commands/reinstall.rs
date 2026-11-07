use crate::cellar;
use crate::commands::install::{self, InstallOpts};
use crate::commands::uninstall;
use crate::config::Config;
use anyhow::{Context, Result};

pub fn run(cfg: &Config, names: &[String], opts: InstallOpts) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("请至少指定一个要重装的包名");
    }
    cfg.ensure_dirs()?;
    for name in names {
        let m = cellar::read_manifest(cfg, name)?
            .with_context(|| format!("{name} 未安装，无法重装"))?;
        println!("==> 重装 {} {}", m.name, m.version);
        // 先移除旧版本（不检查反向依赖：重装是原地修复）
        uninstall::remove_keg(cfg, &m)?;
        install::install_one(cfg, name, opts)?;
        println!("==> 重装完成: {name}");
    }
    Ok(())
}
