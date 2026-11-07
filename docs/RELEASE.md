# 发布流程

## 产物

每次推送 `v*` tag，GitHub Actions 在 `macos-13`（原生 Intel）上构建并发布：

- `fb-<version>-x86_64-macos.tar.gz`：包含 `fb`、`install.sh`、`README.md`、`docs/`、`Formula/`。
- `sha256sums.txt`：压缩包的 SHA-256。
- 可选签名：`fb-<version>-x86_64-macos.tar.gz.sig`、`sha256sums.txt.sig` 及对应 `.pub` 公钥文件。

## 签名密钥

Fieldsbrew 使用 Ed25519 分离签名。私钥是 32 字节 seed 的 hex（64 个十六进制字符）：

```sh
openssl rand -hex 32 > fb-signing-key.txt
```

把内容保存到 GitHub Actions Secret：`FIELDSBREW_SIGNING_KEY`。不要提交私钥。

本地签名：

```sh
export FIELDSBREW_SIGNING_KEY="$(cat fb-signing-key.txt)"
fb sign fb-<version>-x86_64-macos.tar.gz
fb sign sha256sums.txt
```

`fb sign` 会生成 `<file>.sig`（hex 文本）与 `<file>.pub`（公钥 hex）。

校验：

```sh
fb verify fb-<version>-x86_64-macos.tar.gz
# 或显式指定
fb verify <file> --signature <file>.sig --public-key <file>.pub
```

## 本地打包

```sh
cargo build --release --locked --target x86_64-apple-darwin
VERSION=0.1.0
DIST="fieldsbrew-${VERSION}"
mkdir -p "dist/${DIST}"
cp target/x86_64-apple-darwin/release/fb "dist/${DIST}/"
cp install.sh README.md "dist/${DIST}/"
cp -R docs Formula "dist/${DIST}/"
chmod +x "dist/${DIST}/fb" "dist/${DIST}/install.sh"
tar -czf "fb-${VERSION}-x86_64-macos.tar.gz" -C dist "${DIST}"
shasum -a 256 "fb-${VERSION}-x86_64-macos.tar.gz" > sha256sums.txt
```

## CI 发布

工作流 `.github/workflows/release.yml`：

1. 检出代码并安装 Rust + `x86_64-apple-darwin` target。
2. `cargo build --release --locked`。
3. 打包并生成 `sha256sums.txt`。
4. 若配置了 `FIELDSBREW_SIGNING_KEY`，对压缩包与校验和文件签名，并立即用 `fb verify` 自检。
5. 上传到 GitHub Release；未配置密钥时只发布压缩包与 SHA-256。

## 安装脚本

`install.sh` 仅做最小安装：

- 检查 `x86_64`。
- 从压缩包目录复制 `fb` 到 `<prefix>/bin/fb`（默认 `/opt/fieldsbrew`）。
- 打印 `fb shellenv` 与 `fb doctor` 提示。

签名校验在下载/解压阶段完成，安装脚本不重复实现密码学逻辑。

## 自更新

已安装的用户可用 `fb self-update` 更新自身二进制：

```sh
fb self-update --check                  # 仅查询
fb self-update                          # 更新到最新版
fb self-update --version v0.2.0         # 指定版本
fb self-update --url <发布包地址>        # API 不可达时直连
fb self-update --repo owner/repo        # 指定仓库
```

流程：查询 Release → 下载 `fb-<version>-x86_64-macos.tar.gz` → 校验 sha256（Release digest /
`sha256sums.txt` / `--sha256`）→ 若存在 `.sig` + `.pub` 则做 Ed25519 校验 → 解压 → 执行
`fb --version` 自检 → 原子替换当前二进制。

- 默认仓库为 `fieldsbrew/fieldsbrew`，可用 `FIELDSBREW_UPDATE_REPO` 覆盖；
- API 基地址可用 `FIELDSBREW_API_BASE` 指向镜像；
- 下载同样走 `FIELDSBREW_MIRROR` / `FIELDSBREW_PROXY` 与重试策略；
- 安装在系统目录时需 `sudo fb self-update`。
