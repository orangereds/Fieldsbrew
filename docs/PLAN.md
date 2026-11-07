# Fieldsbrew 开发计划 v2（2026-11-05）

> 前提变更：Homebrew 将在约一年后停止支持 Intel Mac。Fieldsbrew 的定位随之调整为
> **「功能与 Homebrew 对齐的包管理器」**，目标是让 Intel Mac 用户在 Homebrew 退出后
> 平滑迁移，而不是重新学一套工具。

## 1. Homebrew 核心能力基线

| # | 能力 | Homebrew 语义 | Fieldsbrew 策略 | 优先级 | 里程碑 |
|---|------|---------------|-----------------|--------|--------|
| 1 | Formula / Cask | Ruby DSL 描述源码或二进制包；Cask 管 GUI App | Formula 用 TOML 子集对齐核心字段；Cask 后置 | P0 / P2 | M2 / M7 |
| 2 | bottle（预编译产物） | Cellar 布局的 tar.gz，按平台/架构分发 | 兼容 `<name>/<version>/...` 布局 + sha256 校验 | P0 | M3 |
| 3 | 依赖解析与版本约束 | 递归依赖、`>= > <= < ==`、keg-only | 拓扑排序 + 约束校验，升级前做反向依赖检查 | P0 | M2 / M2.5 |
| 4 | taps（第三方仓库） | Git 仓库形式的 formula 集合 | 兼容 `tap` 的目录结构：`git clone` 到 `Library/Taps` | P1 | M4 |
| 5 | install / upgrade / reinstall / uninstall | 安装、升级、重装、卸载并检查反向依赖 | 同名命令，语义对齐；卸载默认阻止破坏依赖 | P0 | M2.5 |
| 6 | Cellar 布局与链接 | `Cellar/<name>/<version>`、`opt/<name>`、`bin/lib/...` 相对链接 | 完全一致；`fb link/unlink` 可修复链接 | P0 | M2.5 |
| 7 | prefix / relocation | 安装前缀可配置，bottle 内绝对路径重定位 | `FIELDSBREW_PREFIX`；bottle 落盘时做文本重定位 | P0 | M3 |
| 8 | 环境隔离与 shim | 构建期 PATH/PKG_CONFIG_PATH、`brew shellenv` | 构建环境注入已实现；后续加 `fb shellenv` | P1 | M4 |
| 9 | brew services | launchd 管理后台服务 | 生成/加载 launchd plist，与 brew services 对齐 | P2 | M5 |
| 10 | 审计与校验 | `brew audit`、sha256、签名 | sha256 必做；`fb audit` 检查 DSL/URL/校验和 | P1 | M5 |
| 11 | mirror / 代理 | 可配置镜像与 HTTP(S)_PROXY | 支持 `FIELDSBREW_MIRROR` 与系统代理 | P2 | M5 |
| 12 | 迁移与共存 | 与已有 Homebrew 并存、导入已装包 | 独立 prefix、只读识别 Homebrew、按需重建 | P0 | M6 |

## 2. 兼容边界：必须对齐 vs 可自研替代

### 必须兼容（用户可感知，破坏即迁移失败）
- **Formula DSL 子集**：`name/version/description/homepage`、`source.url/sha256`、
  `build.system/extra_args/env`、`dependencies`（含版本约束）、`install.keg_only`、
  `bottle.url/sha256`。这是把 Homebrew formula 自动转换过来的前提。
- **Cellar 布局**：`<prefix>/Cellar/<name>/<version>/`，绝不在版本目录之外散落文件。
- **bin/lib 链接语义**：`opt/<name>` 永远指向当前版本；`bin`、`sbin`、`lib`、`include`、
  `share` 下用相对符号链接指向具体 keg；keg-only 默认不链接。
- **bottle 下载与校验**：sha256 校验失败必须中止；bottle 解包后必须是
  `<name>/<version>/...` 形状，可直接落入 Cellar。
- **安装数据库**：JSON manifest 记录版本、依赖、文件清单，供 upgrade/uninstall 判定。

### 可自研替代（不要求字节级兼容）
- **构建后端**：Homebrew 用 Ruby 执行 DSL；Fieldsbrew 继续用 Rust 驱动
  autotools/cmake/make，后续可插拔。
- **签名体系**：Homebrew 用 GitHub + cosign；Fieldsbrew 采用自身发布签名 + sha256。
- **UI / 输出格式**：人类可读输出不必逐字对齐，命令语义对齐即可。
- **tap 更新机制**：可先用 `git pull`，不要求与 brew 的 API 一致。
- **services 后端**：继续用 launchd，但 plist 模板自研。

## 3. 迁移路径

### 3.1 与现有 Homebrew 共存
- Fieldsbrew prefix 固定为 `/opt/fieldsbrew`（可用 `FIELDSBREW_PREFIX` 覆盖），
  不写入 `/usr/local`，不修改 Homebrew 的 Cellar。
- PATH 建议顺序：`/opt/fieldsbrew/bin` 在 `/usr/local/bin` 之前或之后由用户决定；
  `fb doctor` 会检测 Homebrew 并提示共存风险。
- 两个包管理器可以同时安装；同一软件建议只在一侧安装，避免 PATH 遮蔽。

