use crate::{archive, download, sign};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_REPO: &str = "fieldsbrew/fieldsbrew";
const DEFAULT_API_BASE: &str = "https://api.github.com";

pub struct Options {
    pub check: bool,
    pub url: Option<String>,
    pub repo: Option<String>,
    pub version: Option<String>,
    pub force: bool,
    pub sha256: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// GitHub API 新版会给出 "sha256:..." 摘要
    #[serde(default)]
    digest: String,
}

/// 一次自更新所需的全部下载信息。
#[derive(Debug, Clone)]
struct Resolved {
    version: Option<String>,
    tarball_url: String,
    tarball_name: String,
    digest: Option<String>,
    checksums_url: Option<String>,
    sig_url: Option<String>,
    pub_url: Option<String>,
}

pub fn run(opts: Options) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("==> 当前版本: fb {current}");

    let resolved = match &opts.url {
        Some(url) => resolve_direct(url, opts.version.as_deref()),
        None => resolve_release(&opts)?,
    };

    if let Some(v) = &resolved.version {
        println!("==> 远端版本: {v}");
        if !opts.force && !is_newer(v, current) {
            println!("==> 已是最新版本（fb {current}），无需更新");
            return Ok(());
        }
    } else {
        println!("==> 远端版本: 未知（使用 --url 直连，跳过版本比较）");
    }

    if opts.check {
        println!("==> 检查完成，未下载（去掉 --check 即可更新）");
        return Ok(());
    }

    let tmp = std::env::temp_dir().join(format!("fb-self-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)
        .with_context(|| format!("创建临时目录失败: {}", tmp.display()))?;

    let result = install(&opts, &resolved, &tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn resolve_direct(url: &str, version: Option<&str>) -> Resolved {
    let name = url.rsplit('/').next().unwrap_or(url).to_string();
    Resolved {
        version: version.map(normalize_version),
        tarball_url: url.to_string(),
        tarball_name: name,
        digest: None,
        checksums_url: None,
        sig_url: None,
        pub_url: None,
    }
}

fn resolve_release(opts: &Options) -> Result<Resolved> {
    let repo = opts
        .repo
        .clone()
        .or_else(|| std::env::var("FIELDSBREW_UPDATE_REPO").ok())
        .unwrap_or_else(|| DEFAULT_REPO.to_string());
    let api_base = std::env::var("FIELDSBREW_API_BASE")
        .unwrap_or_else(|_| DEFAULT_API_BASE.to_string());
    let api_base = api_base.trim_end_matches('/');

    let path = match opts.version.as_deref() {
        Some(v) => format!(
            "{api_base}/repos/{repo}/releases/tags/{}",
            normalize_tag(v)
        ),
        None => format!("{api_base}/repos/{repo}/releases/latest"),
    };
    println!("==> 查询 Release: {path}");

    let tmp_json = std::env::temp_dir().join(format!("fb-release-{}.json", std::process::id()));
    let result = download::fetch(&path, &tmp_json)
        .with_context(|| format!("无法查询 GitHub Release（仓库 {repo}）。\n  受限网络可改用: fb self-update --url <发布包地址>"));
    let release: Release = match result {
        Ok(()) => {
            let text = std::fs::read_to_string(&tmp_json)?;
            serde_json::from_str(&text).with_context(|| format!("解析 Release JSON 失败: {}", path))?
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_json);
            return Err(e);
        }
    };
    let _ = std::fs::remove_file(&tmp_json);

    let version = normalize_version(&release.tag_name);
    let expected = asset_name(&version);
    let tarball = release
        .assets
        .iter()
        .find(|a| a.name == expected)
        .or_else(|| {
            release.assets.iter().find(|a| {
                a.name.starts_with("fb-") && a.name.ends_with("x86_64-macos.tar.gz")
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "Release {} 中没有 x86_64 macOS 包（期望 {expected}）",
                release.tag_name
            )
        })?;

    let find = |suffix: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == format!("{}{suffix}", tarball.name))
            .map(|a| a.browser_download_url.clone())
    };
    let checksums_url = release
        .assets
        .iter()
        .find(|a| a.name == "sha256sums.txt")
        .map(|a| a.browser_download_url.clone());

    Ok(Resolved {
        version: Some(version),
        tarball_url: tarball.browser_download_url.clone(),
        tarball_name: tarball.name.clone(),
        digest: digest_value(&tarball.digest),
        checksums_url,
        sig_url: find(".sig"),
        pub_url: find(".pub"),
    })
}

fn install(opts: &Options, r: &Resolved, tmp: &Path) -> Result<()> {
    let tarball = tmp.join(&r.tarball_name);
    println!("==> 下载: {}", r.tarball_url);
    download::fetch(&r.tarball_url, &tarball)?;

    // 校验和：显式参数 > Release digest > sha256sums.txt
    let mut expected = opts.sha256.clone().or_else(|| r.digest.clone());
    if expected.is_none() {
        if let Some(url) = &r.checksums_url {
            let sums = tmp.join("sha256sums.txt");
            if download::fetch(url, &sums).is_ok() {
                if let Ok(text) = std::fs::read_to_string(&sums) {
                    expected = checksum_for(&text, &r.tarball_name);
                }
            }
        }
    }
    if let Some(sum) = &expected {
        download::verify(&tarball, sum, &r.tarball_name)?;
        println!("     sha256 校验通过: {sum}");
    } else {
        eprintln!("警告: Release 未提供 sha256，跳过完整性校验");
    }

    verify_signature(opts, r, &tarball, tmp)?;

    let format = archive::detect(&r.tarball_url)?;
    let payload = tmp.join("payload");
    archive::extract(&tarball, format, &payload)?;
    let new_bin = find_binary(&payload).ok_or_else(|| {
        anyhow!(
            "发布包内未找到 fb 可执行文件（解压目录 {}）",
            payload.display()
        )
    })?;
    sanity_check(&new_bin, r.version.as_deref())?;
    replace_current_exe(&new_bin, r.version.as_deref())
}

fn verify_signature(opts: &Options, r: &Resolved, tarball: &Path, tmp: &Path) -> Result<()> {
    let (sig_url, pub_spec) = match (&opts.public_key, &r.sig_url, &r.pub_url) {
        (Some(pk), sig, _) => {
            let url = sig.clone().unwrap_or_else(|| format!("{}.sig", r.tarball_url));
            (Some(url), Some(pk.clone()))
        }
        (None, Some(sig), Some(pub_url)) => (Some(sig.clone()), Some(pub_url.clone())),
        _ => (None, None),
    };

    let Some(sig_url) = sig_url else {
        if opts.public_key.is_some() {
            bail!("指定了 --public-key，但找不到对应的 .sig 发布文件");
        }
        println!("     （未提供发布签名，跳过签名校验）");
        return Ok(());
    };
    let Some(pub_spec) = pub_spec else {
        println!("     （发现 .sig 但缺少公钥，跳过签名校验）");
        return Ok(());
    };

    let sig_path = tmp.join("release.sig");
    download::fetch(&sig_url, &sig_path).with_context(|| "下载签名失败")?;

    let pk = if Path::new(&pub_spec).is_file() {
        sign::read_public_key(&pub_spec)?
    } else if pub_spec.starts_with("https://") || pub_spec.starts_with("http://") {
        let p = tmp.join("release.pub");
        download::fetch(&pub_spec, &p).with_context(|| "下载公钥失败")?;
        let path = p.to_str().context("临时公钥路径包含非法字符")?;
        sign::read_public_key(path)?
    } else {
        sign::read_public_key(&pub_spec)?
    };

    let data = std::fs::read(tarball)?;
    let sig_bytes = std::fs::read(&sig_path)?;
    sign::verify_bytes(&data, &sig_bytes, &pk)?;
    println!("     Ed25519 签名校验通过");
    Ok(())
}

fn sanity_check(bin: &Path, version: Option<&str>) -> Result<()> {
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .with_context(|| format!("无法执行新二进制: {}", bin.display()))?;
    if !out.status.success() {
        bail!("新二进制自检失败: fb --version 退出码 {:?}", out.status.code());
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if let Some(v) = version {
        if !text.contains(v) {
            bail!("新二进制版本与 Release 不一致：期望包含 {v}，实际输出: {}", text.trim());
        }
    }
    println!("     新二进制自检通过: {}", text.trim());
    Ok(())
}

fn replace_current_exe(new_bin: &Path, version: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe().context("无法定位当前 fb 二进制")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let staged = exe.with_file_name(format!(".fb-update-{}", std::process::id()));

    std::fs::copy(new_bin, &staged).with_context(|| {
        format!(
            "写入新版本失败: {}\n  若 fb 安装在系统目录，请用 sudo 重新运行",
            staged.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&staged, &exe)
        .with_context(|| format!("替换 {} 失败", exe.display()))?;

    match version {
        Some(v) => println!("==> 已更新: fb -> {v}（{}）", exe.display()),
        None => println!("==> 已更新: {}（版本未知）", exe.display()),
    }
    Ok(())
}

fn find_binary(root: &Path) -> Option<PathBuf> {
    let direct = root.join("fb");
    if direct.is_file() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_binary(&p) {
                return Some(found);
            }
        } else if p.file_name().map(|n| n == "fb").unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

fn asset_name(version: &str) -> String {
    format!("fb-{}-x86_64-macos.tar.gz", normalize_version(version))
}

fn digest_value(d: &str) -> Option<String> {
    let d = d.trim();
    if d.is_empty() {
        None
    } else {
        Some(d.strip_prefix("sha256:").unwrap_or(d).to_string())
    }
}

fn checksum_for(text: &str, filename: &str) -> Option<String> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if hash.len() == 64 && name.trim_start_matches('*') == filename {
            return Some(hash.to_lowercase());
        }
    }
    None
}

/// 去掉前导 `v`，只保留版本号主体（如 v0.2.0 -> 0.2.0）。
fn normalize_version(v: &str) -> String {
    v.trim().trim_start_matches('v').to_string()
}

fn normalize_tag(v: &str) -> String {
    let v = v.trim();
    if v.starts_with('v') {
        v.to_string()
    } else {
        format!("v{v}")
    }
}

/// 简单的数字段版本比较；预发布后缀按主版本号比较。
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        normalize_version(s)
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    let len = r.len().max(l.len());
    for i in 0..len {
        let a = r.get(i).copied().unwrap_or(0);
        let b = l.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(is_newer("v0.1.0-beta.2", "0.1.0-beta.1"));
    }

    #[test]
    fn test_asset_name() {
        assert_eq!(asset_name("v0.2.0"), "fb-0.2.0-x86_64-macos.tar.gz");
        assert_eq!(asset_name("0.2.0"), "fb-0.2.0-x86_64-macos.tar.gz");
    }

    #[test]
    fn test_checksum_for() {
        let text = "\
abc123  other.tar.gz
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  fb-0.2.0-x86_64-macos.tar.gz
";
        assert_eq!(
            checksum_for(text, "fb-0.2.0-x86_64-macos.tar.gz").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert!(checksum_for(text, "missing.tar.gz").is_none());
    }

    #[test]
    fn test_parse_release() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "assets": [
                {"name": "fb-0.2.0-x86_64-macos.tar.gz", "browser_download_url": "https://example.com/fb.tar.gz", "digest": "sha256:deadbeef"},
                {"name": "sha256sums.txt", "browser_download_url": "https://example.com/sha256sums.txt"}
            ]
        }"#;
        let r: Release = serde_json::from_str(json).unwrap();
        assert_eq!(r.tag_name, "v0.2.0");
        assert_eq!(r.assets.len(), 2);
        assert_eq!(digest_value(&r.assets[0].digest).as_deref(), Some("deadbeef"));
    }
}
