//! 应用启动入口的模式解析。

use std::ffi::OsString;
use std::path::PathBuf;

const AUTOSTART_ARG: &str = "--autostart";
const PORTABLE_ARG: &str = "--portable";  // ✅ 新增命令行参数

/// 区分用户主动打开与桌面登录自启，决定是否创建初始主窗口及通知已有实例。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartupMode {
    /// 用户主动启动：显示主窗口，若已有实例则请求其恢复窗口。
    Interactive,
    /// 系统登录自启：只启动常驻业务与托盘，若已有实例则静默退出。
    Autostart,
    /// ✅ 便携模式：配置和数据存储在可执行文件同级目录的 Data/ 下。
    Portable(PathBuf),
}

impl StartupMode {
    pub(crate) fn from_env() -> Self {
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            Self::from_args(std::env::args_os())
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Self::Interactive
        }
    }

    fn from_args(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut has_autostart = false;
        let mut has_portable = false;
        
        for arg in args.into_iter().skip(1) {
            match arg.to_str() {
                Some(PORTABLE_ARG) => has_portable = true,
                Some(AUTOSTART_ARG) => has_autostart = true,
                _ => {}
            }
        }
        
        // ✅ 便携模式优先级最高
        if has_portable {
            // 检测环境变量优先
            if let Ok(dir) = std::env::var("PURE_CLASH_PORTABLE_DIR") {
                return Self::Portable(PathBuf::from(dir));
            }
            
            // 否则使用可执行文件同级目录的 Data/
            if let Ok(exe) = std::env::current_exe() {
                if let Some(parent) = exe.parent() {
                    return Self::Portable(parent.join("Data"));
                }
            }
            
            // 回退到当前目录
            return Self::Portable(PathBuf::from("Data"));
        }
        
        // ✅ 检测 portable.flag 文件（便携版 zip / U 盘部署）
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                if parent.join("portable.flag").exists() {
                    if let Ok(dir) = std::env::var("PURE_CLASH_PORTABLE_DIR") {
                        return Self::Portable(PathBuf::from(dir));
                    }
                    return Self::Portable(parent.join("Data"));
                }
            }
        }
        
        // ✅ 检测环境变量（AppImage 的 AppRun 设置）
        if let Ok(dir) = std::env::var("PURE_CLASH_PORTABLE_DIR") {
            return Self::Portable(PathBuf::from(dir));
        }
        
        if has_autostart {
            Self::Autostart
        } else {
            Self::Interactive
        }
    }

    /// 只有用户主动启动才立即创建窗口；后台自启等待托盘或第二实例唤起。
    pub(crate) fn show_initial_window(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// 后台自启不能干扰已经运行的实例，尤其不能在登录时意外弹出窗口。
    pub(crate) fn notify_existing_instance(&self) -> bool {
        matches!(self, Self::Interactive | Self::Portable(_))
    }

    pub(crate) fn is_autostart(&self) -> bool {
        self == Self::Autostart
    }
    
    /// 获取便携模式的数据目录
    pub(crate) fn portable_data_dir(&self) -> Option<&PathBuf> {
        if let Self::Portable(dir) = self {
            Some(dir)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autostart_flag_without_affecting_normal_launches() {
        assert_eq!(
            StartupMode::from_args(["pure-clash".into(), AUTOSTART_ARG.into()]),
            StartupMode::Autostart
        );
        assert_eq!(
            StartupMode::from_args(["pure-clash".into()]),
            StartupMode::Interactive
        );
    }
    
    #[test]
    fn detects_portable_mode() {
        // 命令行参数
        let mode = StartupMode::from_args(["pure-clash".into(), PORTABLE_ARG.into()]);
        assert!(matches!(mode, StartupMode::Portable(_)));
        
        // autostart + portable 组合
        let mode = StartupMode::from_args([
            "pure-clash".into(),
            AUTOSTART_ARG.into(),
            PORTABLE_ARG.into(),
        ]);
        assert!(matches!(mode, StartupMode::Portable(_)));
    }
}