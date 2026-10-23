[English](README.md) | 简体中文

<div align="center">
  <img src="assets/icons/app.svg" width="88" alt="Pure Clash 图标" />
  <h1>Pure Clash</h1>
  <p>使用 Rust 和 Zed GPUI 构建的轻量原生 Mihomo 桌面客户端</p>
</div>

Pure Clash 通过简洁、快速的原生界面管理配置订阅、代理分组、连接和实时流量，并管理独立运行的 [Mihomo](https://github.com/MetaCubeX/mihomo) 内核，不在 Rust 中重新实现代理协议或规则引擎。

目前支持 Windows x64 和 Linux x64（Wayland / X11）；macOS 仅预留目录和资源边界，尚未实现完整支持。

## 功能

- **内核生命周期**：启动前使用同一版本内核的 `-t` 校验配置；Windows 使用 Job Object、Linux 使用 `PR_SET_PDEATHSIG` 守护子进程，主进程退出时回收内核；支持手动启停，通过托盘退出时完整清理。
- **配置与订阅**：提供仅含 DIRECT 的内置默认配置，支持 URL 订阅和原生文件选择器导入本地 YAML；所有来源均通过结构预检和内核 `-t` 校验后，再原子保存并激活。
- **离线 Geo 数据库**：安装包包含 `GeoSite.dat`、`GeoIP.dat` 和 `Country.mmdb`，可在不联网时从随包资源恢复，也可在设置页手动更新。
- **代理分组与测速**：按分组展示订阅节点，通过 controller 实时切换规则、全局和直连模式；支持单节点和整组测速，按延迟区间显示不同颜色。
- **连接与实时流量**：每秒通过 controller 获取连接快照，展示进程、目标、代理链、规则及每条连接的上传和下载流量；支持关闭单条或全部连接，概览页显示实时网速和活跃连接。
- **自定义入站监听**：支持在默认混合端口之外配置多个 `http`、`socks`、`mixed` 监听，每个监听可独立指定路由模式（跟随全局、规则、全局、直连），保存后通过 `-t` 校验并自动重载内核与系统代理。
- **系统代理**：Windows 写入当前用户的 Internet Settings，并通过 WinINet 通知系统；Linux 通过 `gsettings` 支持 GNOME/Cinnamon 会话，并通过 `kwriteconfig`/`kioslaverc` 自动兼容 KDE Plasma（5/6），支持按自定义监听独立分发 HTTP 与 SOCKS 端口。开启前原子保存用户原有设置，在关闭代理、停止内核或崩溃恢复时还原。
- **TUN 模式**：Windows 通过 UAC 提权启动内核，并使用随包提供的 wintun；Linux 参考 Clash Verge Rev 的服务模型，首次通过 `pkexec` 安装 root systemd 服务，之后通过按 UID 限制的 IPC 启停，无需重复输入密码；TUN 未生效时自动回退。
- **桌面集成**：托盘图标及多语言实时状态、单实例锁、关闭窗口后驻留托盘、深浅色主题、中英文界面。
- **低内存占用**：桌面客户端自身的内存占用通常低于 30 MB，不含 Mihomo 内核。

## 页面

提供五个主要页面和关于页：概览（统计卡片、当前出站、运行状态、最近连接）、代理（分组、节点选择、测速）、连接（实时列表）、配置（订阅管理）、设置（内核与系统集成、语言和主题）。标题栏显示与系统代理和 TUN 开关同步的状态徽标。

## 平台支持

| 能力 | Windows x64 | Linux x64 | macOS |
| --- | --- | --- | --- |
| 内核启停与守护 | ✅ | ✅ | 已预留 |
| 配置与代理控制 | ✅ | ✅ | 已预留 |
| 连接与实时流量 | ✅ | ✅ | 已预留 |
| 系统代理 | ✅ | ✅ GNOME/Cinnamon/KDE | 未实现 |
| TUN 模式 | ✅ UAC + wintun | ✅ polkit + root 服务 | 未实现 |
| 托盘 | ✅ | ✅ SNI（GNOME 需要 AppIndicator 扩展） | 未实现 |
| 单实例 | ✅ | ✅ | 未实现 |
| 安装包 | ✅ NSIS 当前用户安装 / 便携 ZIP | ✅ deb / rpm / AppImage | 未实现 |

Linux 使用 XDG 目录（`~/.config/pure-clash`、`~/.local/share/pure-clash`）；Wayland 使用包含圆角、阴影和缩放边缘的客户端窗口装饰，X11 回退为系统窗口装饰。

## 已测试环境

目前已在以下环境完成实机验证，欢迎反馈其他环境的使用情况：

- Windows 11 x64
- Windows 10 x64
- Fedora 44 x64（Wayland / GNOME）

## 快速开始

### 环境要求

- Windows 10/11 x64：Rust stable + MSVC 工具链。
- Linux x64：Rust stable，Wayland/X11 会话；TUN 需要 `pkexec` 和 polkit 认证代理。
- Windows 打包需要 PowerShell 7；仅构建 Windows 安装器时需要 NSIS 3.x，便携 ZIP 不需要。

### 构建与运行

```bash
cargo run
```

锁定版本的内核随源码提交：Windows 为 `kernel/<version>/pc-mihomo.exe`，Linux 为 `kernel/<version>/pc-mihomo`，克隆后即可启动真实内核。macOS 二进制未入库，需要按 `kernel/<version>/manifest.json` 中 `targets.macos-*` 的地址下载，并校验 SHA-256。

首次启动时，程序会在当前平台对应的目录中初始化配置和数据，包括仅含 DIRECT 的默认配置、随机生成的 controller secret，以及经过离线校验的随包 Geo 数据库副本。

### 打包与发布

```powershell
pwsh -NoLogo -NoProfile -File .\packaging\windows\build-installer.ps1
```

输出为 `dist\pure-clash-<version>-windows-x64-setup.exe`，默认安装到当前用户的 `%LOCALAPPDATA%\Programs\Pure Clash`，安装过程无需管理员权限。

仅构建 Windows 便携包时，执行以下命令（需要 PowerShell 7，无需 NSIS）：

```powershell
pwsh -NoLogo -NoProfile -File .\packaging\windows\build-installer.ps1 -PackageFormat Portable
```

输出为 `dist\pure-clash-<version>-windows-x64-portable.zip`。解压到可写目录后，运行 `pure-clash\pure-clash.exe`。配置、数据和日志会在可执行文件旁创建，迁移时移动整个文件夹即可保留。ZIP 不包含个人配置。使用 `-PackageFormat All` 可在一次构建中生成安装器和便携包。

推送 `v*` 标签后，GitHub Actions 会自动构建 Linux deb / rpm / AppImage，以及 Windows NSIS 安装器和便携 ZIP，并发布到 [Releases](https://github.com/assassin321/pure-clash/releases)。标签版本必须与 Cargo 包版本一致。deb/rpm 安装到 `/opt/pure-clash`，并创建 `/usr/bin/pure-clash` 符号链接；AppImage 可直接运行。所有发行包均包含内核、Geo 数据库、清单和许可证文件。

### 开发检查

```bash
cargo fmt --check
cargo check
cargo test
cargo build --release
```

GPUI 尚未达到 1.0；本项目固定使用 Zed `v1.17.2` 对应的提交 `c8e44cfa7bda9b2e22c8d6934d78969352e7f61a`。升级前请核对目标版本的官方示例和变更记录。

## 内核供应链

随包 Mihomo 内核使用固定版本：`kernel/<version>/manifest.json` 记录版本、许可证和源码地址，`targets` 按构建目标记录下载地址、大小和 SHA-256。构建脚本和安装器会检查文件是否存在并验证哈希，程序不会自动跟随 GitHub latest。内核重命名为 `pc-mihomo`，避免与其他代理客户端的进程重名。

Geo 数据快照通过 `geodata/manifest.json` 独立锁定到 MetaCubeX 官方规则数据仓库的某个提交。构建和打包时校验全部三个数据文件；设置页以支持回滚的完整快照更新它们，订阅校验期间不会隐式下载。

## 安全模型

- controller 仅监听 `127.0.0.1`，每次安装生成高强度随机 secret，且不写入日志。
- 日志与诊断信息对订阅 URL、凭据和 controller secret 进行脱敏；原始校验错误仅向当前用户展示。
- Linux TUN 服务以 root 运行固定版本的内核副本，将配置原子写入受保护目录并重新校验；拒绝未开启 TUN、监听范围超出回环地址或资源路径越界的配置包。
- 修改系统代理前先原子保存原有状态，异常退出后在下次启动时恢复。

完整设计见[技术设计文档](docs/pure-clash-architecture.md)（英文）。

## 后续计划

- 规则页及日志、内存监控（controller `/rules`、`/logs`、`/memory`）。
- Linux 凭据存储，以及除 GNOME/Cinnamon/KDE 外的更多桌面环境系统代理支持。
- 完整 macOS 支持，包括内核守护、窗口行为和 TUN 权限边界。
- 代码签名和更新渠道。

## 参与贡献

本项目包含大量 AI 辅助编程（vibe coding）的成果，并通过 [AGENTS.md](AGENTS.md) 维护 AI 编程代理的工作约定。欢迎通过 vibe coding 提交 Issue 和 PR，请遵循[贡献指南](CONTRIBUTING.md)中的代码约定和验证步骤。上述两份文档默认使用英文。

## 致谢

- [Pure Clash](https://github.com/prime-zt/pure-clash/releases) 使用 Rust 和 Zed GPUI 构建的轻量原生 Mihomo 桌面客户端
- [Zed](https://github.com/zed-industries/zed) 与 GPUI：提供界面框架，客户端窗口装饰参考 Zed 官方示例。
- [Mihomo](https://github.com/MetaCubeX/mihomo)（MetaCubeX）：负责全部网络转发的代理内核。
- [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev)：Linux TUN 服务模型和配置模板的参考。
- [rust-i18n](https://github.com/longbridge/rust-i18n)、[ksni](https://github.com/frostwind/ksni)、[tray-icon](https://github.com/tauri-apps/tray-icon) 及其他优秀的开源库。

## 许可证

本项目采用 [GPL-3.0](LICENSE) 许可证。

随包 Mihomo 内核是未经修改的上游 GPL-3.0 二进制。完整许可证 `LICENSE` 和第三方声明 `NOTICE.md` 位于 `kernel/<version>/`，`manifest.json` 记录对应的源码地址，安装器会将这些文件与内核一同分发。随包 MetaCubeX 规则数据同样采用 GPL-3.0，其固定版本来源、许可证和声明位于 `geodata/`。

Pure Clash 与 MetaCubeX 无隶属关系，也不代表上游项目的官方认可。项目名称避免使用上游保留的 `mihomo` 字样。
