use crate::download::NetConfig;
use anyhow::Result;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

struct Endpoint {
    url: &'static str,
    host: &'static str,
    port: u16,
}

const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        url: "https://github.com",
        host: "github.com",
        port: 443,
    },
    Endpoint {
        url: "https://api.github.com",
        host: "api.github.com",
        port: 443,
    },
    Endpoint {
        url: "https://codeload.github.com",
        host: "codeload.github.com",
        port: 443,
    },
    Endpoint {
        url: "https://raw.githubusercontent.com",
        host: "raw.githubusercontent.com",
        port: 443,
    },
    Endpoint {
        url: "https://objects.githubusercontent.com",
        host: "objects.githubusercontent.com",
        port: 443,
    },
    Endpoint {
        url: "https://ghcr.io",
        host: "ghcr.io",
        port: 443,
    },
    Endpoint {
        url: "https://mirrors.tuna.tsinghua.edu.cn",
        host: "mirrors.tuna.tsinghua.edu.cn",
        port: 443,
    },
    Endpoint {
        url: "https://ftp.gnu.org",
        host: "ftp.gnu.org",
        port: 443,
    },
];

struct ProbeResult {
    host: String,
    https_ok: bool,
    detail: String,
}

pub fn run(timeout_secs: u64) -> Result<()> {
    let timeout = Duration::from_secs(timeout_secs.max(1));
    let net = NetConfig::from_env();

    println!("==> Fieldsbrew 网络诊断");
    println!(
        "  代理: {}",
        net.proxy
            .as_deref()
            .map(redact_proxy)
            .unwrap_or_else(|| "未配置（可用 FIELDSBREW_PROXY 指定）".to_string())
    );
    println!(
        "  镜像: {}",
        if net.mirrors.is_empty() {
            "未配置（可用 FIELDSBREW_MIRROR 指定，多个用逗号分隔）".to_string()
        } else {
            net.mirrors.join(", ")
        }
    );
    println!(
        "  超时: 建连 {}s / 单次下载 {}s / 重试 {} 次",
        timeout_secs,
        net.overall_timeout.as_secs(),
        net.retries
    );
    println!();

    let results: Vec<ProbeResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = ENDPOINTS
            .iter()
            .map(|ep| scope.spawn(move || probe(ep, timeout)))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect()
    });

    for r in &results {
        println!("{}", r.detail);
    }

    let ok = results.iter().filter(|r| r.https_ok).count();
    let tcp_ok = results.iter().filter(|r| r.detail.contains("[TCP 可达]")).count();
    println!(
        "==> 汇总: HTTPS 可达 {ok}/{}，TCP 可达 {tcp_ok}/{}",
        results.len(),
        results.len()
    );

    for tip in suggestions(&results, &net) {
        println!("    - {tip}");
    }
    if ok == 0 {
        println!(
            "    当前无任何端点完成 TLS 握手；可先用 FIELDSBREW_MIRROR 指向内网/本地镜像，\n    或临时使用离线 Formula 目录（FIELDSBREW_FORMULA_DIR）。"
        );
    }
    Ok(())
}

fn probe(ep: &Endpoint, timeout: Duration) -> ProbeResult {
    let mut detail = format!("  --- {}", ep.url);
    let mut https_ok = false;

    let addrs = match (ep.host, ep.port).to_socket_addrs() {
        Ok(a) => a.collect::<Vec<_>>(),
        Err(e) => {
            detail.push_str(&format!("\n      DNS: 解析失败: {e}"));
            return ProbeResult {
                host: ep.host.to_string(),
                https_ok: false,
                detail,
            };
        }
    };
    if addrs.is_empty() {
        detail.push_str("\n      DNS: 无记录");
    }
    for addr in &addrs {
        detail.push_str(&format!("\n      DNS: {} ({})", addr.ip(), family(addr)));
    }

    for addr in addrs.iter().take(4) {
        let start = Instant::now();
        let label = family(addr);
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_) => detail.push_str(&format!(
                "\n      [TCP 可达] {label} {}:{} ({} ms)",
                addr.ip(),
                addr.port(),
                start.elapsed().as_millis()
            )),
            Err(e) => detail.push_str(&format!(
                "\n      [TCP 失败] {label} {}:{}: {e}",
                addr.ip(),
                addr.port()
            )),
        }
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout(Duration::from_secs(10))
        .build();
    match agent.get(ep.url).call() {
        Ok(resp) => {
            https_ok = true;
            detail.push_str(&format!("\n      [HTTPS 可达] HTTP {}", resp.status()));
        }
        Err(ureq::Error::Status(code, _)) => {
            https_ok = true;
            detail.push_str(&format!("\n      [HTTPS 可达] HTTP {code}（TLS 握手完成）"));
        }
        Err(ureq::Error::Transport(t)) => {
            detail.push_str(&format!("\n      [HTTPS 失败] {t}"));
        }
    }

    ProbeResult {
        host: ep.host.to_string(),
        https_ok,
        detail,
    }
}

fn family(addr: &SocketAddr) -> &'static str {
    match addr {
        SocketAddr::V4(_) => "IPv4",
        SocketAddr::V6(v6) => {
            if v6.ip().to_ipv4_mapped().is_some() {
                "IPv4(mapped)"
            } else {
                "IPv6"
            }
        }
    }
}

fn suggestions(results: &[ProbeResult], net: &NetConfig) -> Vec<String> {
    let mut out = Vec::new();
    let failed = |name: &str| {
        results
            .iter()
            .find(|r| r.host == name)
            .map(|r| !r.https_ok)
            .unwrap_or(true)
    };

    if failed("github.com") {
        out.push(
            "github.com 无法完成 TLS 握手：该域名当前没有 AAAA 记录，IPv6 也不可用；\
             tap 克隆与 git push 会失败。".to_string(),
        );
        out.push(
            "GitHub 受限时优先设置 FIELDSBREW_MIRROR（例如内网镜像或 TUNA 反向代理）。".to_string(),
        );
    }
    if failed("api.github.com") {
        out.push(
            "api.github.com 不可达：`fb self-update` 无法查询最新 Release，\
             可改用 `fb self-update --url <发布包地址>`。".to_string(),
        );
    }
    if failed("raw.githubusercontent.com") {
        out.push(
            "raw.githubusercontent.com 不可达：依赖该域名的公式源码需改用镜像或本地缓存。"
                .to_string(),
        );
    }
    if net.mirrors.is_empty() {
        out.push(
            "未配置镜像；受限网络中建议 FIELDSBREW_MIRROR=https://<镜像>/homebrew-bottles"
                .to_string(),
        );
    }
    if net.proxy.is_none() {
        out.push(
            "未配置代理；如有 HTTP(S)/SOCKS 代理，可 FIELDSBREW_PROXY=http://127.0.0.1:7890"
                .to_string(),
        );
    }
    out
}

/// 隐去代理 URL 中的用户名密码，避免诊断输出泄露凭据。
fn redact_proxy(p: &str) -> String {
    let Some((scheme, rest)) = p.split_once("://") else {
        return p.to_string();
    };
    let Some((userinfo, host)) = rest.rsplit_once('@') else {
        return p.to_string();
    };
    if userinfo.is_empty() {
        return p.to_string();
    }
    format!("{scheme}://***@{host}")
}
