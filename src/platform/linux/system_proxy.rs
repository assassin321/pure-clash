//! Linux 桌面系统代理：通过 GNOME/Cinnamon 的 GSettings schema 或 KDE Plasma 的
//! kioslaverc 配置管理当前桌面会话。
//!
//! 写入前保存所有修改键的原始文本，恢复时逐项原样写回，保留用户原来的代理模式及各协议端点。

use std::{env, net::SocketAddr, process::Command};

use anyhow::{Context, Result, anyhow, bail};

use super::super::{LinuxProxySetting, LinuxSystemProxySnapshot, SystemProxySnapshot};

const SCHEMA_CANDIDATES: [&str; 2] = ["org.gnome.system.proxy", "org.cinnamon.system.proxy"];
const PROXY_KEYS: [(&str, &str); 11] = [
    ("", "mode"),
    ("", "use-same-proxy"),
    ("http", "host"),
    ("http", "port"),
    ("http", "use-authentication"),
    ("https", "host"),
    ("https", "port"),
    ("ftp", "host"),
    ("ftp", "port"),
    ("socks", "host"),
    ("socks", "port"),
];

const KDE_PROXY_GROUP: &str = "Proxy Settings";
const KDE_PROXY_KEYS: [&str; 6] = [
    "ProxyType",
    "httpProxy",
    "httpsProxy",
    "ftpProxy",
    "socksProxy",
    "NoProxyFor",
];

enum DesktopKind {
    GSettings(String),
    Kde(KdeTool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KdeTool {
    Kde6,
    Kde5,
}

impl KdeTool {
    fn write_cmd(self) -> &'static str {
        match self {
            Self::Kde6 => "kwriteconfig6",
            Self::Kde5 => "kwriteconfig5",
        }
    }

    fn read_cmd(self) -> &'static str {
        match self {
            Self::Kde6 => "kreadconfig6",
            Self::Kde5 => "kreadconfig5",
        }
    }
}

