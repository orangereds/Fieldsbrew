use crate::cellar;
use crate::config::Config;
use crate::formula::Formula;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// 在 src_dir 中构建并把产物安装进该版本的 Cellar 目录。
pub fn build_and_install(
    cfg: &Config,
    f: &Formula,
    src_dir: &Path,
    no_sandbox: bool,
    verbose: bool,
) -> Result<()> {
    let cellar_path = f.cellar_dir(&cfg.prefix);
    std::fs::create_dir_all(&cellar_path)?;

    let system = if f.build.system.is_empty() {
        "autotools".to_string()
    } else {
        f.build.system.clone()
    };
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // 构建环境：已安装包的 bin/sbin 进 PATH，lib/pkgconfig 进 PKG_CONFIG_PATH，
    // 使 configure/cmake 能找到我们前缀里的依赖（keg-only 除外）
    let envs = build_env(cfg);

    let run = |cmd: &mut Command| -> Result<()> {
        for (k, v) in &envs {
            cmd.env(k, v);
        }
        for (k, v) in &f.build.env {
            cmd.env(k, v);
        }
        crate::sandbox::run(cmd, src_dir, no_sandbox, verbose)
    };

    match system.as_str() {
        "autotools" => {
            let mut c = Command::new("./configure");
            c.arg(format!("--prefix={}", cellar_path.display()))
                .args(&f.build.extra_args);
            run(&mut c)?;
            let mut m = Command::new("make");
            m.arg(format!("-j{jobs}"));
            run(&mut m)?;
            let mut mi = Command::new("make");
            mi.arg("install");
            run(&mut mi)?;
        }
        "cmake" => {
            let mut c = Command::new("cmake");
            c.arg("-S")
                .arg(".")
                .arg("-B")
                .arg(".fb-build")
                .arg(format!("-DCMAKE_INSTALL_PREFIX={}", cellar_path.display()))
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .args(&f.build.extra_args);
            run(&mut c)?;
            let mut b = Command::new("cmake");
            b.arg("--build")
                .arg(".fb-build")
                .arg("--parallel")
                .arg(jobs.to_string());
            run(&mut b)?;
            let mut i = Command::new("cmake");
            i.arg("--install").arg(".fb-build");
            run(&mut i)?;
        }
        "make" => {
            let mut m = Command::new("make");
            m.arg(format!("-j{jobs}"));
            run(&mut m)?;
            let mut mi = Command::new("make");
            mi.arg("install")
                .arg(format!("PREFIX={}", cellar_path.display()))
                .args(&f.build.extra_args);
            run(&mut mi)?;
        }
        other => bail!("不支持的构建系统: {other}（支持 autotools / cmake / make）"),
    }
    Ok(())
}

/// 为构建过程准备 PATH / PKG_CONFIG_PATH
fn build_env(cfg: &Config) -> Vec<(String, String)> {
    let mut bins: Vec<String> = Vec::new();
    let mut pkgconfigs: Vec<String> = Vec::new();
    if let Ok(manifests) = cellar::all_manifests(cfg) {
        for m in manifests {
            if m.keg_only {
                continue;
            }
            let base = m.cellar_path(cfg);
            bins.push(base.join("bin").display().to_string());
            bins.push(base.join("sbin").display().to_string());
            pkgconfigs.push(base.join("lib/pkgconfig").display().to_string());
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let mut parts = bins;
    parts.push(path);
    let mut envs = vec![("PATH".to_string(), parts.join(":"))];
    if !pkgconfigs.is_empty() {
        if let Ok(cur) = std::env::var("PKG_CONFIG_PATH") {
            if !cur.is_empty() {
                pkgconfigs.push(cur);
            }
        }
        envs.push(("PKG_CONFIG_PATH".to_string(), pkgconfigs.join(":")));
    }
    envs
}
