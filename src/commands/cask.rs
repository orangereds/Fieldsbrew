use crate::archive;
use crate::cask::{self, Cask, CaskManifest, InstalledArtifact};
use crate::cli::CaskCommand;
use crate::config::Config;
use crate::download;
use crate::version;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(cfg: &Config, cmd: CaskCommand) -> Result<()> {
    match cmd {
        CaskCommand::Install { tokens, force } => {
            if tokens.is_empty() {
                bail!("请至少指定一个 cask，例如: fb cask install firefox");
            }
            for token in &tokens {
                install_one(cfg, token, force)?;
            }
            Ok(())
        }
        CaskCommand::Uninstall { tokens } => {
            if tokens.is_empty() {
                bail!("请至少指定一个 cask");
            }
            for token in &tokens {
                uninstall_one(cfg, token)?;
            }
            Ok(())
        }
        CaskCommand::List => list(cfg),
        CaskCommand::Search { query } => search(cfg, query.as_deref()),
    }
}

fn install_one(cfg: &Config, token: &str, force: bool) -> Result<()> {
    let path = cask::find_cask(cfg, token)?;
    let c = Cask::load(&path)?;
    let new_version = c.package.version.clone();

    if let Some(m) = cask::read_manifest(cfg, token)? {
        if !force && version::compare(&m.version, &new_version) == std::cmp::Ordering::Equal {
            println!("==> cask {token} {new_version} 已安装，无需操作");
            return Ok(());
        }
        println!(
            "==> 更新 cask {token}: {} -> {new_version}",
            m.version
        );
        uninstall_manifest(cfg, &m)?;
    }

    cfg.ensure_dirs()?;
    println!(
        "==> 安装 cask {token} {new_version}（{}）",
        c.display_name()
    );
    if !c.package.homepage.is_empty() {
        println!("    homepage: {}", c.package.homepage);
    }

    let work = cfg
        .tmp()
        .join(format!("cask-{}-{}", token, std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let mut installed: Vec<InstalledArtifact> = Vec::new();
    let result = (|| -> Result<()> {
        let archive = work.join(url_file_name(&c.source.url, token));
        println!("    下载 {}", c.source.url);
        download::fetch(&c.source.url, &archive)?;
        download::verify(&archive, &c.source.sha256, &c.package.token)?;

        let payload = Payload::prepare(&archive, &c.source.url, &work, cfg.verbose)?;
        let mut warnings = Vec::new();
        for a in &c.artifacts {
            match install_artifact(cfg, &c, a, payload.root(), force, &mut warnings)? {
                Some(x) => installed.push(x),
                None => {}
            }
        }
        for w in warnings {
            eprintln!("    警告: {w}");
        }
        if installed.is_empty() {
            bail!("没有安装任何产物，请检查 cask 的 artifacts 配置");
        }

        let m = CaskManifest {
            token: c.package.token.clone(),
            name: c.display_name().to_string(),
            version: new_version.clone(),
            installed_at: crate::cellar::now(),
            artifacts: installed.clone(),
        };
        cask::write_manifest(cfg, &m)?;
        println!("    完成: {} 个产物", m.artifacts.len());
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&work);
    if result.is_err() {
        for a in &installed {
            remove_path(Path::new(&a.path));
        }
        return result.map_err(|e| e.context(format!("安装 cask {token} 失败")));
    }
    Ok(())
}

fn uninstall_one(cfg: &Config, token: &str) -> Result<()> {
    let m = cask::read_manifest(cfg, token)?
        .with_context(|| format!("cask {token} 未安装"))?;
    uninstall_manifest(cfg, &m)?;
    println!("==> 已卸载 cask {token} {}", m.version);
    Ok(())
}

fn uninstall_manifest(cfg: &Config, m: &CaskManifest) -> Result<()> {
    for a in &m.artifacts {
        let p = Path::new(&a.path);
        if !p.is_absolute() {
            eprintln!("警告: 跳过非绝对路径的安装记录: {}", a.path);
            continue;
        }
        remove_path(p);
    }
    cask::remove_manifest(cfg, &m.token)
}

fn list(cfg: &Config) -> Result<()> {
    let manifests = cask::all_manifests(cfg)?;
    if manifests.is_empty() {
        println!("（当前无已安装的 cask）");
        return Ok(());
    }
    for m in manifests {
        println!("{:<28} {:<14} {}", m.token, m.version, m.name);
    }
    Ok(())
}

fn search(cfg: &Config, query: Option<&str>) -> Result<()> {
    let casks = cask::all_casks(cfg)?;
    if casks.is_empty() {
        println!(
            "未找到任何 cask。\n提示: 可把 cask TOML 放到 {}/Casks，\
             或用 `fb convert --cask <file.rb>` 转换 Homebrew cask",
            cfg.prefix.display()
        );
        return Ok(());
    }
    let q = query.map(|s| s.to_lowercase());
    let mut found = 0;
    for c in &casks {
        if let Some(q) = &q {
            let hay = format!(
                "{} {} {}",
                c.package.token,
                c.package.name,
                c.package.description
            )
            .to_lowercase();
            if !hay.contains(q) {
                continue;
            }
        }
        println!(
            "{:<28} {:<14} {}",
            c.package.token,
            c.package.version,
            c.package.description
        );
        found += 1;
    }
    if found == 0 {
        println!("未找到匹配 {query:?} 的 cask");
    }
    Ok(())
}

fn install_artifact(
    cfg: &Config,
    c: &Cask,
    a: &cask::Artifact,
    payload: &Path,
    force: bool,
    warnings: &mut Vec<String>,
) -> Result<Option<InstalledArtifact>> {
    let src = payload.join(&a.path);
    if !src.exists() {
        bail!(
            "cask {} 的安装产物不存在: {}（解压目录 {}）",
            c.package.token,
            a.path,
            payload.display()
        );
    }
    let dest = match a.kind.as_str() {
        "app" => {
            let base = app_dir();
            let name = src
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&a.path));
            if a.target.is_empty() {
                base.join(name)
            } else {
                resolve_target(&a.target, &base)
            }
        }
        "binary" => {
            let base = cfg.prefix.join("bin");
            if a.target.is_empty() {
                base.join(
                    src.file_name()
                        .map(|s| s.to_os_string())
                        .unwrap_or_else(|| "binary".into()),
                )
            } else {
                resolve_target(&a.target, &base)
            }
        }
        "font" => {
            let base = home_fonts_dir()?;
            if a.target.is_empty() {
                base.join(
                    src.file_name()
                        .map(|s| s.to_os_string())
                        .unwrap_or_else(|| "font".into()),
                )
            } else {
                resolve_target(&a.target, &base)
            }
        }
        "pkg" => bail!(
            "cask {} 使用 pkg 安装器（{}），需要 sudo，暂不支持；请手动安装或改用 app/binary 产物",
            c.package.token,
            a.path
        ),
        other => {
            warnings.push(format!("未支持的产物类型 {other}，已跳过: {}", a.path));
            return Ok(None);
        }
    };

    if dest.exists() || dest.symlink_metadata().is_ok() {
        if !force {
            bail!(
                "目标已存在: {}（使用 `fb cask install {} --force` 覆盖）",
                dest.display(),
                c.package.token
            );
        }
        remove_path(&dest);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if src.is_dir() {
        copy_app(&src, &dest)?;
    } else {
        std::fs::copy(&src, &dest)
            .with_context(|| format!("复制失败: {} -> {}", src.display(), dest.display()))?;
        if a.kind == "binary" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&dest)?.permissions().mode();
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode | 0o111))?;
            }
        }
    }
    Ok(Some(InstalledArtifact {
        kind: a.kind.clone(),
        path: dest.display().to_string(),
    }))
}

