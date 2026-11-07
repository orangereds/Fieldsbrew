# 网络故障记录与 Fieldsbrew 适配

> 实测机器：Intel Mac，macOS，Wi-Fi `en0`。
> 记录时间：2026-11-07。
> 网络状况会变化，本文同时保留「故障态」和「恢复态」两组观测，避免把临时现象当成永久结论。

## 1. 结论摘要

1. 该机器**不是稳定的“IPv4 不通、IPv6 可用”**，而是**间歇性出网异常**：
   - 故障时段：绝大多数公网 `443/443` 连 TCP 都超时，DNS 也超时；
   - 恢复时段：`github.com`、`api.github.com`、`codeload.github.com`、`ghcr.io`、TUNA、GNU 均可 HTTPS 访问；
   - 始终不稳定：`raw.githubusercontent.com` 的 TLS 被重置（TCP 可连）。
2. `github.com` **没有 AAAA 记录**，因此“只走 IPv6”无法访问 GitHub 主站；tap 克隆、git push 不能依赖 IPv6 绕过。
3. 当前 Wi-Fi 未配置 DNS 服务器，系统 resolver 使用链路本地 `fe80::1%en0`；故障时段 `curl` 报 `Resolving timed out`。
4. 网关 `192.168.1.1:8080` 不是 HTTP 代理（返回 JSON API，`CONNECT` 得 404），不能当代理用。
5. 本机存在 Homebrew 7.0.12，但 Intel Mac 支持已进入退出阶段；后续构建/测试**不以 brew 可用为前提**，Fieldsbrew 自带下载、校验、解包与源码构建路径。

## 2. 现场信息

```text
en0: 192.168.1.17 / IPv6 2409:8a3c:3464:...（全局地址）
默认路由: IPv4 192.168.1.1，IPv6 fe80::...%en0
系统 DNS: fe80::1%en0（networksetup -getdnsservers Wi-Fi: 未设置）
系统代理: networksetup -getwebproxy Wi-Fi: Enabled: No
环境代理: 未设置 ALL_PROXY/HTTPS_PROXY/HTTP_PROXY/FIELDSBREW_PROXY
Homebrew 镜像变量: HOMEBREW_API_DOMAIN / HOMEBREW_BOTTLE_DOMAIN 指向 TUNA
```

## 3. 故障现象与对策（逐项）

| # | 现象 | 实测证据 | 判定 | 对策 |
|---|------|----------|------|------|
| 1 | DNS 解析超时 | `curl -v https://github.com/` → `Resolving timed out after 4004 ms`；`networksetup -getdnsservers Wi-Fi` → `There aren't any DNS Servers set on Wi-Fi.` | 系统 DNS 依赖链路本地地址，链路异常时解析一并失败 | 临时改用 `223.5.5.5` / IPv6 DNS；`fb net` 会显示解析结果与耗时 |
| 2 | IPv4 到公网 443 超时 | `curl -4 https://1.1.1.1` → `Connection timed out`；`nc 20.205.243.166 443`、`140.82.112.3 443`、`185.199.108.133 443`、`39.156.66.10 443` 均超时 | 不是 DNS 问题，是 TCP 层出网受限/丢包 | 配置 `FIELDSBREW_PROXY`；或使用可达镜像；`fb net` 逐地址标注成功/失败 |
| 3 | IPv4 80 端口却可达 | `curl -4 http://github.com/` → `301 20.205.243.166`；`curl -4 http://www.baidu.com` → `200` | 443 受限不等于整机断网，可能是端口/策略限制 | 不要据此判断“已恢复”；以 HTTPS 探测为准 |
| 4 | IPv6 出网可用 | `curl -6 https://www.baidu.com` → `200`（`2409:8c00:...`）；TUNA、`ftp.gnu.org` 均 IPv6 200 | IPv6 路径本身可用 | 优选支持 IPv6 的镜像（如 TUNA） |
| 5 | github.com 无 AAAA | `dig +short AAAA github.com @223.5.5.5` → 空 | GitHub 主站无 IPv6，IPv6-only 无法直连 | 使用镜像/代理；不要宣称“IPv6 可访问 GitHub” |
| 6 | raw.githubusercontent.com TLS 重置 | TCP 到 IPv4/IPv6 均可达；HTTPS 报 `Connection reset by peer (os error 54)` | SNI/中间设备重置，非 Fieldsbrew 问题 | 用 `FIELDSBREW_MIRROR` 代理 raw 内容或本地缓存 formula |
| 7 | api.github.com 时通时断 | 故障时段 `Connection timed out`；恢复时段 `HTTP 200`；`nc` 端口先成功后失败 | 间歇性过滤/链路抖动 | `fb self-update` 支持 `--url` 直连发布包，绕开 API |
| 8 | 网关 8080 疑似代理 | `http://192.168.1.1:8080/` → JSON；`curl -x ... https://github.com` → `CONNECT tunnel failed, response 404` | 是设备 API，不是代理 | 不要配置为代理 |
| 9 | Homebrew 不能作为依赖来源 | `/usr/local/bin/brew` 存在（7.0.12），但 Intel 支持即将结束；`gh` 未安装 | 不能把 `brew install` 写进构建前提 | 使用 Fieldsbrew 自身 bottle/源码构建；`fb doctor` 只做只读识别 |

