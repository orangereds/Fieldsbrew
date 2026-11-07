use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

/// 网络策略：全部可通过环境变量覆盖，方便在受限网络（如 IPv4 不可达、
/// 仅 IPv6 出网）下切换镜像、代理和重试节奏。
///
/// | 变量 | 默认值 | 说明 |
/// |------|--------|------|
/// | `FIELDSBREW_NET_CONNECT_TIMEOUT` | 10 | TCP/TLS 建连超时（秒） |
/// | `FIELDSBREW_NET_TIMEOUT` | 600 | 单次下载整体超时（秒） |
/// | `FIELDSBREW_NET_RETRIES` | 3 | 每个地址的最大尝试次数 |
/// | `FIELDSBREW_NET_BACKOFF` | 1 | 指数退避基数（秒）：1、2、4… |
/// | `FIELDSBREW_MIRROR` | 空 | 镜像前缀，可用逗号/分号/空白分隔多个，按顺序优先 |
/// | `FIELDSBREW_PROXY` | 空 | 显式代理，优先级最高 |
#[derive(Debug, Clone)]
pub struct NetConfig {
    pub connect_timeout: Duration,
    pub overall_timeout: Duration,
    pub retries: u32,
    pub backoff: Duration,
    pub mirrors: Vec<String>,
    pub proxy: Option<String>,
}