fn app_dir() -> PathBuf {
    std::env::var("FIELDSBREW_APPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Applications"))
}

fn home_fonts_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("无法确定 HOME 目录，不能安装字体")?;
    Ok(PathBuf::from(home).join("Library/Fonts"))
}

fn resolve_target(target: &str, base: &Path) -> PathBuf {
    if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        base.join(target)
    }
}

fn url_file_name(url: &str, token: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{token}.download"))
}

fn remove_path(p: &Path) {
    let Ok(meta) = p.symlink_metadata() else {
        return;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        let _ = std::fs::remove_file(p);
    } else {
        let _ = std::fs::remove_dir_all(p);
    }
}

fn copy_app(src: &Path, dest: &Path) -> Result<()> {
    let ok = Command::new("ditto")
        .arg(src)
        .arg(dest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        return Ok(());
    }
    copy_tree(src, dest)
}

fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            let p = entry.path();
            let target = dest.join(entry.file_name());
            if entry.file_type()?.is_symlink() {
                #[cfg(unix)]
                {
                    let link = std::fs::read_link(&p)?;
                    std::fs::remove_file(&target).ok();
                    std::os::unix::fs::symlink(link, &target)?;
                }
            } else if p.is_dir() {
                copy_tree(&p, &target)?;
            } else {
                std::fs::copy(&p, &target)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

/// 解压/挂载后的 payload 根目录；DMG 在 Drop 时自动 detach。
struct Payload {
    root: PathBuf,
    mount: Option<PathBuf>,
}

impl Payload {
    fn prepare(archive: &Path, url: &str, work: &Path, verbose: bool) -> Result<Self> {
        let lower = url
            .split(['?', '#'])
            .next()
            .unwrap_or(url)
            .to_lowercase();
        if lower.ends_with(".dmg") {
            let mount = work.join("mnt");
            std::fs::create_dir_all(&mount)?;
            let out = Command::new("hdiutil")
                .arg("attach")
                .arg("-nobrowse")
                .arg("-readonly")
                .arg("-mountpoint")
                .arg(&mount)
                .arg(archive)
                .output()
                .context("执行 hdiutil 失败")?;
            if !out.status.success() {
                bail!(
                    "挂载 DMG 失败: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            if verbose {
                println!("{}", String::from_utf8_lossy(&out.stdout));
            }
            Ok(Payload {
                root: mount.clone(),
                mount: Some(mount),
            })
        } else if lower.ends_with(".zip") {
            let root = work.join("payload");
            std::fs::create_dir_all(&root)?;
            let status = Command::new("ditto")
                .arg("-x")
                .arg("-k")
                .arg(archive)
                .arg(&root)
                .status()
                .context("执行 ditto 解压失败（zip 需要 macOS 的 ditto）")?;
            if !status.success() {
                bail!("解压 zip 失败: {}", archive.display());
            }
            Ok(Payload {
                root,
                mount: None,
            })
        } else if lower.ends_with(".pkg") {
            bail!("cask 使用 pkg 安装器（{}），需要 sudo，暂不支持", url);
        } else {
            let root = work.join("payload");
            archive::extract(archive, archive::detect(url)?, &root)?;
            Ok(Payload {
                root,
                mount: None,
            })
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        if let Some(mount) = &self.mount {
            let _ = Command::new("hdiutil")
                .arg("detach")
                .arg("-quiet")
                .arg(mount)
                .status();
        }
    }
}