## 4. 两组实测快照

### 4.1 故障态（同一台机器，较早时段）

```text
curl -4 https://1.1.1.1              → connect timeout after 3998ms
curl -6 https://[2606:4700:4700::1111] → connect timeout after 3996ms
curl    https://github.com           → Resolving timed out after 4004ms
curl    https://mirrors.tuna...      → code=000
ping 192.168.1.1                     → 100% packet loss
nc 223.5.5.5 53                      → Network is unreachable
```

此时 `scutil --nwi` 仍显示 `Reachable`，但实际链路不可用：**系统状态字段不能代替真实探测**。

### 4.2 恢复态（本次复测，`fb net --timeout 4`）

```text
github.com               TCP IPv4 可达 (89ms)   HTTPS 200
api.github.com           TCP IPv4 可达 (94ms)   HTTPS 200
codeload.github.com      TCP IPv4 可达 (91ms)   HTTPS 200
raw.githubusercontent.com TCP IPv6 可达          HTTPS 失败: Connection reset by peer
objects.githubusercontent.com 部分 IPv4 超时、部分可达  HTTPS 404（TLS 完成）
ghcr.io                  TCP IPv4 可达 (82ms)   HTTPS 200
mirrors.tuna.tsinghua.edu.cn TCP IPv6/IPv4 可达 HTTPS 200
ftp.gnu.org              TCP IPv6/IPv4 可达     HTTPS 200

汇总: HTTPS 可达 7/8，TCP 可达 8/8
```

## 5. Fieldsbrew 中的产品化适配

### 5.1 `fb net` 诊断命令

新增 `fb net`，对 GitHub 主站/API/codeload/raw/objects/ghcr、TUNA、GNU 做三件事：

1. DNS 解析（同时显示 IPv4 / IPv6 / IPv4-mapped）；
2. 逐地址 TCP 连接计时；
3. HTTPS 请求，区分“TLS 握手成功但 HTTP 4xx”和“传输失败”。

它会把当前代理、镜像、超时与重试配置一并打印，并给出针对性建议。受限网络下先跑它，再决定用镜像、代理还是 `--url`。

### 5.2 下载层环境变量

