use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// TOML formula 示例：
/// ```toml
/// # 顶层 dependencies（可选，必须位于任何 [section] 之前）
/// dependencies = []
///
/// [package]
/// name = "hello"
/// version = "2.12.1"
/// description = "GNU Hello"
/// homepage = "https://www.gnu.org/software/hello/"
/// # 也可写在这里，两处会合并去重
///
/// [source]
/// url = "https://ftp.gnu.org/gnu/hello/hello-2.12.1.tar.gz"
/// sha256 = "8d9914..."
///
/// # 可选：预编译 bottle（Homebrew 布局：<name>/<version>/...）
/// # 下载/校验失败会自动回退到 [source] 源码构建
/// [bottle]
/// url = "https://example.com/hello-2.12.1.bottle.tar.gz"
/// sha256 = "..."
///
/// [build]
/// system = "autotools"   # autotools | cmake | make
/// extra_args = []
///
/// [install]
/// keg_only = false
/// ```

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    /// 容错：也接受写在 [package] 小节内的依赖列表
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub url: String,
    /// 为空时跳过校验（仅警告）
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Build {
    /// autotools | cmake | make；缺省按 autotools 处理
    #[serde(default)]
    pub system: String,
    /// 附加参数，追加到 configure/cmake/make install 之后
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// 构建期间的额外环境变量
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Install {
    /// keg-only：默认不做 prefix 链接（opt/<name> 仍会创建），避免覆盖系统同名工具
    #[serde(default)]
    pub keg_only: bool,
}

/// 预编译产物：与 Homebrew bottle 布局兼容——归档内为 `<name>/<version>/...`
///
/// 支持平台/架构标签与镜像候选：
/// ```toml
/// [bottle]
/// url = "https://example.com/hello-2.12.1.bottle.tar.gz"
/// sha256 = "..."
/// arch = "x86_64"            # 可选；非 Intel 产物直接拒绝
/// platforms = ["catalina"]   # 可选；仅在这些平台使用该通用 bottle
/// mirrors = ["https://mirror.example.com/hello-2.12.1.bottle.tar.gz"]
/// signature = "https://example.com/hello-2.12.1.bottle.sig"
/// public_key = "..."         # Ed25519 公钥（hex）
///
/// # 按 macOS 平台标签提供专用产物，优先于通用 url
/// [bottle.tags.monterey]
/// url = "..."
/// sha256 = "..."
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Bottle {
    #[serde(default)]
    pub url: String,
    /// 为空时跳过校验（仅警告）
    #[serde(default)]
    pub sha256: String,
    /// 目标架构；x86_64 / amd64 / intel 或留空表示 Intel
    #[serde(default)]
    pub arch: String,
    /// 适用平台标签（Homebrew 风格，如 catalina/monterey/ventura）
    #[serde(default)]
    pub platforms: Vec<String>,
    /// 镜像候选，按顺序优先于 url 尝试
    #[serde(default)]
    pub mirrors: Vec<String>,
    /// 分离式签名文件 URL（可选）
    #[serde(default)]
    pub signature: String,
    /// Ed25519 公钥（hex，可选；与 signature 成对出现）
    #[serde(default)]
    pub public_key: String,
    /// 按平台标签索引的专用 bottle
    #[serde(default)]
    pub tags: BTreeMap<String, BottleTag>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BottleTag {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub public_key: String,
}

/// 后台服务描述（对齐 brew services 的 formula DSL 子集）
/// ```toml
/// [service]
/// run = "opt/foo/bin/foo --flag"
/// keep_alive = true
/// working_dir = ""
/// log_path = ""
/// error_log_path = ""
/// [service.env]
/// FOO = "bar"
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Service {
    /// 启动命令；相对路径基于 prefix，裸命令名在 prefix/bin 下查找
    #[serde(default)]
    pub run: String,
    /// 异常退出后是否自动拉起
    #[serde(default)]
    pub keep_alive: bool,
    /// `fb services run` 时是否随 launchd 加载立即运行
    #[serde(default)]
    pub run_at_load: bool,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub log_path: String,
    #[serde(default)]
    pub error_log_path: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Formula {
    pub package: Package,
    pub source: Source,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub install: Install,
    /// 可选 bottle；下载/校验失败时自动回退源码构建
    #[serde(default)]
    pub bottle: Option<Bottle>,
    /// 可选后台服务定义（`fb services`）
    #[serde(default)]
    pub service: Option<Service>,
    /// 依赖列表："zlib" 或 "zlib >=1.2"
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub req: Option<crate::version::Requirement>,
}

impl Formula {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("读取 formula 失败: {}", path.display()))?;
        Self::parse(&s).with_context(|| format!("解析 formula 失败: {}", path.display()))
    }

    /// 解析依赖声明为 (名称, 可选版本约束)。
    /// 兼容两处写法：文件顶层 `dependencies = [...]`（必须在任何 [section] 之前）
    /// 与 `[package]` 小节内的 `dependencies`，合并去重。
    pub fn deps(&self) -> Result<Vec<Dependency>> {
        let mut merged: Vec<String> = Vec::new();
        for s in self.package.dependencies.iter().chain(self.dependencies.iter()) {
            let s = s.trim();
            if !s.is_empty() && !merged.iter().any(|m: &String| m == s) {
                merged.push(s.to_string());
            }
        }
        let mut out = Vec::new();
        for s in &merged {
            let split_at = s
                .char_indices()
                .find(|(_, c)| matches!(c, '>' | '<' | '='))
                .map(|(i, _)| i);
            match split_at {
                Some(i) => {
                    let name = s[..i].trim().to_string();
                    let req = crate::version::parse_requirement(&s[i..])?;
                    out.push(Dependency { name, req: Some(req) });
                }
                None => out.push(Dependency {
                    name: s.to_string(),
                    req: None,
                }),
            }
        }
        Ok(out)
    }

    /// 该版本在 Cellar 中的安装目录
    pub fn cellar_dir(&self, prefix: &Path) -> PathBuf {
        prefix
            .join("Cellar")
            .join(&self.package.name)
            .join(&self.package.version)
    }
}

/// 按 name 查找 formula 文件
pub fn find_formula(cfg: &crate::config::Config, name: &str) -> Result<PathBuf> {
    let mut searched = Vec::new();
    for dir in cfg.formula_dirs() {
        let p = dir.join(format!("{name}.toml"));
        if p.exists() {
            return Ok(p);
        }
        searched.push(dir.display().to_string());
    }
    anyhow::bail!(
        "找不到 formula `{name}`，已搜索: {}",
        if searched.is_empty() {
            "（无搜索目录）".to_string()
        } else {
            searched.join(", ")
        }
    )
}

/// 加载所有 formula（按 name 去重，先出现的目录优先）
pub fn all_formulas(cfg: &crate::config::Config) -> Result<Vec<Formula>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in cfg.formula_dirs() {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            if p.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Ok(f) = Formula::load(&p) {
                    if seen.insert(f.package.name.clone()) {
                        out.push(f);
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"dependencies = ["libtool"]

[package]
name = "curl"
version = "8.11.1"
description = "command line url tool"
homepage = "https://curl.se/"
dependencies = ["zlib", "pcre2 >=10.0"]

[source]
url = "https://example.com/curl-8.11.1.tar.xz"
sha256 = "abc123"

[build]
system = "autotools"
extra_args = ["--with-openssl"]

[install]
keg_only = false
"#;

    #[test]
    fn test_parse_formula() {
        let f = Formula::parse(SAMPLE).unwrap();
        assert_eq!(f.package.name, "curl");
        assert_eq!(f.package.version, "8.11.1");
        let deps = f.deps().unwrap();
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].name, "zlib");
        assert!(deps[0].req.is_none());
        assert_eq!(deps[1].name, "pcre2");
        let req = deps[1].req.as_ref().unwrap();
        assert!(req.matches("10.42"));
        assert!(!req.matches("9.9"));
        assert_eq!(deps[2].name, "libtool");
        assert_eq!(f.build.extra_args, vec!["--with-openssl"]);
        assert!(!f.install.keg_only);
    }

    #[test]
    fn test_parse_minimal() {
        let f = Formula::parse(
            r#"
[package]
name = "hello"
version = "1.0"
[source]
url = "https://example.com/hello.tar.gz"
"#,
        )
        .unwrap();
        assert!(f.deps().unwrap().is_empty());
        assert!(f.build.system.is_empty());
        assert!(f.source.sha256.is_empty());
    }
}
