use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Tgz,
    Txz,
    Tbz2,
    Tar,
}

/// 按 URL/文件名后缀识别压缩格式（忽略查询串）
pub fn detect(url: &str) -> Result<Format> {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
    if path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        Ok(Format::Tgz)
    } else if path.ends_with(".tar.xz") || path.ends_with(".txz") {
        Ok(Format::Txz)
    } else if path.ends_with(".tar.bz2") || path.ends_with(".tbz2") {
        Ok(Format::Tbz2)
    } else if path.ends_with(".tar") {
        Ok(Format::Tar)
    } else {
        bail!("无法识别的压缩格式: {url}（支持 tar.gz / tar.xz / tar.bz2 / tar）")
    }
}

/// 解压归档到 dest。若归档内所有条目共享同一顶层目录（如 hello-2.12.1/），
/// 自动剥离该层，文件直接落在 dest 下。
///
/// 实现：tar 条目的数据流只能顺序消费，无法先收集再解包，因此统一先解码成
/// 临时 .tar，第一遍扫描判定顶层目录，第二遍按官方用法边迭代边解包。
pub fn extract(archive: &Path, format: Format, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let tmp_tar = match format {
        Format::Tar => archive.to_path_buf(),
        other => decode_to_tar(archive, other)?,
    };
    let r = (|| {
        let strip = detect_strip(&tmp_tar)?;
        let file = std::fs::File::open(&tmp_tar)?;
        unpack(tar::Archive::new(std::io::BufReader::new(file)), dest, strip)
    })();
    if format != Format::Tar {
        let _ = std::fs::remove_file(&tmp_tar);
    }
    r
}

/// 把压缩流解码成临时 .tar 文件
fn decode_to_tar(archive: &Path, format: Format) -> Result<PathBuf> {
    let tar_path = archive.with_extension("fb.tar");
    let mut out = std::fs::File::create(&tar_path)
        .with_context(|| format!("创建临时文件失败: {}", tar_path.display()))?;
    let file = std::fs::File::open(archive)?;
    match format {
        Format::Tgz => {
            std::io::copy(&mut flate2::read::GzDecoder::new(file), &mut out)?;
        }
        Format::Tbz2 => {
            std::io::copy(&mut bzip2_rs::DecoderReader::new(file), &mut out)?;
        }
        Format::Txz => {
            lzma_rs::xz_decompress(&mut std::io::BufReader::new(file), &mut out)
                .map_err(|e| anyhow::anyhow!("xz 解压失败: {e}"))?;
        }
        Format::Tar => unreachable!(),
    }
    Ok(tar_path)
}

/// 扫描归档内条目，判断是否所有条目共享同一顶层目录
fn detect_strip(tar_path: &Path) -> Result<bool> {
    let file = std::fs::File::open(tar_path)?;
    let mut ar = tar::Archive::new(std::io::BufReader::new(file));
    let mut tops = std::collections::HashSet::new();
    for e in ar.entries()? {
        let e = e?;
        let p = e.path()?.to_path_buf();
        if let Some(t) = first_normal_component(&p) {
            tops.insert(t.as_os_str().to_os_string());
            if tops.len() > 1 {
                return Ok(false);
            }
        }
    }
    Ok(tops.len() == 1)
}

fn unpack<R: Read>(mut ar: tar::Archive<R>, dest: &Path, strip: bool) -> Result<()> {
    // 必须边迭代边 unpack：Entry 的数据是对底层流的前向引用
    for e in ar.entries()? {
        let mut e = e?;
        let orig = e.path()?.to_path_buf();
        let stripped: PathBuf = if strip {
            let mut comps = normal_components(&orig);
            comps.next(); // 丢弃顶层目录
            comps.collect()
        } else {
            normal_components(&orig).collect()
        };
        if stripped.as_os_str().is_empty() {
            continue;
        }
        check_traversal(&stripped)?;
        let target = dest.join(stripped);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        e.unpack(&target)
            .with_context(|| format!("解包失败: {}", orig.display()))?;
    }
    Ok(())
}

fn normal_components(p: &Path) -> impl Iterator<Item = Component<'_>> + '_ {
    p.components().filter(|c| !matches!(c, Component::CurDir))
}

fn first_normal_component(p: &Path) -> Option<Component<'_>> {
    normal_components(p).next()
}

fn check_traversal(p: &Path) -> Result<()> {
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("归档内包含可疑路径，拒绝解包: {}", p.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect() {
        assert_eq!(detect("https://x.com/a/b-1.0.tar.gz").unwrap(), Format::Tgz);
        assert_eq!(detect("https://x.com/a.tar.gz?sig=abc").unwrap(), Format::Tgz);
        assert_eq!(detect("file.txz").unwrap(), Format::Txz);
        assert_eq!(detect("f.tar.bz2").unwrap(), Format::Tbz2);
        assert_eq!(detect("f.tar").unwrap(), Format::Tar);
        assert!(detect("f.zip").is_err());
    }

    #[test]
    fn test_traversal_check() {
        assert!(check_traversal(Path::new("bin/hello")).is_ok());
        assert!(check_traversal(Path::new("../evil")).is_err());
        assert!(check_traversal(Path::new("/abs/path")).is_err());
    }

    #[test]
    fn test_extract_strips_top_dir() {
        // 构造一个带顶层目录的 tar.gz：top/a.txt + top/sub/b.txt
        let dir = std::env::temp_dir().join(format!("fb-arch-{}", std::process::id()));
        let src = dir.join("pkg-1.0");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"aaa").unwrap();
        std::fs::write(src.join("sub/b.txt"), b"bbb").unwrap();
        let tar_path = dir.join("pkg-1.0.tar.gz");
        {
            let f = std::fs::File::create(&tar_path).unwrap();
            let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("pkg-1.0", &src).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let dest = dir.join("out");
        extract(&tar_path, Format::Tgz, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(dest.join("sub/b.txt")).unwrap(), b"bbb");
        std::fs::remove_dir_all(&dir).ok();
    }
}
