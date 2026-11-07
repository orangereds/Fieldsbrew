use anyhow::{anyhow, bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use std::path::Path;

/// 从 32 字节 seed 的 hex（64 个字符）派生签名密钥。
pub fn signing_key_from_hex(s: &str) -> Result<SigningKey> {
    let cleaned: String = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("");
    let bytes = hex::decode(&cleaned).context("签名密钥不是合法 hex")?;
    if bytes.len() != 32 {
        bail!(
            "签名密钥应为 32 字节 seed（64 位 hex），实际 {} 字节",
            bytes.len()
        );
    }
    let seed: [u8; 32] = bytes.as_slice().try_into().unwrap();
    Ok(SigningKey::from_bytes(&seed))
}

pub fn public_key_hex(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

pub fn sign_bytes(key: &SigningKey, data: &[u8]) -> Vec<u8> {
    key.sign(data).to_bytes().to_vec()
}

/// 校验分离式 Ed25519 签名；sig 可为 64 字节原始数据或 hex 文本。
pub fn verify_bytes(data: &[u8], sig_raw: &[u8], public_key_hex: &str) -> Result<()> {
    let pk = hex::decode(public_key_hex.trim()).context("公钥不是合法 hex")?;
    let pk_arr: [u8; 32] = pk
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Ed25519 公钥应为 32 字节，实际 {} 字节", pk.len()))?;
    let vk = VerifyingKey::from_bytes(&pk_arr).context("无效的 Ed25519 公钥")?;

    let sig_bytes = if sig_raw.len() == 64 {
        sig_raw.to_vec()
    } else {
        let text = String::from_utf8_lossy(sig_raw).trim().to_string();
        hex::decode(&text).context("签名文件既非 64 字节原始数据，也不是合法 hex")?
    };
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Ed25519 签名应为 64 字节，实际 {} 字节", sig_bytes.len()))?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify_strict(data, &sig)
        .map_err(|e| anyhow!("Ed25519 签名不匹配（文件被篡改或签名错误）: {e}"))?;
    Ok(())
}

/// 读取公钥：`hex` 或包含公钥的文件路径。
pub fn read_public_key(spec: &str) -> Result<String> {
    let p = Path::new(spec);
    if p.is_file() {
        let s = std::fs::read_to_string(p)
            .with_context(|| format!("读取公钥失败: {}", p.display()))?;
        Ok(s.trim().to_string())
    } else {
        Ok(spec.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let key = signing_key_from_hex(&hex::encode([7u8; 32])).unwrap();
        let pk = public_key_hex(&key);
        let data = b"fieldsbrew release";

        let sig = sign_bytes(&key, data);
        assert_eq!(sig.len(), 64);
        verify_bytes(data, &sig, &pk).unwrap();

        // hex 形式的签名也应支持
        let sig_hex = hex::encode(&sig);
        verify_bytes(data, sig_hex.as_bytes(), &pk).unwrap();

        assert!(verify_bytes(b"tampered", &sig, &pk).is_err());
    }

    #[test]
    fn test_bad_key() {
        assert!(signing_key_from_hex("abcd").is_err());
        assert!(signing_key_from_hex("zz").is_err());
    }
}