/// 捕获当前桌面会话的完整代理设置，供关闭和异常恢复时原样还原。
pub(crate) fn capture_system_proxy() -> Result<SystemProxySnapshot> {
    match detect_desktop_kind()? {
        DesktopKind::GSettings(prefix) => {
            let settings = PROXY_KEYS
                .iter()
                .map(|(suffix, key)| {
                    let schema = schema_name(&prefix, suffix);
                    Ok(LinuxProxySetting {
                        value: gsettings_get(&schema, key)?,
                        schema,
                        key: (*key).to_owned(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(SystemProxySnapshot {
                managed: true,
                prev_enabled: false,
                prev_server: String::new(),
                linux: Some(LinuxSystemProxySnapshot {
                    backend: "gsettings".to_string(),
                    schema_prefix: prefix,
                    settings,
                }),
            })
        }
        DesktopKind::Kde(tool) => {
            let settings = KDE_PROXY_KEYS
                .iter()
                .map(|key| {
                    let value = kde_read_config(tool, KDE_PROXY_GROUP, key).unwrap_or_default();
                    LinuxProxySetting {
                        schema: KDE_PROXY_GROUP.to_string(),
                        key: (*key).to_string(),
                        value,
                    }
                })
                .collect();

            Ok(SystemProxySnapshot {
                managed: true,
                prev_enabled: false,
                prev_server: String::new(),
                linux: Some(LinuxSystemProxySnapshot {
                    backend: "kde".to_string(),
                    schema_prefix: String::new(),
                    settings,
                }),
            })
        }
    }
}

/// 将系统代理指向指定的监听端点（支持独立 http/socks 端点，未独立指定时回退到统一 server）。
pub(crate) fn set_system_proxy_endpoints(
    http_server: &str,
    socks_server: Option<&str>,
) -> Result<()> {
    match detect_desktop_kind()? {
        DesktopKind::GSettings(prefix) => {
            let (http_host, http_port) = parse_proxy_server(http_server)?;
            let http_host_val = gvariant_string(&http_host);
            let http_port_val = http_port.to_string();

            let (socks_host_val, socks_port_val) = if let Some(socks_server) = socks_server {
                let (socks_host, socks_port) = parse_proxy_server(socks_server)?;
                (gvariant_string(&socks_host), socks_port.to_string())
            } else {
                (http_host_val.clone(), http_port_val.clone())
            };

            for protocol in ["http", "https", "ftp"] {
                let schema = schema_name(&prefix, protocol);
                gsettings_set(&schema, "host", &http_host_val)?;
                gsettings_set(&schema, "port", &http_port_val)?;
            }
            let socks_schema = schema_name(&prefix, "socks");
            gsettings_set(&socks_schema, "host", &socks_host_val)?;
            gsettings_set(&socks_schema, "port", &socks_port_val)?;

            gsettings_set(&schema_name(&prefix, "http"), "use-authentication", "false")?;
            gsettings_set(&prefix, "use-same-proxy", "false")?;
            gsettings_set(&prefix, "mode", "'manual'")
        }
        DesktopKind::Kde(tool) => {
            let (http_host, http_port) = parse_proxy_server(http_server)?;
            let (socks_host, socks_port) = if let Some(socks) = socks_server {
                parse_proxy_server(socks)?
            } else {
                (http_host.clone(), http_port)
            };

            // KDE 格式: http://host port 或 http://host:port (kioslave 兼容 http://host port)
            let http_uri = format!("http://{http_host} {http_port}");
            let socks_uri = format!("socks://{socks_host} {socks_port}");

            kde_write_config(tool, KDE_PROXY_GROUP, "ProxyType", "1")?;
            kde_write_config(tool, KDE_PROXY_GROUP, "httpProxy", &http_uri)?;
            kde_write_config(tool, KDE_PROXY_GROUP, "httpsProxy", &http_uri)?;
            kde_write_config(tool, KDE_PROXY_GROUP, "ftpProxy", &http_uri)?;
            kde_write_config(tool, KDE_PROXY_GROUP, "socksProxy", &socks_uri)?;

            kde_notify_kio_reload();
            Ok(())
        }
    }
}

/// 兼容旧接口：将全部协议指向同一个 server。
#[allow(dead_code)]
pub(crate) fn set_system_proxy(server: &str) -> Result<()> {
    set_system_proxy_endpoints(server, None)
}

/// 按状态文件中的原始快照恢复全部设置。
pub(crate) fn restore_system_proxy(snapshot: &SystemProxySnapshot) -> Result<()> {
    let linux = snapshot
        .linux
        .as_ref()
        .ok_or_else(|| anyhow!("系统代理状态文件缺少 Linux 快照"))?;

    if linux.backend == "kde" {
        let tool = detect_kde_tool().unwrap_or(KdeTool::Kde5);
        for setting in &linux.settings {
            if setting.value.is_empty() {
                let _ = kde_delete_config(tool, &setting.schema, &setting.key);
            } else {
                let _ = kde_write_config(tool, &setting.schema, &setting.key, &setting.value);
            }
        }
        kde_notify_kio_reload();
        return Ok(());
    }

    if !SCHEMA_CANDIDATES.contains(&linux.schema_prefix.as_str()) {
        bail!("系统代理状态文件包含未知的 Linux schema");
    }
    validate_snapshot(linux)?;

    let mode = linux
        .settings
        .iter()
        .find(|setting| setting.schema == linux.schema_prefix && setting.key == "mode")
        .expect("完整快照必须包含 mode")
        .value
        .as_str();
    for setting in &linux.settings {
        if setting.schema != linux.schema_prefix || setting.key != "mode" {
            gsettings_set(&setting.schema, &setting.key, &setting.value)?;
        }
    }
    gsettings_set(&linux.schema_prefix, "mode", mode)
}

fn detect_desktop_kind() -> Result<DesktopKind> {
    let desktop = [
        env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        env::var("XDG_SESSION_DESKTOP").unwrap_or_default(),
        env::var("DESKTOP_SESSION").unwrap_or_default(),
    ]
    .join(":")
    .to_ascii_lowercase();

    if desktop.contains("kde") || desktop.contains("plasma") {
        let tool = detect_kde_tool().ok_or_else(|| {
            anyhow!("检测到 KDE 桌面，但未找到 kwriteconfig6 或 kwriteconfig5 工具")
        })?;
        return Ok(DesktopKind::Kde(tool));
    }

    let schema = detect_schema(&desktop)?;
    Ok(DesktopKind::GSettings(schema))
}

fn detect_kde_tool() -> Option<KdeTool> {
    if which_bin("kwriteconfig6") && which_bin("kreadconfig6") {
        Some(KdeTool::Kde6)
    } else if which_bin("kwriteconfig5") && which_bin("kreadconfig5") {
        Some(KdeTool::Kde5)
    } else {
        None
    }
}

fn which_bin(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn kde_read_config(tool: KdeTool, group: &str, key: &str) -> Result<String> {
    let output = Command::new(tool.read_cmd())
        .args(["--file", "kioslaverc", "--group", group, "--key", key])
        .output()
        .with_context(|| format!("无法执行 {} 读取 kioslaverc", tool.read_cmd()))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn kde_write_config(tool: KdeTool, group: &str, key: &str, value: &str) -> Result<()> {
    let output = Command::new(tool.write_cmd())
        .args([
            "--file",
            "kioslaverc",
            "--group",
            group,
            "--key",
            key,
            value,
        ])
        .output()
        .with_context(|| format!("无法执行 {} 写入 kioslaverc", tool.write_cmd()))?;
    if !output.status.success() {
        bail!(
            "写入 KDE 代理配置失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn kde_delete_config(tool: KdeTool, group: &str, key: &str) -> Result<()> {
    let _ = Command::new(tool.write_cmd())
        .args([
            "--file",
            "kioslaverc",
            "--group",
            group,
            "--key",
            key,
            "--delete",
        ])
        .output();
    Ok(())
}

fn kde_notify_kio_reload() {
    // 优先通过 D-Bus 通知 kded 重载 proxyscout / reconfigure
    for service in ["org.kde.kded6", "org.kde.kded5"] {
        let _ = Command::new("qdbus")
            .args([
                service,
                "/modules/proxyscout",
                "org.kde.KPAC.ProxyScout.reset",
            ])
            .output();
        let _ = Command::new("qdbus")
            .args([service, "/kded", "reconfigure"])
            .output();
    }
}

fn detect_schema(desktop: &str) -> Result<String> {
    let candidates = schema_candidates_for_desktop(desktop)?;
    let output = Command::new("gsettings")
        .arg("list-schemas")
        .output()
        .context("无法执行 gsettings；当前桌面不支持 GNOME/Cinnamon 系统代理")?;
    if !output.status.success() {
        bail!("gsettings 无法列出当前桌面 schema");
    }
    let schemas = String::from_utf8_lossy(&output.stdout);
    candidates
        .into_iter()
        .find(|candidate| schemas.lines().any(|schema| schema == *candidate))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("当前桌面没有可用的 GNOME/Cinnamon 系统代理 schema"))
}

fn schema_candidates_for_desktop(desktop: &str) -> Result<[&'static str; 2]> {
    if desktop.contains("cinnamon") {
        Ok([SCHEMA_CANDIDATES[1], SCHEMA_CANDIDATES[0]])
    } else if ["gnome", "ubuntu", "unity", "budgie"]
        .iter()
        .any(|name| desktop.contains(name))
    {
        Ok(SCHEMA_CANDIDATES)
    } else {
        bail!("当前 Linux 桌面暂不支持系统代理；目前支持 GNOME/Cinnamon 与 KDE Plasma 会话")
    }
}

fn schema_name(prefix: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix}.{suffix}")
    }
}

fn gsettings_get(schema: &str, key: &str) -> Result<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .with_context(|| format!("无法读取系统代理设置 {schema}.{key}"))?;
    if !output.status.success() {
        bail!(
            "读取系统代理设置 {schema}.{key} 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value = String::from_utf8(output.stdout)
        .context("gsettings 返回了非 UTF-8 数据")?
        .trim()
        .to_owned();
    if value.is_empty() {
        bail!("系统代理设置 {schema}.{key} 返回空值");
    }
    Ok(value)
}

fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<()> {
    let output = Command::new("gsettings")
        .args(["set", schema, key, value])
        .output()
        .with_context(|| format!("无法写入系统代理设置 {schema}.{key}"))?;
    if !output.status.success() {
        bail!(
            "写入系统代理设置 {schema}.{key} 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn parse_proxy_server(server: &str) -> Result<(String, u16)> {
    let addr: SocketAddr = server
        .parse()
        .with_context(|| format!("系统代理地址无效：{server}"))?;
    if !addr.ip().is_loopback() {
        bail!("系统代理只允许指向本机回环地址");
    }
    Ok((addr.ip().to_string(), addr.port()))
}

fn gvariant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn validate_snapshot_setting(prefix: &str, setting: &LinuxProxySetting) -> Result<()> {
    let allowed = PROXY_KEYS
        .iter()
        .any(|(suffix, key)| setting.schema == schema_name(prefix, suffix) && setting.key == *key);
    if !allowed || setting.value.trim().is_empty() || setting.value.contains(['\n', '\r']) {
        bail!("Linux 系统代理快照包含无效设置");
    }
    Ok(())
}

fn validate_snapshot(snapshot: &LinuxSystemProxySnapshot) -> Result<()> {
    if snapshot.settings.len() != PROXY_KEYS.len() {
        bail!("Linux 系统代理快照不完整");
    }
    for setting in &snapshot.settings {
        validate_snapshot_setting(&snapshot.schema_prefix, setting)?;
    }
    for (suffix, key) in PROXY_KEYS {
        let schema = schema_name(&snapshot.schema_prefix, suffix);
        if snapshot
            .settings
            .iter()
            .filter(|setting| setting.schema == schema && setting.key == key)
            .count()
            != 1
        {
            bail!("Linux 系统代理快照不完整或包含重复设置");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_loopback_proxy_servers() {
        assert_eq!(
            parse_proxy_server("127.0.0.1:7890").unwrap(),
            ("127.0.0.1".to_owned(), 7890)
        );
        assert_eq!(
            parse_proxy_server("[::1]:7890").unwrap(),
            ("::1".to_owned(), 7890)
        );
        assert!(parse_proxy_server("192.0.2.1:7890").is_err());
        assert!(parse_proxy_server("localhost:7890").is_err());
    }

    #[test]
    fn snapshot_keys_are_restricted_to_managed_schemas() {
        let valid = LinuxProxySetting {
            schema: "org.gnome.system.proxy.http".into(),
            key: "host".into(),
            value: "'example.test'".into(),
        };
        assert!(validate_snapshot_setting("org.gnome.system.proxy", &valid).is_ok());

        let mut invalid = valid.clone();
        invalid.schema = "org.gnome.desktop.interface".into();
        assert!(validate_snapshot_setting("org.gnome.system.proxy", &invalid).is_err());
    }

    #[test]
    fn rejects_incomplete_or_duplicate_snapshots_before_restore() {
        let snapshot = LinuxSystemProxySnapshot {
            backend: "gsettings".into(),
            schema_prefix: "org.gnome.system.proxy".into(),
            settings: PROXY_KEYS
                .iter()
                .map(|(suffix, key)| LinuxProxySetting {
                    schema: schema_name("org.gnome.system.proxy", suffix),
                    key: (*key).into(),
                    value: "false".into(),
                })
                .collect(),
        };
        assert!(validate_snapshot(&snapshot).is_ok());

        let mut incomplete = snapshot.clone();
        incomplete.settings.pop();
        assert!(validate_snapshot(&incomplete).is_err());

        let mut duplicate = snapshot;
        duplicate.settings[1] = duplicate.settings[0].clone();
        assert!(validate_snapshot(&duplicate).is_err());
    }

    #[test]
    fn chooses_schema_for_supported_desktops() {
        assert_eq!(
            schema_candidates_for_desktop("ubuntu:gnome").unwrap()[0],
            "org.gnome.system.proxy"
        );
        assert_eq!(
            schema_candidates_for_desktop("x-cinnamon").unwrap()[0],
            "org.cinnamon.system.proxy"
        );
        assert!(schema_candidates_for_desktop("xfce").is_err());
    }

    #[test]
    fn kde_tool_detection_and_command_names() {
        let tool = KdeTool::Kde5;
        assert_eq!(tool.write_cmd(), "kwriteconfig5");
        assert_eq!(tool.read_cmd(), "kreadconfig5");
        let tool6 = KdeTool::Kde6;
        assert_eq!(tool6.write_cmd(), "kwriteconfig6");
        assert_eq!(tool6.read_cmd(), "kreadconfig6");
    }

    /// 会短暂修改当前桌面会话的系统代理，手动验证时显式运行：
    /// `cargo test linux_system_proxy_roundtrip -- --ignored`
    #[test]
    #[ignore = "会短暂修改当前桌面会话的系统代理"]
    fn linux_system_proxy_roundtrip() {
        let snapshot = capture_system_proxy().expect("应捕获系统代理");
        set_system_proxy("127.0.0.1:7899").expect("应设置系统代理");
        restore_system_proxy(&snapshot).expect("应恢复系统代理");
        let restored = capture_system_proxy().expect("应再次捕获系统代理");
        assert_eq!(
            restored.linux.unwrap().settings,
            snapshot.linux.unwrap().settings
        );
    }
}
