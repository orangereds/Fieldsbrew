# 从 Homebrew 迁移到 Fieldsbrew

Fieldsbrew 面向 Homebrew 停止 Intel Mac 支持后的过渡期。迁移原则是 **并行安装、只读 Homebrew、人工验收后再卸载**：`fb migrate` 不会修改或卸载 Homebrew 的文件。

## 0. 安装 Fieldsbrew

```sh
tar -xzf fb-<version>-x86_64-macos.tar.gz
cd fieldsbrew-<version>
./install.sh
eval "$(/opt/fieldsbrew/bin/fb shellenv)"
fb doctor
```

建议使用独立 prefix（默认 `/opt/fieldsbrew`），不要与 Homebrew 的 `/usr/local` 混用。

## 1. 盘点 Homebrew 现状

```sh
fb list --brew          # 只读列出 Homebrew Cellar 中的 formula
fb migrate              # 不带参数：显示已安装/可迁移/缺少 formula
```

`fb migrate` 需要 Fieldsbrew 侧存在同名 formula。Homebrew 的 Ruby formula 需要先转换。

## 2. 转换 formula

从 Homebrew tap 或本地 formula 目录转换（输出默认写入 Fieldsbrew 的 `Formula/`，不会写回 Homebrew）：

```sh
fb convert /usr/local/Homebrew/Library/Taps/<user>/homebrew-<repo>/Formula \
  --out /opt/fieldsbrew/Formula

# 单个文件，结果打印到 stdout
fb convert hello.rb

# 转换后检查
fb audit hello
fb info hello
```

转换是 Ruby DSL 子集静态解析。请重点复核：

- 源码 `url` / `sha256`（git 源、动态 URL 无法自动转换）。
- bottle URL（Homebrew 的 ghcr 地址无法直接推导时会留空）。
- `depends_on` 的构建依赖与系统依赖。
- `service` 的 `run` 命令。
- 构建系统（autotools / cmake / make）与额外参数。

## 3. 在 Fieldsbrew 中安装

```sh
fb migrate hello         # 读取 Homebrew 的版本，用 Fieldsbrew 独立 prefix 重装
fb list                  # 确认 Fieldsbrew 侧已安装
which hello              # 确认命令来自 /opt/fieldsbrew/bin
```

关闭沙盒或保留编译目录：

```sh
fb migrate hello --no-sandbox
fb migrate hello --keep-tmp
```

`fb migrate` 只迁移已安装项；批量迁移时逐个执行更便于排查。

## 4. 共存检查与 PATH

```sh
fb doctor
```

关注两类提示：

- **同名包两边都已安装**：迁移期可保留，但建议尽快只留一侧。
- **PATH 顺序**：`eval "$(fb shellenv)"` 应出现在 Homebrew 的 env 之后，确保 `/opt/fieldsbrew/bin` 优先。

## 5. services

Homebrew services 不会自动迁移。需要：

1. 转换的 formula 中带有 `[service]` 配置（`fb convert` 会尝试解析 `service do` 块）。
2. 停止 Homebrew 侧服务（手动）：`brew services stop <name>`。
3. 启动 Fieldsbrew 服务：`fb services start <name>`。
4. 查看状态：`fb services list`。

## 6. Cask / GUI 应用

```sh
brew list --cask                 # 盘点
fb convert --cask <cask.rb> --out /opt/fieldsbrew/Casks
fb cask search <name>
fb cask install <token>
fb cask list
```

- `.app` 默认安装到 `/Applications`；可用 `FIELDSBREW_APPDIR` 指定其他目录。
- 若 `/Applications` 已有同名应用，先手动移除，或用 `--force` 覆盖。
- `pkg` 安装器需要 sudo，Fieldsbrew 暂不支持，请保留 Homebrew 侧或手动安装。

## 7. bottle 与源码回退

Fieldsbrew 会优先使用当前 macOS 平台标签对应的 Intel bottle；没有匹配 bottle 时自动回退源码构建。源码构建需要 Xcode Command Line Tools：

```sh
xcode-select --install
```

若公司网络限制访问 bottle 源，可设置镜像：

```sh
export FIELDSBREW_MIRROR="https://your-mirror.example.com"
export HTTPS_PROXY="http://proxy.example.com:8080"
```

镜像会按原始 URL 的 path 拼接；失败自动回退原始地址。

## 8. 验收与卸载 Homebrew 侧

建议观察一段时间后再卸载：

```sh
which <command>                  # 确认来自 Fieldsbrew
fb doctor                        # 无同名包/PATH 警告
brew uninstall <name>            # 由你手动决定，fb 不会自动执行
```

Homebrew 本体是否保留可自行决定；Fieldsbrew 不依赖它运行。

## 常见问题

| 现象 | 处理 |
|------|------|
| prefix 不可写 | `sudo mkdir -p /opt/fieldsbrew && sudo chown -R "$USER" /opt/fieldsbrew`，或设置 `FIELDSBREW_PREFIX` |
| 找不到 formula | 用 `fb convert` 生成，并确认输出在 `<prefix>/Formula` 或 `FIELDSBREW_FORMULA_DIR` |
| bottle 下载失败 | 检查 `FIELDSBREW_MIRROR`/代理；失败会自动尝试源码构建 |
| 版本与 brew 不一致 | `fb migrate` 按 Fieldsbrew formula 的版本安装；转换后请复核 `version` |
| 命令被 brew 遮蔽 | 调整 PATH 顺序，`fb doctor` 会提示 |
