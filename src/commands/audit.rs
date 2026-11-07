use crate::config::Config;
use crate::formula::{find_formula, Formula};
use anyhow::Result;

/// `fb audit [names...]`：检查 formula 的必填字段、URL、校验和、依赖与构建系统。
/// 有 error 时退出码非 0；warn 只提示。
pub fn run(cfg: &Config, names: &[String]) -> Result<()> {
    let mut errors = 0;
    let mut warnings = 0;
    let mut checked = 0;

    if names.is_empty() {
        let mut seen = std::collections::HashSet::new();
        for dir in cfg.formula_dirs() {
            if !dir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                if !path.extension().map(|e| e == "toml").unwrap_or(false) {
                    continue;
                }
                let Ok(f) = Formula::load(&path) else {
                    continue;
                };
                if !seen.insert(f.package.name.clone()) {
                    continue;
                }
                let (e, w) = audit_one(cfg, &f);
                print_result(&path, &f, &e, &w);
                errors += e.len();
                warnings += w.len();
                checked += 1;
            }
        }
    } else {
        for name in names {
            let path = find_formula(cfg, name)?;
            let f = Formula::load(&path)?;
            let (e, w) = audit_one(cfg, &f);
            print_result(&path, &f, &e, &w);
            errors += e.len();
            warnings += w.len();
            checked += 1;
        }
    }

    println!("==> 审计完成：{checked} 个 formula，{errors} 个错误，{warnings} 个警告");
    if errors > 0 {
        anyhow::bail!("审计发现 {errors} 个错误");
    }
    Ok(())
}

fn print_result(path: &std::path::Path, f: &Formula, errors: &[String], warnings: &[String]) {
    println!("==> {} {} ({})", f.package.name, f.package.version, path.display());
    if errors.is_empty() && warnings.is_empty() {
        println!("  [ok] 字段、URL、校验和、依赖与构建系统均通过");
        return;
    }
    for e in errors {
        println!("  [fail] {e}");
    }
    for w in warnings {
        println!("  [warn] {w}");
    }
}

fn audit_one(cfg: &Config, f: &Formula) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if f.package.name.trim().is_empty() {
        errors.push("package.name 不能为空".to_string());
    } else if f.package.name.chars().any(|c| c.is_whitespace()) {
        errors.push("package.name 不能包含空白字符".to_string());
    }
    if f.package.version.trim().is_empty() {
        errors.push("package.version 不能为空".to_string());
    }
    if f.package.description.trim().is_empty() {
        warnings.push("package.description 为空".to_string());
    }
    if f.package.homepage.trim().is_empty() {
        warnings.push("package.homepage 为空".to_string());
    }

    if f.source.url.trim().is_empty() {
        errors.push("source.url 不能为空".to_string());
    } else if !valid_url(&f.source.url) {
        warnings.push(format!("source.url 协议不是 http(s)/ftp: {}", f.source.url));
    }
    check_sha(&f.source.sha256, "source.sha256", &mut errors, &mut warnings);

    match f.build.system.trim() {
        "" | "autotools" | "cmake" | "make" => {}
        other => errors.push(format!("build.system `{other}` 不受支持（autotools|cmake|make）")),
    }

    match f.deps() {
        Ok(deps) => {
            for d in deps {
                if find_formula(cfg, &d.name).is_err() {
                    errors.push(format!("依赖 `{}` 在 formula 目录中不存在", d.name));
                }
            }
        }
        Err(e) => errors.push(format!("依赖声明无法解析: {e}")),
    }

    if let Some(b) = &f.bottle {
        if !b.url.trim().is_empty() {
            check_sha(&b.sha256, "bottle.sha256", &mut errors, &mut warnings);
        }
        for (tag, t) in &b.tags {
            if t.url.trim().is_empty() {
                errors.push(format!("bottle.tags.{tag}.url 为空"));
            }
            check_sha(
                &t.sha256,
                &format!("bottle.tags.{tag}.sha256"),
                &mut errors,
                &mut warnings,
            );
        }
        if !b.signature.trim().is_empty() && b.public_key.trim().is_empty() {
            errors.push("bottle 配置了 signature 但缺少 public_key".to_string());
        }
        if b.signature.trim().is_empty() && !b.public_key.trim().is_empty() {
            warnings.push("bottle 配置了 public_key 但没有 signature".to_string());
        }
    }

    if let Some(s) = &f.service {
        if s.run.trim().is_empty() {
            errors.push("[service].run 不能为空".to_string());
        }
    }

    (errors, warnings)
}

fn check_sha(
    sha: &str,
    field: &str,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let sha = sha.trim();
    if sha.is_empty() {
        warnings.push(format!("{field} 为空，安装时无法校验完整性"));
    } else if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        errors.push(format!("{field} 不是合法的 64 位 hex sha256: {sha}"));
    }
}

fn valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")
        || url.starts_with("ftps://")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(extra: &str) -> Formula {
        Formula::parse(&format!(
            r#"
[package]
name = "demo"
version = "1.0.0"
description = "demo"
homepage = "https://example.com"

[source]
url = "https://example.com/demo-1.0.0.tar.gz"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
{extra}
"#
        ))
        .unwrap()
    }

    #[test]
    fn test_audit_clean_formula() {
        let f = formula("");
        let dir = std::env::temp_dir().join(format!("fb-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config {
            prefix: dir.clone(),
            verbose: false,
        };
        let (e, w) = audit_one(&cfg, &f);
        assert!(e.is_empty(), "errors: {e:?}");
        assert!(w.is_empty(), "warnings: {w:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_audit_bad_fields() {
        let base = std::env::temp_dir().join(format!("fb-audit2-{}", std::process::id()));
        std::fs::create_dir_all(base.join("Formula")).unwrap();
        std::fs::write(
            base.join("Formula/dep.toml"),
            "[package]\nname=\"dep\"\nversion=\"1.0\"\n[source]\nurl=\"https://example.com/dep.tar.gz\"\n",
        )
        .unwrap();
        let cfg = Config {
            prefix: base.clone(),
            verbose: false,
        };

        let f = formula(
            r#"
[build]
system = "meson"

[bottle]
url = "https://example.com/demo.bottle.tar.gz"
sha256 = "xyz"
signature = "https://example.com/demo.sig"
"#,
        );
        let (e, w) = audit_one(&cfg, &f);
        assert!(e.iter().any(|x| x.contains("build.system")));
        assert!(e.iter().any(|x| x.contains("bottle.sha256")));
        assert!(e.iter().any(|x| x.contains("public_key")));
        assert!(w.is_empty(), "warnings: {w:?}");

        let f = formula(
            r#"
[service]
run = ""
"#,
        );
        let (e, _) = audit_one(&cfg, &f);
        assert!(e.iter().any(|x| x.contains("[service].run")));
        std::fs::remove_dir_all(&base).ok();
    }
}
