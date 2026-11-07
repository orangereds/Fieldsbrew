use crate::sign;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// `fb sign <file>`：为发布产物生成 Ed25519 分离签名。
/// 私钥来自 `--key-file` 或环境变量 `FIELDSBREW_SIGNING_KEY`（32 字节 seed 的 hex）。
pub fn run_sign(file: &Path, key_file: Option<&Path>, output: Option<&Path>) -> Result<()> {
    if !file.is_file() {
        bail!("文件不存在: {}", file.display());
    }
    let key_hex = match key_file {
        Some(p) => std::fs::read_to_string(p)
            .with_context(|| format!("读取签名密钥失败: {}", p.display()))?,
        None => std::env::var("FIELDSBREW_SIGNING_KEY").context(
            "未提供签名密钥：请设置 FIELDSBREW_SIGNING_KEY，或使用 --key-file <hex 文件>",
        )?,
    };
    let key = sign::signing_key_from_hex(&key_hex)?;
    let data =
        std::fs::read(file).with_context(|| format!("读取文件失败: {}", file.display()))?;
    let sig = sign::sign_bytes(&key, &data);

    let out = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(format!("{}.sig", file.display())));
    std::fs::write(&out, format!("{}\n", hex::encode(&sig)))
        .with_context(|| format!("写入签名失败: {}", out.display()))?;

    let pub_path = PathBuf::from(format!("{}.pub", file.display()));
    let pk = sign::public_key_hex(&key);
    std::fs::write(&pub_path, format!("{pk}\n"))
        .with_context(|| format!("写入公钥失败: {}", pub_path.display()))?;

    println!("签名文件: {}", out.display());
    println!("公钥文件: {}", pub_path.display());
    println!("公钥: {pk}");
    Ok(())
}

/// `fb verify <file>`：校验 Ed25519 分离签名。
pub fn run_verify(file: &Path, signature: Option<&Path>, public_key: Option<&str>) -> Result<()> {
    if !file.is_file() {
        bail!("文件不存在: {}", file.display());
    }
    let sig_path = signature
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(format!("{}.sig", file.display())));
    if !sig_path.is_file() {
        bail!(
            "签名文件不存在: {}（可用 --signature 指定）",
            sig_path.display()
        );
    }
    let data =
        std::fs::read(file).with_context(|| format!("读取文件失败: {}", file.display()))?;
    let raw = std::fs::read(&sig_path)
        .with_context(|| format!("读取签名失败: {}", sig_path.display()))?;

    let pk = match public_key {
        Some(s) => sign::read_public_key(s)?,
        None => {
            let default = PathBuf::from(format!("{}.pub", file.display()));
            if !default.is_file() {
                bail!(
                    "公钥文件不存在: {}（可用 --public-key <hex|文件> 指定）",
                    default.display()
                );
            }
            std::fs::read_to_string(&default)?.trim().to_string()
        }
    };
    sign::verify_bytes(&data, &raw, &pk)?;
    println!("签名有效: {}", file.display());
    Ok(())
}
