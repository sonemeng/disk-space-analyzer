//! Windows 子进程创建辅助：隐藏控制台黑窗。
//! GUI 父进程（无控制台）在 Windows 上派生 cmd 类子进程会新建控制台窗口，
//! 需要显式传入 CREATE_NO_WINDOW 标志。
use std::process::Command;

#[cfg(windows)]
pub fn hidden(mut cmd: Command) -> Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

#[cfg(not(windows))]
pub fn hidden(cmd: Command) -> Command {
    cmd
}