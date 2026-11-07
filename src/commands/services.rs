use crate::cellar;
use crate::config::Config;
use crate::formula::{find_formula, Formula, Service};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Run,
    Start,
    Stop,
    Restart,
    List,
    Cleanup,
}

/// `fb services` 入口：launchd 用户级服务管理，语义对齐 `brew services`。
pub fn run(cfg: &Config, action: Action, name: Option<&str>) -> Result<()> {
    match action {
        Action::List => list(cfg),
        Action::Cleanup => cleanup(cfg),
        _ => {
            let name = name.ok_or_else(|| anyhow::anyhow!("请指定服务名，例如: fb services start foo"))?;
            one(cfg, action, name)
        }
    }
}

fn one(cfg: &Config, action: Action, name: &str) -> Result<()> {
    let formula_path = find_formula(cfg, name)?;
    let f = Formula::load(&formula_path)?;
    let svc = f
        .service
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("{name} 的 formula 未定义 [service]"))?;
    if svc.run.trim().is_empty() {
        bail!("{name} 的 [service].run 为空");
    }

    let label = label(name);
    let plist = plist_path(name);
    match action {
        Action::Start => {
            write_plist(cfg, name, svc, &plist, true)?;
            bootstrap(&plist, &label)?;
            println!("==> 已启动 {name}（{label}）");
        }
        Action::Run => {
            let at_load = svc.run_at_load;
            write_plist(cfg, name, svc, &plist, at_load)?;
            bootstrap(&plist, &label)?;
            if !at_load {
                kickstart(&label)?;
            }
            println!("==> 已运行 {name}（{label}，不随登录启动）");
        }
        Action::Stop => {
            bootout(&label);
            println!("==> 已停止 {name}");
        }
        Action::Restart => {
            if !plist.exists() {
                anyhow::bail!("{name} 尚未注册为服务，请先执行 `fb services start {name}`");
            }
            if kickstart(&label).is_err() {
                bootstrap(&plist, &label)?;
                kickstart(&label)?;
            }
            println!("==> 已重启 {name}");
        }
        Action::List | Action::Cleanup => unreachable!(),
    }
    Ok(())
}

fn list(cfg: &Config) -> Result<()> {
    let manifests = cellar::all_manifests(cfg)?;
    let mut rows = Vec::new();
    for m in &manifests {
        let Ok(path) = find_formula(cfg, &m.name) else {
            continue;
        };
        let Ok(f) = Formula::load(&path) else {
            continue;
        };
        if f.service.is_none() {
            continue;
        }
        let plist = plist_path(&m.name);
        let registered = plist.exists();
        let running = registered && status(&label(&m.name));
        rows.push((m.name.clone(), m.version.clone(), registered, running, plist));
    }

    if rows.is_empty() {
        println!("没有带 [service] 定义的已安装包。");
        return Ok(());
    }

    println!("{:<24} {:<10} {:<10} {}", "名称", "版本", "状态", "plist");
    for (name, version, registered, running, plist) in rows {
        let state = if !registered {
            "未注册"
        } else if running {
            "started"
        } else {
            "stopped"
        };
        println!("{:<24} {:<10} {:<10} {}", name, version, state, plist.display());
    }
    Ok(())
}

/// 清理已卸载或已不再定义 [service] 的 plist
fn cleanup(cfg: &Config) -> Result<()> {
    let mut removed = 0;
    for plist in all_plists()? {
        let Some(name) = plist_name(&plist) else {
            continue;
        };
        let keep = cellar::read_manifest(cfg, &name)?.is_some()
            && find_formula(cfg, &name)
                .ok()
                .and_then(|p| Formula::load(&p).ok())
                .map(|f| f.service.is_some())
                .unwrap_or(false);
        if keep {
            continue;
        }
        bootout(&label(&name));
        std::fs::remove_file(&plist)
            .with_context(|| format!("删除 plist 失败: {}", plist.display()))?;
        println!("    已清理 {name}: {}", plist.display());
        removed += 1;
    }
    println!("==> 清理完成，共移除 {removed} 个服务配置");
    Ok(())
}

