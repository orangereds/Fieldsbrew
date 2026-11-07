use crate::config::Config;
use crate::platform;
use anyhow::Result;
use std::process::Command;

pub fn run(cfg: &Config) -> Result<()> {
    println!("==> fb doctor");
    let mut all_ok = true;

    macro_rules! check {
        ($label:expr, $passed:expr, $hint:expr) => {
            if $passed {
                println!("  [ok]   {}", $label);
            } else {
                println!("  [fail] {}\n         {}", $label, $hint);
                all_ok = false;
            }
        };
    }

    check!(
        "平台: Intel Mac (x86_64) + macOS",
        platform::is_intel_mac() && platform::macos_supported(),
        "Fieldsbrew 仅支持 Intel Mac 与 macOS 10.15+"
    );

    let (maj, min) = platform::macos_version().unwrap_or((0, 0));
    println!("         macOS 版本: {maj}.{min}");

    let clt = Command::new("xcode-select").arg("-p").output().is_ok()
        && Command::new("cc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    check!(
        "Xcode Command Line Tools (cc)",
        clt,
        "源码编译需要 CLT，执行: xcode-select --install"
    );

    let git = Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    check!("git（后续 tap 功能依赖）", git, "请安装 git");

    // prefix 可写性
    let prefix_ok = std::fs::create_dir_all(cfg.tmp()).is_ok()
        && std::fs::write(cfg.tmp().join(".probe"), b"ok").is_ok();
    if prefix_ok {
        let _ = std::fs::remove_file(cfg.tmp().join(".probe"));
    }
    check!(
        format!("prefix 可写: {}", cfg.prefix.display()),
        prefix_ok,
        "默认 prefix /opt/fieldsbrew 需要管理员创建:\n         sudo mkdir -p /opt/fieldsbrew && sudo chown -R $(whoami) /opt/fieldsbrew\n         或使用环境变量 FIELDSBREW_PREFIX 指定可写目录"
    );

    check!(
        "sandbox-exec 可用",
        platform::is_intel_mac() && crate::sandbox::available(),
        "缺失时构建将回退为无沙盒执行（不致命）"
    );

    // 共存体检：Homebrew 与 Fieldsbrew 并存时的同名包与 PATH 遮蔽风险
    match crate::homebrew::prefix() {
        Some(brew_prefix) => {
            println!("  [info] 检测到 Homebrew: {}", brew_prefix.display());
            if let Some(b) = crate::homebrew::brew_binary() {
                println!("         brew: {}", b.display());
            }
            let fb_names: Vec<String> = crate::cellar::all_manifests(cfg)?
                .into_iter()
                .map(|m| m.name)
                .collect();
            let overlap = crate::homebrew::overlap(&fb_names);
            if overlap.is_empty() {
                println!("  [ok]   共存：没有同名包同时由两套包管理器安装");
            } else {
                let names: Vec<String> = overlap.into_iter().map(|b| b.name).collect();
                println!(
                    "  [warn] 共存：{} 个同名包两边都已安装，建议只保留一侧: {}",
                    names.len(),
                    names.join(", ")
                );
            }

            if let Ok(path) = std::env::var("PATH") {
                let (fb_pos, brew_pos) =
                    crate::homebrew::path_positions(&path, &cfg.prefix, &brew_prefix);
                match (fb_pos, brew_pos) {
                    (Some(f), Some(b)) if f < b => {
                        println!("  [ok]   PATH：Fieldsbrew bin 优先于 Homebrew bin")
                    }
                    (Some(f), Some(b)) => println!(
                        "  [warn] PATH：Homebrew bin 位于第 {} 位、Fieldsbrew 位于第 {} 位，同名命令可能被 Homebrew 遮蔽",
                        b + 1,
                        f + 1
                    ),
                    (Some(_), None) => println!("  [info] PATH：仅包含 Fieldsbrew bin"),
                    _ => println!(
                        "  [warn] PATH：未包含 {}，fb 安装的命令可能无法直接调用",
                        cfg.prefix.join("bin").display()
                    ),
                }
            }
        }
        None => println!("  [info] 未检测到 Homebrew；Fieldsbrew 可独立使用"),
    }

    if all_ok {
        println!("==> 环境就绪");
        Ok(())
    } else {
        anyhow::bail!("体检发现问题，请按上述提示处理")
    }
}
