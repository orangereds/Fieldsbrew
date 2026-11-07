# Fieldsbrew

Fieldsbrew（命令 `fb`）是面向 **Intel Mac** 的包管理器。目标是在 Homebrew 逐步停止 Intel Mac 支持后，提供与其核心能力对齐的替代方案：Homebrew 风格的 Cellar/opt 布局、bin 链接、bottle 下载校验、taps、launchd services、Cask，以及从 Homebrew 平滑迁移的工具。

- 仅支持 macOS 10.15+ / x86_64。
- 源码构建与预编译 bottle 双路径，bottle 不可用时自动回退源码。
- 默认使用独立 prefix `/opt/fieldsbrew`，可与 Homebrew 并行共存。
- 对 Homebrew 只读：`fb` 不会修改或卸载 Homebrew 的文件。

## 安装

从 Releases 页面下载 `fb-<version>-x86_64-macos.tar.gz` 与 `sha256sums.txt`：

```sh
shasum -a 256 -c sha256sums.txt
tar -xzf fb-<version>-x86_64-macos.tar.gz
cd fieldsbrew-<version>
./install.sh
```

安装脚本默认写入 `/opt/fieldsbrew`；没有权限时可：

```sh
sudo ./install.sh
# 或
FIELDSBREW_PREFIX="$HOME/fieldsbrew" ./install.sh
```

把 shell 环境加入 `~/.zshrc` 或 `~/.bashrc`：

```sh
eval "$(/opt/fieldsbrew/bin/fb shellenv)"
```

然后检查环境：

```sh
fb doctor
```

### 校验发布签名

Releases 中的压缩包与校验和文件可带 Ed25519 分离签名（`.sig`）与公钥（`.pub`）：

```sh
fb verify fb-<version>-x86_64-macos.tar.gz
```

首次安装前如需校验，可先临时解压出 `fb`，用它验证压缩包，再执行安装。

## 快速开始

```sh
fb search hello          # 搜索 formula
fb info hello            # 查看配方信息
fb install hello         # 安装（bottle 优先，失败回退源码）
fb list                  # 已安装
fb upgrade               # 升级全部过期包
fb uninstall hello
```

常用命令：

| 命令 | 用途 |
|------|------|
| `fb install/uninstall/upgrade/reinstall` | 包生命周期 |
| `fb outdated` | 查看可升级的包 |
| `fb list [--brew]` | 列出 Fieldsbrew / Homebrew（只读）已安装包 |
| `fb search/info/deps/uses` | 搜索、信息、依赖关系 |
| `fb link/unlink` | 手动修复链接 |
| `fb tap` / `fb update` | 管理 taps 与更新 |
| `fb shellenv` | 输出 PATH/MANPATH 等环境变量 |
| `fb services` | launchd 用户服务 |
| `fb audit` | 检查 formula 字段与完整性 |
| `fb convert` | Homebrew Ruby formula/cask → TOML |
| `fb cask` | Cask 安装/卸载/列表/搜索 |
| `fb migrate` | 从 Homebrew 迁移到独立 prefix |
| `fb sign` / `fb verify` | 发布产物签名与校验 |
| `fb self-update` | 自更新 `fb` 二进制 |
| `fb net` | 网络诊断（DNS / IPv4 / IPv6 / 代理 / 镜像） |

## Formula（TOML）

```toml
dependencies = ["zlib >=1.2"]

[package]
name = "hello"
version = "2.12.1"
description = "GNU Hello"
homepage = "https://www.gnu.org/software/hello/"

[source]
url = "https://ftp.gnu.org/gnu/hello/hello-2.12.1.tar.gz"
sha256 = "..."

[build]
system = "autotools" # autotools | cmake | make
extra_args = []

[bottle]
url = "https://example.com/hello-2.12.1.sonoma.bottle.tar.gz"
sha256 = "..."
arch = "x86_64"
mirrors = ["https://mirror.example.com"]

[bottle.tags.tahoe]
url = "https://example.com/hello/2.12.1/hello-2.12.1.tahoe.bottle.tar.gz"
sha256 = "..."
```