fn write_plist(
    cfg: &Config,
    name: &str,
    svc: &Service,
    plist: &Path,
    run_at_load: bool,
) -> Result<()> {
    let log_dir = cfg.prefix.join("var/log");
    std::fs::create_dir_all(&log_dir)?;
    let out_log = if svc.log_path.trim().is_empty() {
        log_dir.join(format!("{name}.log"))
    } else {
        PathBuf::from(svc.log_path.trim())
    };
    let err_log = if svc.error_log_path.trim().is_empty() {
        log_dir.join(format!("{name}.err.log"))
    } else {
        PathBuf::from(svc.error_log_path.trim())
    };
    let working_dir = if svc.working_dir.trim().is_empty() {
        cfg.prefix.clone()
    } else {
        PathBuf::from(svc.working_dir.trim())
    };
    let args = program_args(cfg, svc)?;
    let content = render_plist(
        &label(name),
        &args,
        run_at_load,
        svc.keep_alive,
        &working_dir,
        &out_log,
        &err_log,
        &svc.env,
        &cfg.prefix,
    );

    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 LaunchAgents 目录失败: {}", parent.display()))?;
    }
    std::fs::write(plist, content)
        .with_context(|| format!("写入 plist 失败: {}", plist.display()))?;
    Ok(())
}

fn program_args(cfg: &Config, svc: &Service) -> Result<Vec<String>> {
    let run = svc.run.trim();
    if run.is_empty() {
        bail!("[service].run 为空");
    }
    let mut parts: Vec<String> = run.split_whitespace().map(|s| s.to_string()).collect();
    let first = parts.first().cloned().unwrap_or_default();
    let bin = if first.starts_with('/') {
        PathBuf::from(&first)
    } else if first.contains('/') {
        cfg.prefix.join(&first)
    } else {
        cfg.prefix.join("bin").join(&first)
    };
    if !bin.exists() {
        eprintln!("警告: 服务命令不存在: {}（请确认已安装该包）", bin.display());
    }
    parts[0] = bin.display().to_string();
    Ok(parts)
}

#[allow(clippy::too_many_arguments)]
fn render_plist(
    label: &str,
    args: &[String],
    run_at_load: bool,
    keep_alive: bool,
    working_dir: &Path,
    out_log: &Path,
    err_log: &Path,
    env: &std::collections::BTreeMap<String, String>,
    prefix: &Path,
) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    s.push_str("<plist version=\"1.0\">\n<dict>\n");
    s.push_str(&format!("  <key>Label</key>\n  <string>{}</string>\n", esc(label)));
    s.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    for a in args {
        s.push_str(&format!("    <string>{}</string>\n", esc(a)));
    }
    s.push_str("  </array>\n");
    s.push_str(&format!(
        "  <key>RunAtLoad</key>\n  <{0}/>\n",
        if run_at_load { "true" } else { "false" }
    ));
    s.push_str(&format!(
        "  <key>KeepAlive</key>\n  <{0}/>\n",
        if keep_alive { "true" } else { "false" }
    ));
    s.push_str(&format!(
        "  <key>WorkingDirectory</key>\n  <string>{}</string>\n",
        esc(&working_dir.display().to_string())
    ));
    s.push_str(&format!(
        "  <key>StandardOutPath</key>\n  <string>{}</string>\n",
        esc(&out_log.display().to_string())
    ));
    s.push_str(&format!(
        "  <key>StandardErrorPath</key>\n  <string>{}</string>\n",
        esc(&err_log.display().to_string())
    ));

    s.push_str("  <key>EnvironmentVariables</key>\n  <dict>\n");
    let default_path = format!(
        "{}:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        prefix.join("bin").display()
    );
    s.push_str(&format!(
        "    <key>PATH</key>\n    <string>{}</string>\n",
        esc(&default_path)
    ));
    for (k, v) in env {
        s.push_str(&format!(
            "    <key>{}</key>\n    <string>{}</string>\n",
            esc(k),
            esc(v)
        ));
    }
    s.push_str("  </dict>\n</dict>\n</plist>\n");
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn label(name: &str) -> String {
    format!("fieldsbrew.mxcl.{name}")
}

fn launch_agents_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("无法确定 HOME 目录")?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents"))
}

