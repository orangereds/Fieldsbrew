use crate::archive;
use crate::bottle;
use crate::build;
use crate::cellar;
use crate::config::Config;
use crate::deps;
use crate::download;
use crate::formula::{find_formula, Formula};
use crate::link;
use crate::version;
use anyhow::{Context, Result};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct InstallOpts {
    pub no_sandbox: bool,
    pub keep_tmp: bool,
}

pub fn run(cfg: &Config, names: &[String], opts: InstallOpts) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("请至少指定一个要安装的包名，例如: fb install hello");
    }
    cfg.ensure_dirs()?;
    for name in names {
        if let Some(m) = cellar::read_manifest(cfg, name)? {
            let f = Formula::load(&find_formula(cfg, name)?)?;
            match version::compare(&m.version, &f.package.version) {
                Ordering::Less => {
                    println!(
                        "==> {name} 已安装 {}，配方已更新到 {}，转为升级",
                        m.version, f.package.version
                    );
                    super::upgrade::upgrade_one(cfg, name, opts)?;
                }
                Ordering::Equal => println!("==> {name} 已安装 ({})，无需操作", m.version),
                Ordering::Greater => println!(
                    "==> {name} 已安装 {}，比配方版本 {} 更新，跳过",
                    m.version, f.package.version
                ),
            }
            continue;
        }
        install_one(cfg, name, opts)?;
    }
    Ok(())
}

/// 安装一个尚未安装的包（自动递归安装缺失依赖）
pub fn install_one(cfg: &Config, name: &str, opts: InstallOpts) -> Result<()> {
    let plan = deps::resolve(cfg, name)?;
    if plan.install_order.is_empty() {
        println!("==> {name} 已安装，无需操作");
        return Ok(());
    }
    let list: Vec<&str> = plan
        .install_order
        .iter()
        .map(|f| f.package.name.as_str())
        .collect();
    println!("==> 将安装: {}", list.join(" -> "));
    if !plan.satisfied.is_empty() {
        println!("==> 已满足依赖: {}", plan.satisfied.join(", "));
    }
    for f in &plan.install_order {
        install_package(cfg, f, opts)?;
    }
    Ok(())
}

/// 安装/升级单个 formula 到 Cellar 并维护链接与 manifest。
/// 升级时先构建新版本，成功后才清理旧版本，失败不影响现有安装。
pub fn install_package(cfg: &Config, f: &Formula, opts: InstallOpts) -> Result<()> {
    let name = &f.package.name;
    let version = &f.package.version;
    let existing = cellar::read_manifest(cfg, name)?;
    if let Some(m) = &existing {
        match version::compare(&m.version, version) {
            Ordering::Equal => {
                println!("==> {name} {version} 已安装，无需操作");
                return Ok(());
            }
            Ordering::Greater => {
                println!(
                    "==> {name} 已安装更新版本 {}，不回退到 {version}",
                    m.version
                );
                return Ok(());
            }
            Ordering::Less => {}
        }
    }

    let system = if f.build.system.is_empty() {
        "autotools".to_string()
    } else {
        f.build.system.clone()
    };
    if existing.is_some() {
        println!("==> 升级 {name} {} -> {version}", existing.as_ref().unwrap().version);
    } else {
        println!("==> 安装 {name} {version}");
    }

    let work = cfg.tmp().join(format!("{name}-{}-{}", version, std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let src = work.join("src");
    std::fs::create_dir_all(&src)?;

    let result = (|| -> Result<()> {
        let cellar_path = f.cellar_dir(&cfg.prefix);
        if !bottle::pour(cfg, f, &work)? {
            // bottle 未配置或失败：走源码构建
            if cellar_path.exists() {
                std::fs::remove_dir_all(&cellar_path)?;
            }
            install_from_source(cfg, f, &work, &src, &system, opts)?;
        }

        // 新版本已就绪：清理旧版本链接与目录
        if let Some(m) = &existing {
            if version::compare(&m.version, version) == Ordering::Less {
                link::unlink_keg(cfg, name, &m.version)?;
                let old = m.cellar_path(cfg);
                if old.exists() {
                    std::fs::remove_dir_all(&old)
                        .with_context(|| format!("清理旧版本失败: {}", old.display()))?;
                }
                let pkg_dir = cfg.cellar().join(name);
                if pkg_dir.is_dir() && std::fs::read_dir(&pkg_dir)?.next().is_none() {
                    std::fs::remove_dir(&pkg_dir)?;
                }
            }
        }

        // opt/<name> 始终指向当前版本（keg-only 也保留，方便其他 formula 显式引用）
        cellar::create_opt_link(cfg, name, version)?;

        // prefix 链接：keg-only 默认不链接
        if f.install.keg_only {
            println!("    keg-only：跳过 bin/lib 链接（可用 fb link {name} 手动链接）");
        } else {
            let report = link::link_keg(cfg, name, version, false)?;
            if !report.conflicts.is_empty() {
                eprintln!(
                    "警告: {} 个文件未链接（已存在同名文件）: {}",
                    report.conflicts.len(),
                    report
                        .conflicts
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            println!("    已链接 {} 个文件到 {}", report.linked, cfg.prefix.display());
        }

        // 写入 manifest（文件清单作为后续卸载依据）
        let files = cellar::collect_files(&cellar_path)?;
        let m = cellar::Manifest {
            name: name.clone(),
            version: version.clone(),
            description: f.package.description.clone(),
            dependencies: f.deps()?.into_iter().map(|d| d.name).collect(),
            keg_only: f.install.keg_only,
            installed_at: cellar::now(),
            files,
        };
        cellar::write_manifest(cfg, &m)?;
        println!("    完成: {}", cellar_path.display());
        Ok(())
    })();

    match (result, opts.keep_tmp) {
        (Ok(()), true) => println!("    临时目录保留: {}", work.display()),
        (Ok(()), false) => {
            let _ = std::fs::remove_dir_all(&work);
        }
        (Err(e), _) => {
            let _ = std::fs::remove_dir_all(&work);
            return Err(e.context(format!("安装 {name} 失败")));
        }
    }
    Ok(())
}

fn install_from_source(
    cfg: &Config,
    f: &Formula,
    work: &Path,
    src: &Path,
    system: &str,
    opts: InstallOpts,
) -> Result<()> {
    let archive_name = f
        .source
        .url
        .rsplit('/')
        .next()
        .unwrap_or("source.tar")
        .to_string();
    let archive = work.join(&archive_name);
    println!("    下载 {}", f.source.url);
    download::fetch(&f.source.url, &archive)?;
    download::verify(&archive, &f.source.sha256, &f.package.name)?;

    println!("    解压 {archive_name}");
    archive::extract(&archive, archive::detect(&f.source.url)?, src)?;
    let src_dir = find_src_root(src)?;

    println!("    构建 (system={system})");
    build::build_and_install(cfg, f, &src_dir, opts.no_sandbox, cfg.verbose)?;
    Ok(())
}

/// 解压后定位实际源码根目录：若 src 下只有一个子目录（带顶层目录的 tarball）则进入它
fn find_src_root(base: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(base)?.flatten().collect();
    if entries.len() == 1 && entries[0].path().is_dir() {
        Ok(entries[0].path())
    } else {
        Ok(base.to_path_buf())
    }
}