- 搜索目录：`FIELDSBREW_FORMULA_DIR` → `<prefix>/Formula` → taps。
- bottle 候选顺序：当前 macOS 平台标签 → 通用 bottle → 源码构建。
- `bottle` 支持 `mirrors`（按顺序优先）、Ed25519 `signature`/`public_key` 与 Mach-O 重定位。

## Cask（TOML）

```toml
[package]
token = "firefox"
name = "Mozilla Firefox"
version = "132.0"
description = ""
homepage = ""

[source]
url = "https://example.com/Firefox.dmg"
sha256 = ""

[[artifacts]]
type = "app"
path = "Firefox.app"

[[artifacts]]
type = "binary"
path = "firefox"
target = "firefox"
```

- 搜索目录：`FIELDSBREW_CASK_DIR` → `<prefix>/Casks` → taps 的 `Casks/`。
- 支持 zip、dmg、tar；`app` 默认安装到 `/Applications`，`binary` 到 `<prefix>/bin`，`font` 到 `~/Library/Fonts`。
- `pkg` 安装器需要 sudo，当前明确拒绝，请手动安装。

## 环境变量

| 变量 | 说明 |
|------|------|
| `FIELDSBREW_PREFIX` | 自定义 prefix（默认 `/opt/fieldsbrew`） |
| `FIELDSBREW_FORMULA_DIR` | 覆盖 formula 搜索目录 |
| `FIELDSBREW_CASK_DIR` | 覆盖 cask 搜索目录 |
| `FIELDSBREW_MIRROR` / `FIELDSBREW_MIRRORS` | 下载镜像根地址，可多个（逗号/空白分隔），按顺序优先，失败回退原始 URL |
| `FIELDSBREW_PROXY` | 显式下载代理，优先级高于系统代理变量 |
| `FIELDSBREW_APPDIR` | cask `app` 安装目录（默认 `/Applications`） |
| `FIELDSBREW_NET_CONNECT_TIMEOUT` | 建连超时秒数（默认 10） |
| `FIELDSBREW_NET_TIMEOUT` | 单次下载整体超时秒数（默认 600） |
| `FIELDSBREW_NET_RETRIES` | 每个地址最大尝试次数（默认 3） |
| `FIELDSBREW_NET_BACKOFF` | 重试指数退避基数秒数（默认 1） |
| `FIELDSBREW_UPDATE_REPO` | 自更新默认 GitHub 仓库（`owner/repo`） |
| `FIELDSBREW_API_BASE` | 自更新 Release API 基地址（默认 `https://api.github.com`） |
| `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` | 下载代理（大小写均可） |
| `HOMEBREW_PREFIX` | 指定 Homebrew 前缀（只读识别用） |

受限网络（IPv4 不稳定、GitHub 超时、`raw.githubusercontent.com` 被重置等）的处理与实测记录见
[docs/NETWORK.md](docs/NETWORK.md)。常用命令：

```sh
fb net                      # 先诊断 DNS / IPv4 / IPv6 / 端点
fb self-update --check      # 查询最新版本
fb self-update              # 下载、校验并原子替换当前 fb
fb self-update --url <发布包地址>   # API 不可达时直连
```

## 从 Homebrew 迁移

完整步骤见 [docs/MIGRATION.md](docs/MIGRATION.md)。核心原则：

1. 先用 `fb list --brew`、`fb migrate`（不带参数）盘点 Homebrew 已安装项。
2. 转换需要的 Homebrew Ruby formula：`fb convert <tap>/Formula --out <prefix>/Formula`。
3. 用 `fb migrate <name>` 在 Fieldsbrew 独立 prefix 中重装，Homebrew 侧保持不动。
4. 用 `fb doctor` 检查同名包与 PATH 顺序。
5. 验收后再手动决定是否卸载 Homebrew 侧。

## 发布与签名

发布流程、密钥管理与产物说明见 [docs/RELEASE.md](docs/RELEASE.md)。

## 已知限制

- 仅支持 Intel Mac（x86_64）。
- 不支持完整 Homebrew Ruby DSL；`fb convert` 是静态子集转换，复杂配方需人工复核。
- 不支持 `pkg` 型 cask；不处理 cask 的 `zap`/`installer` 脚本。
- Cask 尚无自动升级；暂无 GUI 界面。

## 开发

```sh
cargo build
cargo test
./target/debug/fb --help
```
