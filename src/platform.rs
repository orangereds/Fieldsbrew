use anyhow::{bail, Result};

/// Fieldsbrew 仅支持 Intel Mac；任何安装类操作前都会检查。
pub fn ensure_supported() -> Result<()> {
    if !is_intel_mac() {
        bail!(
            "Fieldsbrew 仅支持 Intel Mac (x86_64 + macOS)，当前环境为 {}/{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        );
    }
    Ok(())
}

pub fn is_intel_mac() -> bool {
    cfg!(target_arch = "x86_64") && std::env::consts::OS == "macos"
}

/// 通过 sw_vers 获取 macOS 主次版本号，如 "10.15" -> (10, 15)
pub fn macos_version() -> Result<(u64, u64)> {
    let out = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut it = s.split('.');
    let major = it.next().unwrap_or("0").parse().unwrap_or(0);
    let minor = it.next().unwrap_or("0").parse().unwrap_or(0);
    Ok((major, minor))
}

pub fn macos_supported() -> bool {
    match macos_version() {
        // 支持 10.15 (Catalina) 及以上；macOS 11+ 主版本号变为 11/12/...
        Ok((10, minor)) => minor >= 15,
        Ok((major, _)) => major >= 11,
        Err(_) => false,
    }
}

/// Homebrew 风格的 macOS 平台标签（bottle 选择用）
pub fn macos_tag() -> Option<&'static str> {
    match macos_version() {
        Ok((10, minor)) if minor >= 15 => Some("catalina"),
        Ok((11, _)) => Some("big_sur"),
        Ok((12, _)) => Some("monterey"),
        Ok((13, _)) => Some("ventura"),
        Ok((14, _)) => Some("sonoma"),
        Ok((15, _)) => Some("sequoia"),
        Ok((26, _)) => Some("tahoe"),
        _ => None,
    }
}