### 3.2 导入策略（分三档，按风险递增）
1. **只读识别（M6 起步）**：扫描 `/usr/local/Cellar` 与 `brew list --json`，
   在 `fb list --brew` 中展示，不改变任何文件。
2. **依赖感知**：若 Homebrew 已装同名包，`fb install` 提示「已由 Homebrew 提供」，
   默认不重复安装；`--independent` 时强制走独立安装。
3. **重建接管（推荐迁移方式）**：`fb migrate <name>` 读取 Homebrew formula/版本，
   用 Fieldsbrew 在独立 prefix 重新安装，验证通过后再由用户卸载 Homebrew 侧。
   不直接把 Homebrew 的 keg 移动到 Fieldsbrew Cellar，避免链接与权限混乱。

### 3.3 Intel Mac 的 bottle 来源与 fallback
- Homebrew 的 Intel bottle 主要发布在 `ghcr.io/v2/homebrew/core/...` 与
  `homebrew-core` 的 `bottle` 段，覆盖 x86_64 的 Big Sur / Monterey / Ventura 等。
- Homebrew 停止 Intel 支持后，官方新 bottle 会消失。Fieldsbrew 的策略：
  1. 优先使用**已归档的 Intel bottle 镜像**（自建 mirror + GitHub Releases 托管）；
  2. 镜像缺失时**回退到源码构建**（本仓库已有 autotools/cmake/make 后端）；
  3. formula 中同时保留 `[bottle]` 与 `[source]`，bottle 失败自动 fallback 并记录警告。
- bottle 仅接受 x86_64 产物；其他架构直接拒绝（与平台检查一致）。

## 4. 里程碑

| 里程碑 | 内容 | 状态 |
|--------|------|------|
| M0 | 脚手架、CLI、config、平台检查 | 已完成 |
| M1 | formula 解析、下载校验、解压、源码构建、manifest | 已完成 |
| M2 | 依赖解析、opt 链接、卸载反向依赖检查 | 已完成 |
| **M2.5** | **upgrade/reinstall/outdated、bin/lib 链接、最小 bottle、cleanup** | **本次落地** |
| M3 | bottle 完整化：relocation、平台标签、镜像优先、失败 fallback | 已完成 |
| M4 | taps/update、shellenv、tap 内 formula 更新 | 已完成 |
| M5 | services、audit、mirror/代理 | 已完成 |
| M6 | Homebrew 只读导入、`fb migrate`、共存体检 | 已完成 |
| M7 | Homebrew formula → TOML 转换器、Cask 支持 | 已完成 |
| M8 | 发布签名、二进制安装包、文档与迁移指南 | 已完成 |
| M9 | 受限网络适配（`fb net`、超时/重试/镜像/代理）与 `fb self-update` | 已完成 |

### 本次（M2.5）验收标准
- `fb install hello`：依赖正确、可执行。
- `fb upgrade hello`：切换到新版本，`opt` 链接更新，旧版本清理，运行仍正常。
- `fb upgrade`（不带参数）：升级所有过期包。
- `fb outdated`：列出「已安装版本 → 配方版本」。
- `fb reinstall hello`：卸载后重装成功。
- `fb uninstall`：默认被依赖时拒绝；`--force` 可覆盖。
- `fb link/unlink`：可手动修复 keg-only 或冲突链接。
- `fb cleanup`：清理旧版本与临时目录。
- 单测 + 真机 smoke test 通过。

## 5. 差距清单（截至本次）

| 能力 | 现状 | 缺口 |
|------|------|------|
| install / uninstall | 已实现 | upgrade 时的依赖约束需更细粒度求解 |
| upgrade / reinstall / outdated | 本次实现 | 批量升级中依赖链升级仍偏保守 |
| Cellar + opt | 已实现 | 增加 bin/lib/include/share 真实链接（本次） |
| bottle | 已完成（M3） | 平台标签/arch、镜像优先、Ed25519 签名、文本+符号链接+Mach-O 重定位 |
| taps / update | 已完成（M4） | tap/untap、git pull 更新、shellenv（sh/fish/csh） |
| services / audit / mirror | 已完成（M5） | launchd 服务管理、formula 审计、`FIELDSBREW_MIRROR` 与 HTTP(S)_PROXY |
| Homebrew 导入 | 已完成（M6） | `fb list --brew` 只读扫描、`fb doctor` 同名包/PATH 体检、`fb migrate` 独立 prefix 重装且不卸载 brew |
| Ruby 转换 / Cask | 已完成（M7） | `fb convert` 解析 formula/cask DSL 子集；`fb cask` 支持 zip/dmg/tar、app/binary/font 安装与卸载 |
| 发布 / 签名 / 文档 | 已完成（M8） | `fb sign/verify` Ed25519、Release 打包与安装脚本、README/MIGRATION/RELEASE 文档 |
| 网络适配 / 自更新 | 已完成（M9） | `fb net` 诊断、可配置超时/退避/多镜像/代理优先级、4xx 不重试；`fb self-update` 支持 GitHub API、`--url`、sha256 与 Ed25519 校验、原子替换；实测记录见 `docs/NETWORK.md` |