impl NetConfig {
    pub fn from_env() -> Self {
        Self {
            connect_timeout: Duration::from_secs(env_u64("FIELDSBREW_NET_CONNECT_TIMEOUT", 10)),
            overall_timeout: Duration::from_secs(env_u64("FIELDSBREW_NET_TIMEOUT", 600)),
            retries: env_u64("FIELDSBREW_NET_RETRIES", 3).max(1) as u32,
            backoff: Duration::from_secs(env_u64("FIELDSBREW_NET_BACKOFF", 1)),
            mirrors: mirrors_from_env(),
            proxy: proxy_from_env(),
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// 下载 url 到 dest（覆盖写）。镜像按 `FIELDSBREW_MIRROR` 顺序优先，
/// 全部失败后回退原始 URL。每个地址按 `FIELDSBREW_NET_RETRIES` 重试。
pub fn fetch(url: &str, dest: &Path) -> Result<()> {
    fetch_with(url, dest, &NetConfig::from_env())
}

pub fn fetch_with(url: &str, dest: &Path, cfg: &NetConfig) -> Result<()> {
    let candidates = candidates_with(url, &cfg.mirrors);
    let mut last_err = None;
    for (i, c) in candidates.iter().enumerate() {
        if i > 0 {
            eprintln!("警告: 上一个地址失败，尝试回退地址: {c}");
        }
        match fetch_retries(c, dest, cfg) {
            Ok(()) => {
                if c != url {
                    println!("    已通过镜像获取: {c}");
                }
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("至少有一个下载地址"))
}

fn candidates_with(url: &str, mirrors: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for base in mirrors {
        if let Some(m) = mirror_candidate(url, base) {
            if !out.iter().any(|u| u == &m) {
                out.push(m);
            }
        }
    }
    if !out.iter().any(|u| u == url) {
        out.push(url.to_string());
    }
    out
}

/// `FIELDSBREW_MIRROR` / `FIELDSBREW_MIRRORS`，按逗号、分号或空白分隔。
fn mirrors_from_env() -> Vec<String> {
    let mut out = Vec::new();
    for key in ["FIELDSBREW_MIRROR", "FIELDSBREW_MIRRORS"] {
        let Ok(v) = std::env::var(key) else {
            continue;
        };
        for part in v.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
            let part = part.trim().trim_end_matches('/');
            if !part.is_empty() && !out.iter().any(|m: &String| m == part) {
                out.push(part.to_string());
            }
        }
    }
    out
}

/// 把 `https://host/path?query` 映射到 `<mirror_base>/path?query`
pub fn mirror_candidate(url: &str, base: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("/");
    Some(format!("{base}{path}"))
}

fn fetch_retries(url: &str, dest: &Path, cfg: &NetConfig) -> Result<()> {
    let mut last_err = None;
    for attempt in 1..=cfg.retries {
        match fetch_once(url, dest, cfg) {
            Ok(()) => return Ok(()),
            Err(FetchError::Permanent(e)) => {
                let _ = std::fs::remove_file(dest);
                return Err(e);
            }
            Err(FetchError::Retryable(e)) => {
                let _ = std::fs::remove_file(dest);
                if attempt < cfg.retries {
                    let wait = cfg
                        .backoff
                        .saturating_mul(1 << (attempt - 1).min(5));
                    eprintln!(
                        "警告: 下载失败（第 {attempt}/{} 次）: {e}，{} 秒后重试...",
                        cfg.retries,
                        wait.as_secs()
                    );
                    std::thread::sleep(wait);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.expect("至少重试一次"))
}

enum FetchError {
    /// 4xx（除 408/429）：重试无意义
    Permanent(anyhow::Error),
    Retryable(anyhow::Error),
}

fn fetch_once(url: &str, dest: &Path, cfg: &NetConfig) -> std::result::Result<(), FetchError> {
    let mut builder = ureq::AgentBuilder::new()
        .user_agent(concat!("fb/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(cfg.connect_timeout)
        .timeout(cfg.overall_timeout);
    if let Some(proxy) = proxy_for(cfg) {
        builder = builder.proxy(proxy);
    }
    let agent = builder.build();
    let resp = match agent.get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _))
            if (400..500).contains(&code) && code != 408 && code != 429 =>
        {
            return Err(FetchError::Permanent(anyhow!(
                "下载失败 {url}: HTTP {code}（不可重试）"
            )));
        }
        Err(e) => {
            return Err(FetchError::Retryable(anyhow!("下载失败 {url}: {e}")));
        }
    };
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| {
        FetchError::Retryable(anyhow!("创建文件失败 {}: {e}", dest.display()))
    })?;
    std::io::copy(&mut reader, &mut file).map_err(|e| {
        FetchError::Retryable(anyhow!("写入下载内容失败 {}: {e}", dest.display()))
    })?;
    Ok(())
}

fn proxy_for(cfg: &NetConfig) -> Option<ureq::Proxy> {
    let raw = cfg.proxy.as_deref()?;
    match ureq::Proxy::new(raw) {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("警告: 忽略无效代理配置 {raw}: {e}");
            None
        }
    }
}

/// 代理优先级：`FIELDSBREW_PROXY` > `HTTPS_PROXY` > `HTTP_PROXY` > `ALL_PROXY`
/// （大小写两种写法均识别，空值忽略）。
fn proxy_from_env() -> Option<String> {
    pick_proxy(|key| std::env::var(key).ok())
}

fn pick_proxy<F>(get: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    for key in [
        "FIELDSBREW_PROXY",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        let Some(v) = get(key) else { continue };
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f =
        std::fs::File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// 校验下载产物；formula 未提供 sha256 时跳过并警告。
pub fn verify(path: &Path, expected: &str, name: &str) -> Result<()> {
    if expected.trim().is_empty() {
        eprintln!("警告: {name} 的 formula 未提供 sha256，跳过完整性校验");
        return Ok(());
    }
    let actual = sha256_file(path)?;
    if actual != expected.trim().to_lowercase() {
        return Err(anyhow!(
            "{name} sha256 校验失败\n  期望: {}\n  实际: {actual}",
            expected.trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_candidate() {
        assert_eq!(
            mirror_candidate(
                "https://ftp.gnu.org/gnu/hello/hello-2.12.1.tar.gz",
                "https://mirror.example.com/homebrew/"
            ),
            Some(
                "https://mirror.example.com/homebrew/gnu/hello/hello-2.12.1.tar.gz".to_string()
            )
        );
        assert_eq!(
            mirror_candidate("https://x.com/a.tgz?sig=1", "mirror.local"),
            Some("mirror.local/a.tgz?sig=1".to_string())
        );
        assert_eq!(mirror_candidate("/local/file.tar.gz", "https://mirror"), None);
        assert_eq!(mirror_candidate("https://x.com/a", "  "), None);
    }

    #[test]
    fn test_candidates_multi_mirror() {
        let mirrors = vec![
            "https://m1.example".to_string(),
            "https://m2.example/base/".to_string(),
        ];
        assert_eq!(
            candidates_with("https://x.com/a.tgz?sig=1", &mirrors),
            vec![
                "https://m1.example/a.tgz?sig=1".to_string(),
                "https://m2.example/base/a.tgz?sig=1".to_string(),
                "https://x.com/a.tgz?sig=1".to_string()
            ]
        );
    }

    #[test]
    fn test_proxy_precedence() {
        let get = |key: &str| match key {
            "HTTPS_PROXY" => Some("http://https-proxy:8080".to_string()),
            "ALL_PROXY" => Some("socks5://all-proxy:1080".to_string()),
            "FIELDSBREW_PROXY" => Some("http://fb-proxy:7890".to_string()),
            _ => None,
        };
        assert_eq!(
            pick_proxy(get).as_deref(),
            Some("http://fb-proxy:7890")
        );

        let get = |key: &str| match key {
            "HTTPS_PROXY" => Some("http://https-proxy:8080".to_string()),
            "HTTP_PROXY" => Some("  ".to_string()),
            "ALL_PROXY" => Some("socks5://all-proxy:1080".to_string()),
            _ => None,
        };
        assert_eq!(
            pick_proxy(get).as_deref(),
            Some("http://https-proxy:8080")
        );

        assert_eq!(pick_proxy(|_| None), None);
    }

    #[test]
    fn test_sha256_deterministic() {
        let dir = std::env::temp_dir().join(format!("fb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("data");
        std::fs::write(&p, b"fieldsbrew").unwrap();
        let a = sha256_file(&p).unwrap();
        let b = sha256_file(&p).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
