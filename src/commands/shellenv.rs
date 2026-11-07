use crate::config::Config;
use anyhow::{bail, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Sh,
    Fish,
    Csh,
}

/// `fb shellenv`：输出可直接 eval 的环境变量设置，语义对齐 `brew shellenv`。
/// 省略 --shell 时按 $SHELL 自动识别 sh/bash/zsh、fish、csh/tcsh。
pub fn run(cfg: &Config, shell: Option<&str>) -> Result<()> {
    let kind = detect_shell(shell)?;
    print!("{}", render(&cfg.prefix.display().to_string(), kind));
    Ok(())
}

fn detect_shell(explicit: Option<&str>) -> Result<ShellKind> {
    let shell = match explicit {
        Some(s) => s.to_string(),
        None => std::env::var("SHELL").unwrap_or_default(),
    };
    let name = Path::new(&shell)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| shell.clone());
    match name.as_str() {
        "fish" => Ok(ShellKind::Fish),
        "csh" | "tcsh" => Ok(ShellKind::Csh),
        "" | "sh" | "bash" | "zsh" | "dash" | "ksh" => Ok(ShellKind::Sh),
        other => bail!("不支持的 shell: {other}（支持 sh/bash/zsh/fish/csh/tcsh）"),
    }
}

pub fn render(prefix: &str, kind: ShellKind) -> String {
    let path = format!("{prefix}/bin:{prefix}/sbin");
    let manpath = format!("{prefix}/share/man");
    let infopath = format!("{prefix}/share/info");
    let pkgconfig = format!("{prefix}/lib/pkgconfig:{prefix}/share/pkgconfig");
    match kind {
        ShellKind::Sh => format!(
            "export FIELDSBREW_PREFIX=\"{prefix}\"\n\
             export PATH=\"{path}:$PATH\"\n\
             export MANPATH=\"${{MANPATH:-}}:{manpath}\"\n\
             export INFOPATH=\"${{INFOPATH:-}}:{infopath}\"\n\
             export PKG_CONFIG_PATH=\"${{PKG_CONFIG_PATH:-}}:{pkgconfig}\"\n\
             export CMAKE_PREFIX_PATH=\"${{CMAKE_PREFIX_PATH:-}}:{prefix}\"\n"
        ),
        ShellKind::Fish => format!(
            "set -gx FIELDSBREW_PREFIX \"{prefix}\"\n\
             set -gx PATH \"{prefix}/bin\" \"{prefix}/sbin\" $PATH\n\
             set -gx MANPATH \"{manpath}\" $MANPATH\n\
             set -gx INFOPATH \"{infopath}\" $INFOPATH\n\
             set -gx PKG_CONFIG_PATH \"{prefix}/lib/pkgconfig\" \"{prefix}/share/pkgconfig\" $PKG_CONFIG_PATH\n\
             set -gx CMAKE_PREFIX_PATH \"{prefix}\" $CMAKE_PREFIX_PATH\n"
        ),
        ShellKind::Csh => format!(
            "setenv FIELDSBREW_PREFIX \"{prefix}\";\n\
             setenv PATH \"{path}:$PATH\";\n\
             setenv MANPATH \"{manpath}:$MANPATH\";\n\
             setenv INFOPATH \"{infopath}:$INFOPATH\";\n\
             setenv PKG_CONFIG_PATH \"{pkgconfig}:$PKG_CONFIG_PATH\";\n\
             setenv CMAKE_PREFIX_PATH \"{prefix}:$CMAKE_PREFIX_PATH\";\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell() {
        assert_eq!(detect_shell(Some("/bin/zsh")).unwrap(), ShellKind::Sh);
        assert_eq!(detect_shell(Some("fish")).unwrap(), ShellKind::Fish);
        assert_eq!(detect_shell(Some("/bin/tcsh")).unwrap(), ShellKind::Csh);
        assert!(detect_shell(Some("powershell")).is_err());
    }

    #[test]
    fn test_render() {
        let sh = render("/opt/fieldsbrew", ShellKind::Sh);
        assert!(sh.contains("export PATH=\"/opt/fieldsbrew/bin:/opt/fieldsbrew/sbin:$PATH\""));
        assert!(sh.contains("export MANPATH=\"${MANPATH:-}:/opt/fieldsbrew/share/man\""));
        let fish = render("/opt/fieldsbrew", ShellKind::Fish);
        assert!(fish.contains("set -gx PATH \"/opt/fieldsbrew/bin\" \"/opt/fieldsbrew/sbin\" $PATH"));
        let csh = render("/opt/fieldsbrew", ShellKind::Csh);
        assert!(csh.contains("setenv PATH \"/opt/fieldsbrew/bin:/opt/fieldsbrew/sbin:$PATH\";"));
    }
}