| 变量 | 默认值 | 作用 |
|------|--------|------|
| `FIELDSBREW_MIRROR` / `FIELDSBREW_MIRRORS` | 空 | 镜像前缀，逗号/分号/空白分隔多个，按顺序优先，全部失败再回退原始 URL |
| `FIELDSBREW_PROXY` | 空 | 显式代理，优先级最高 |
| `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` | 空 | 兼容系统代理；大小写均识别 |
| `FIELDSBREW_NET_CONNECT_TIMEOUT` | 10 秒 | 建连超时 |
| `FIELDSBREW_NET_TIMEOUT` | 600 秒 | 单次下载整体超时 |
| `FIELDSBREW_NET_RETRIES` | 3 | 每个地址最大尝试次数 |
| `FIELDSBREW_NET_BACKOFF` | 1 秒 | 指数退避基数（1、2、4…，上限 32 秒） |

实现要点：

- 每个地址失败后会删除半成品文件再重试，避免把损坏内容当成成功下载；
- `4xx`（除 408/429）判定为永久错误，立即失败，不再做无意义重试；
- 下载请求带 `User-Agent: fb/<version>`，满足 GitHub API 要求；
- 代理 URL 在 `fb net` 输出中会隐去用户名密码。

### 5.3 自更新对受限网络的适配

`fb self-update` 支持三条路径：

1. 默认：`api.github.com/repos/<owner>/<repo>/releases/latest`（可用 `FIELDSBREW_UPDATE_REPO`、`FIELDSBREW_API_BASE` 覆盖）；
2. 指定仓库/版本：`--repo owner/repo --version v0.2.0`；
3. API 不可达时：`--url <发布包地址>` 直接下载。

下载、镜像、代理、重试、sha256、Ed25519 签名校验与原子替换共用同一套网络层。

## 6. 实测记录（自更新）

使用本地 HTTP 服务模拟 Release，验证完整链路（详见本次会话输出）：

| 场景 | 结果 |
|------|------|
| 直连 tarball URL + `--sha256` + `--version` | 通过，二进制被替换且 `--version` 输出新版本 |
| `FIELDSBREW_MIRROR` 指向可用镜像、原始 URL 不可达 | 通过，输出“已通过镜像获取” |
| 镜像不可达、原始 URL 可用 | 通过，先警告后回退原始 URL |
| `--sha256` 错误 | 中止，退出码 1，原二进制保持 0.1.0 |
| `--check` | 只查询、不下载 |
| 远端版本与本地相同 | 提示“已是最新”并跳过 |
| Ed25519 `.sig` + `--public-key` | 签名校验通过 |
| 本地 mock GitHub API（含 asset digest/sig/pub） | 完整 Release 路径通过 |
| 真实 GitHub API（`BurntSushi/ripgrep`） | 成功解析 `15.2.0`，因无 fb 资产按预期报错 |
| 不存在仓库 | `HTTP 404（不可重试）`，并提示改用 `--url` |

## 7. 复现与验收命令

```sh
# 1. 网络现状
fb net --timeout 4

# 2. 单点验证（IPv4 / IPv6 分开）
curl -4 -m 6 -o /dev/null -w '%{http_code} %{remote_ip}\n' https://github.com/
curl -6 -m 6 -o /dev/null -w '%{http_code} %{remote_ip}\n' https://www.baidu.com/
dig +short AAAA github.com @223.5.5.5

# 3. 镜像 + 重试
FIELDSBREW_MIRROR=https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles \
FIELDSBREW_NET_RETRIES=3 \
  fb install hello

# 4. 代理
FIELDSBREW_PROXY=http://127.0.0.1:7890 fb net

# 5. 自更新（API 不可达时）
fb self-update --url https://<镜像>/fb-0.2.0-x86_64-macos.tar.gz --sha256 <hash>
```

## 8. 已知限制

- `github.com` 无 AAAA，当前无法用 IPv6 直连；tap 克隆与 `git push` 依赖 IPv4/代理/镜像。
- `raw.githubusercontent.com` 的 TLS 重置无法在客户端修复，只能走镜像或本地 formula。
- 物理链路中断（ping 网关丢包）时，任何软件层配置都无效；先恢复网络。
- 本机 Wi-Fi 未配置 DNS，故障时解析会一并失败；建议显式配置 `223.5.5.5` 与 IPv6 DNS。
