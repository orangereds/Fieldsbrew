use anyhow::Result;
use std::path::PathBuf;

/// 全局配置与路径布局。
///
/// 目录结构（prefix 默认 /opt/fieldsbrew，可用 FIELDSBREW_PREFIX 覆盖）：
/// ```text
/// <prefix>/
/// ├── Cellar/<name>/<version>/   # 每个版本独立目录
/// ├── opt/<name>                 # 指向当前版本的符号链接（keg-only 除外）
/// ├── Formula/*.toml             # 本地 formula 目录
/// ├── var/db/fieldsbrew/installed/<name>.json   # 安装数据库（manifest）
/// └── tmp/                       # 构建临时目录
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    pub prefix: PathBuf,
    pub verbose: bool,
}

impl Config {
    pub fn new(verbose: bool) -> Self {
        let prefix = std::env::var("FIELDSBREW_PREFIX")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/opt/fieldsbrew"));
        Config { prefix, verbose }
    }

    pub fn cellar(&self) -> PathBuf {
        self.prefix.join("Cellar")
    }

    pub fn opt(&self) -> PathBuf {
        self.prefix.join("opt")
    }

    pub fn db_dir(&self) -> PathBuf {
        self.prefix.join("var/db/fieldsbrew/installed")
    }

    /// Cask 安装记录目录
    pub fn cask_db_dir(&self) -> PathBuf {
        self.prefix.join("var/db/fieldsbrew/casks")
    }

    pub fn tmp(&self) -> PathBuf {
        self.prefix.join("tmp")
    }

    /// 第三方 tap 根目录（`<prefix>/Library/Taps`）
    pub fn taps_root(&self) -> PathBuf {
        self.prefix.join("Library/Taps")
    }

    /// 已添加的 tap 目录（`<taps_root>/<user>/<repo>`）
    pub fn tap_dirs(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let root = self.taps_root();
        let Ok(users) = std::fs::read_dir(&root) else {
            return out;
        };
        for user in users.flatten() {
            if !user.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if let Ok(repos) = std::fs::read_dir(user.path()) {
                for repo in repos.flatten() {
                    if repo.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        out.push(repo.path());
                    }
                }
            }
        }
        out
    }

    /// tap 内可能的 formula 目录：Formula/、HomebrewFormula/ 与 tap 根目录
    pub fn tap_formula_dirs(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for tap in self.tap_dirs() {
            for sub in ["Formula", "HomebrewFormula"] {
                let d = tap.join(sub);
                if d.exists() {
                    out.push(d);
                }
            }
            out.push(tap);
        }
        out
    }

    /// formula 搜索目录（先环境变量覆盖，后 prefix 内置，最后 taps）
    pub fn formula_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Ok(d) = std::env::var("FIELDSBREW_FORMULA_DIR") {
            dirs.push(PathBuf::from(d));
        }
        dirs.push(self.prefix.join("Formula"));
        dirs.extend(self.tap_formula_dirs());
        dirs
    }

    /// cask 搜索目录（先环境变量覆盖，后 prefix 内置，最后 taps）
    pub fn cask_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Ok(d) = std::env::var("FIELDSBREW_CASK_DIR") {
            dirs.push(PathBuf::from(d));
        }
        dirs.push(self.prefix.join("Casks"));
        for tap in self.tap_dirs() {
            for sub in ["Casks", "HomebrewCask"] {
                let d = tap.join(sub);
                if d.exists() {
                    dirs.push(d);
                }
            }
        }
        dirs
    }

    /// 创建所有必需目录；失败时给出可操作的提示
    pub fn ensure_dirs(&self) -> Result<()> {
        for d in [self.cellar(), self.opt(), self.db_dir(), self.tmp()] {
            std::fs::create_dir_all(&d).map_err(|e| {
                anyhow::anyhow!(
                    "无法创建目录 {}（{e}）\n提示: 默认 prefix 为 /opt/fieldsbrew，需要管理员权限:\n  \
                     sudo mkdir -p /opt/fieldsbrew && sudo chown -R $(whoami) /opt/fieldsbrew\n  \
                     开发/测试时可改用环境变量 FIELDSBREW_PREFIX 指定可写目录",
                    d.display()
                )
            })?;
        }
        Ok(())
    }
}
