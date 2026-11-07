use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub fn available() -> bool {
    Path::new(SANDBOX_EXEC).exists()
}

/// 执行一条构建命令。
///
/// 默认尝试放入“仅禁网”沙盒（允许文件读写、禁止网络），构建阶段不需要联网；
/// sandbox-exec 不可用或用户指定 --no-sandbox 时直接执行。
pub fn run(cmd: &mut Command, cwd: &Path, no_sandbox: bool, verbose: bool) -> Result<()> {
    cmd.current_dir(cwd);
    if verbose {
        eprintln!("$ cd {} && {:?}", cwd.display(), cmd);
    }

    if !no_sandbox && available() {
        let profile = std::env::temp_dir().join(format!("fb-sandbox-{}.sb", std::process::id()));
        std::fs::write(&profile, "(version 1)\n(allow default)\n(deny network*)\n")?;
        let mut sb = Command::new(SANDBOX_EXEC);
        sb.arg("-f").arg(&profile);
        sb.arg(cmd.get_program());
        sb.args(cmd.get_args());
        for (k, v) in cmd.get_envs() {
            if let Some(v) = v {
                sb.env(k, v);
            }
        }
        sb.current_dir(cwd);
        let res = sb.status();
        let _ = std::fs::remove_file(&profile);
        match res {
            Ok(st) if st.success() => return Ok(()),
            Ok(st) => {
                return Err(anyhow!(
                    "沙盒内命令失败（退出码 {:?}）；排查时可加 --no-sandbox 重试",
                    st.code()
                ))
            }
            Err(e) => {
                eprintln!("警告: sandbox-exec 启动失败（{e}），回退为无沙盒执行");
            }
        }
    }

    let st = cmd.status().with_context(|| "执行命令失败")?;
    if !st.success() {
        return Err(anyhow!("命令失败（退出码 {:?}）", st.code()));
    }
    Ok(())
}