fn plist_path(name: &str) -> PathBuf {
    launch_agents_dir()
        .map(|d| d.join(format!("{}.plist", label(name))))
        .unwrap_or_else(|_| PathBuf::from(format!("{}.plist", label(name))))
}

fn plist_name(plist: &Path) -> Option<String> {
    let file = plist.file_name()?.to_string_lossy();
    file.strip_prefix("fieldsbrew.mxcl.")
        .and_then(|s| s.strip_suffix(".plist"))
        .map(|s| s.to_string())
}

fn all_plists() -> Result<Vec<PathBuf>> {
    let dir = launch_agents_dir()?;
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    Ok(entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("fieldsbrew.mxcl.") && n.ends_with(".plist")
                })
                .unwrap_or(false)
        })
        .collect())
}

fn uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "501".to_string())
}

fn bootstrap(plist: &Path, label: &str) -> Result<()> {
    bootout(label);
    let domain = format!("gui/{}", uid());
    let out = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(plist)
        .output()
        .context("无法执行 launchctl bootstrap")?;
    if out.status.success() {
        return Ok(());
    }
    // 兼容旧版 macOS
    let legacy = Command::new("launchctl").arg("load").arg("-w").arg(plist).output();
    if legacy.map(|o| o.status.success()).unwrap_or(false) {
        return Ok(());
    }
    let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
    bail!("launchctl 加载服务失败: {msg}");
}

fn bootout(label: &str) {
    let domain = format!("gui/{}", uid());
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();
}

fn kickstart(label: &str) -> Result<()> {
    let domain = format!("gui/{}", uid());
    let out = Command::new("launchctl")
        .args(["kickstart", "-k", &format!("{domain}/{label}")])
        .output()
        .context("无法执行 launchctl kickstart")?;
    if out.status.success() {
        Ok(())
    } else {
        bail!("launchctl kickstart 失败: {}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

fn status(label: &str) -> bool {
    Command::new("launchctl")
        .args(["print", &format!("gui/{}/{label}", uid())])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_render_plist() {
        let env = BTreeMap::from([("FOO".to_string(), "bar".to_string())]);
        let out = render_plist(
            "fieldsbrew.mxcl.demo",
            &["/opt/fieldsbrew/bin/demo".to_string(), "--flag".to_string()],
            true,
            true,
            Path::new("/opt/fieldsbrew"),
            Path::new("/opt/fieldsbrew/var/log/demo.log"),
            Path::new("/opt/fieldsbrew/var/log/demo.err.log"),
            &env,
            Path::new("/opt/fieldsbrew"),
        );
        assert!(out.contains("<string>fieldsbrew.mxcl.demo</string>"));
        assert!(out.contains("<string>--flag</string>"));
        assert!(out.contains("<key>RunAtLoad</key>\n  <true/>"));
        assert!(out.contains("<key>KeepAlive</key>\n  <true/>"));
        assert!(out.contains("<key>FOO</key>"));
        assert!(out.contains("<string>bar</string>"));
        assert!(out.contains("PATH"));
        assert!(out.ends_with("</plist>\n"));
    }

    #[test]
    fn test_program_args_resolution() {
        let cfg = Config {
            prefix: PathBuf::from("/opt/fieldsbrew"),
            verbose: false,
        };
        let svc = Service {
            run: "opt/demo/bin/demo --port 8080".to_string(),
            ..Default::default()
        };
        let args = program_args(&cfg, &svc).unwrap();
        assert_eq!(args[0], "/opt/fieldsbrew/opt/demo/bin/demo");
        assert_eq!(args[1], "--port");
        assert_eq!(args[2], "8080");

        let svc = Service {
            run: "demo".to_string(),
            ..Default::default()
        };
        assert_eq!(
            program_args(&cfg, &svc).unwrap()[0],
            "/opt/fieldsbrew/bin/demo"
        );
    }

    #[test]
    fn test_plist_name() {
        assert_eq!(
            plist_name(Path::new("/x/fieldsbrew.mxcl.demo.plist")),
            Some("demo".to_string())
        );
        assert_eq!(plist_name(Path::new("/x/homebrew.mxcl.demo.plist")), None);
    }
}
