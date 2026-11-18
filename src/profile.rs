//! 配置文件（订阅）管理：下载、结构校验、内核校验与落盘。
//!
//! 管线约定：任何新内容（下载或导入）必须依次通过
//! 1. YAML 结构预检与本地基线合并（[`merge_runtime`]）；
//! 2. 随包内核 `-t` 终审（[`validate_kernel_config`]）；
//!
//! 全部通过后才写入 `profiles/<id>.yaml`；激活时再把合并产物写入
//! `runtime.yaml` 并按需重启内核。

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::{
    config::ProfileMeta,
    mihomo::{
        config::{ensure_baseline, merge_runtime, validate_kernel_config, write_runtime},
        controller::{Controller, MAX_DOWNLOAD_BYTES},
    },
    platform::AppPaths,
};

/// 单个配置文件的磁盘路径。
pub(crate) fn profile_yaml_path(paths: &AppPaths, id: &str) -> PathBuf {
    paths.profiles_dir.join(format!("{id}.yaml"))
}

/// 读取已保存的配置内容。
pub(crate) fn read_profile(paths: &AppPaths, id: &str) -> Result<String> {
    let path = profile_yaml_path(paths, id);
    fs::read_to_string(&path).with_context(|| format!("无法读取配置：{}", path.display()))
}

/// 删除配置文件；文件不存在时视为成功。
pub(crate) fn delete_profile_file(paths: &AppPaths, id: &str) -> Result<()> {
    let path = profile_yaml_path(paths, id);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("无法删除配置：{}", path.display()))?;
    }
    Ok(())
}

/// 当前 UNIX 时间戳（秒）。
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// 校验并保存订阅/导入内容，返回合并产物供后续激活使用。
///
/// 内容只有通过结构预检与内核 `-t` 校验才会落盘，失败时不产生任何残留。
pub(crate) fn validate_and_store(
    paths: &AppPaths,
    version: &str,
    id: &str,
    content: &str,
) -> Result<String> {
    let baseline = ensure_baseline(paths)?;
    let runtime = merge_runtime(content, &baseline)?;
    validate_kernel_config(paths, version, &runtime)?;
    let profile_file = profile_yaml_path(paths, id);
    crate::platform::file::atomic_write(&profile_file, content.as_bytes())
        .with_context(|| format!("无法写入配置：{}", profile_file.display()))?;
    Ok(runtime)
}

/// 本次下载的出口快照；只在应用代理开启时使用，不记录含凭据的代理地址。
pub(crate) struct SubscriptionProxy {
    pub(crate) preferred: String,
    /// TUN 开启时必须走内核 DIRECT 专用入口，不能靠取消 HTTP 代理假装绕过 TUN。
    pub(crate) direct: SubscriptionDirect,
}

pub(crate) enum SubscriptionDirect {
    System,
    Proxy(String),
    Unavailable,
}

/// 先代理后直连；只有两次下载均失败才返回错误。后续 YAML 校验失败不触发网络重试。
pub(crate) fn download_subscription(
    url: &str,
    proxy: Option<&SubscriptionProxy>,
) -> Result<String> {
    let download = |route: Option<&str>| {
        Controller::download_subscription(url, route)
            .map_err(|error| anyhow::anyhow!("{}", crate::logging::redact(&format!("{error:#}"))))
    };
    let Some(proxy) = proxy else {
        return download(None);
    };
    let proxy_error = match download(Some(&proxy.preferred)) {
        Ok(content) => return Ok(content),
        Err(error) => error,
    };
    log_info!("profile", "订阅代理下载失败，尝试直连");
    let result = match &proxy.direct {
        SubscriptionDirect::System => download(None),
        SubscriptionDirect::Proxy(route) => download(Some(route)),
        SubscriptionDirect::Unavailable => {
            Err(anyhow::anyhow!("订阅直连入口未就绪，请重启内核后重试"))
        }
    };
    result.map_err(|direct_error| {
        anyhow::anyhow!("订阅下载失败；代理：{proxy_error:#}；直连：{direct_error:#}")
    })
}

/// 读取用户选择的本地 Mihomo YAML；沿用订阅 10MB 上限并要求 UTF-8 文本。
pub(crate) fn read_local_config(path: &Path) -> Result<String> {
    let metadata =
        fs::metadata(path).with_context(|| format!("无法读取本地配置：{}", path.display()))?;
    if !metadata.is_file() {
        bail!("选择的路径不是文件");
    }
    if metadata.len() > MAX_DOWNLOAD_BYTES {
        bail!("本地配置超过 10MB 体积上限");
    }
    let bytes = fs::read(path).with_context(|| format!("无法读取本地配置：{}", path.display()))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        bail!("本地配置超过 10MB 体积上限");
    }
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("本地配置不是有效的 UTF-8 文本"))
}

/// 从本地文件名派生默认显示名；路径本身不会写入配置元数据。
pub(crate) fn default_name_from_path(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_string_lossy().trim().to_owned();
    (!name.is_empty() && !name.starts_with('.')).then_some(name)
}

/// 从订阅 URL 派生默认显示名：取主机名部分。
pub(crate) fn default_name_from_url(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_owned())
    }
}

/// 校验 URL 基本形状；只接受 http/https。
pub(crate) fn validate_subscription_url(url: &str) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        bail!("订阅地址必须以 http:// 或 https:// 开头");
    }
    if url.len() <= 8 {
        bail!("订阅地址不完整");
    }
    Ok(())
}

/// 修改订阅元数据；只修改内存，由调用方持久化后决定是否下载。
///
/// `meta` 为现有远程订阅（本地配置会报错）；`url` 为 HTTP(S) 地址，首尾空白会移除；
/// `interval` 为分钟数字，空值或 0 表示关闭；`now` 为 UNIX 秒。
/// 返回链接是否改变，供调用方仅在变更链接时立即更新；任何校验失败均保持原值。
pub(crate) fn edit_subscription_metadata(
    meta: &mut ProfileMeta,
    url: &str,
    interval: &str,
    now: u64,
) -> Result<bool> {
    let Some(previous_url) = meta.url.as_deref() else {
        bail!("本地配置没有订阅链接");
    };
    let url = url.trim();
    validate_subscription_url(url)?;
    let minutes = parse_update_interval(interval)?;
    let url_changed = previous_url != url;
    meta.url = Some(url.to_owned());
    meta.update_interval_minutes = minutes;
    // 从未更新的订阅首次启用定时更新时，从本次保存起算，避免意外立即到期。
    if minutes > 0 && meta.updated_at == 0 && meta.last_auto_attempt_at == 0 {
        meta.last_auto_attempt_at = now;
    }
    Ok(url_changed)
}

/// 把 `source_id` 对应配置移动到 `target_id` 原位置，其他配置保持相对顺序。
///
/// `profiles` 为可排序的用户配置列表（不含内置默认配置）；ID 缺失、相同或列表为空
/// 时返回 false 且不修改列表，成功返回 true。向上拖放置于目标之前，向下则置于之后。
pub(crate) fn reorder_profiles(
    profiles: &mut [ProfileMeta],
    source_id: &str,
    target_id: &str,
) -> bool {
    let Some(source) = profiles.iter().position(|meta| meta.id == source_id) else {
        return false;
    };
    let Some(target) = profiles.iter().position(|meta| meta.id == target_id) else {
        return false;
    };
    match source.cmp(&target) {
        std::cmp::Ordering::Less => profiles[source..=target].rotate_left(1),
        std::cmp::Ordering::Greater => profiles[target..=source].rotate_right(1),
        std::cmp::Ordering::Equal => return false,
    }
    true
}

/// 生成新的随机配置标识。
pub(crate) fn new_profile_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// 把当前激活态同步到 runtime.yaml；`active_id` 为 None 时回退到内置默认配置。
pub(crate) fn sync_runtime_file(
    paths: &AppPaths,
    baseline: Option<&crate::mihomo::config::LocalBaseline>,
    version: &str,
    active_id: Option<&str>,
) -> Result<()> {
    let runtime = prepare_runtime(paths, baseline, version, active_id)?;
    write_runtime(paths, &runtime)
}

/// 在内存中生成当前激活配置的 runtime，并用目标版本内核完成终审。
///
/// 成功返回完整 YAML，但不修改磁盘；调用方可在其他状态准备就绪后原子提交。
pub(crate) fn prepare_runtime(
    paths: &AppPaths,
    baseline: Option<&crate::mihomo::config::LocalBaseline>,
    version: &str,
    active_id: Option<&str>,
) -> Result<String> {
    let Some(baseline) = baseline else {
        bail!("本地基线不可用，无法生成运行时配置");
    };
    let runtime = match active_id {
        Some(id) => {
            let content = read_profile(paths, id)?;
            merge_runtime(&content, baseline)?
        }
        None => {
            // 无激活配置时回退到内置默认配置，保证内核始终有可用配置。
            let default_path = &paths.default_mihomo_config_file;
            let content = fs::read_to_string(&default_path)
                .with_context(|| format!("无法读取默认配置：{}", default_path.display()))?;
            merge_runtime(&content, baseline)?
        }
    };
    validate_kernel_config(paths, version, &runtime)?;
    Ok(runtime)
}

/// 只在内核停止后准备专用端口；运行中的配置合并和热重载不能换端口。
pub(crate) fn prepare_subscription_direct_startup(
    paths: &AppPaths,
    baseline: &mut crate::mihomo::config::LocalBaseline,
    version: &str,
    active_id: Option<&str>,
) -> Result<Option<std::net::TcpListener>> {
    use crate::mihomo::config::save_baseline;
    let mut desired = baseline.clone();
    let reservation = desired.reserve_subscription_direct_port()?;
    if desired.subscription_direct_port != baseline.subscription_direct_port {
        let runtime = prepare_runtime(paths, Some(&desired), version, active_id)?;
        save_baseline(paths, &desired)?;
        if let Err(error) = write_runtime(paths, &runtime) {
            return match save_baseline(paths, baseline) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "{error:#}；恢复原本地基线也失败：{rollback_error:#}"
                )),
            };
        }
        log_info!("profile", "订阅直连端口被占用，已改用空闲端口");
        *baseline = desired;
    }
    Ok(reservation)
}

/// 创建订阅类型配置的元数据。
pub(crate) fn subscription_meta(name: String, url: String) -> ProfileMeta {
    let now = now_secs();
    ProfileMeta {
        id: new_profile_id(),
        name,
        url: Some(url),
        added_at: now,
        updated_at: now,
        update_interval_minutes: 0,
        last_auto_attempt_at: 0,
    }
}

/// 创建本地导入类型配置的元数据；`url` 为空表示不提供远程更新操作。
pub(crate) fn local_meta(name: String) -> ProfileMeta {
    let now = now_secs();
    ProfileMeta {
        id: new_profile_id(),
        name,
        url: None,
        added_at: now,
        updated_at: now,
        update_interval_minutes: 0,
        last_auto_attempt_at: 0,
    }
}

/// 自动更新间隔下限（分钟）；更短的间隔对订阅服务器不友好。
/// debug 构建放宽到 1 分钟便于真机快速验证调度链路。
pub(crate) const MIN_UPDATE_INTERVAL_MINUTES: u64 = if cfg!(debug_assertions) { 1 } else { 10 };
/// 自动更新间隔上限（分钟），30 天。
pub(crate) const MAX_UPDATE_INTERVAL_MINUTES: u64 = 43_200;

/// 解析用户输入的自动更新间隔：`0` 表示关闭，其余必须落在
/// [`MIN_UPDATE_INTERVAL_MINUTES`]..=[`MAX_UPDATE_INTERVAL_MINUTES`]。
pub(crate) fn parse_update_interval(input: &str) -> Result<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let minutes: u64 = trimmed
        .parse()
        .map_err(|_| anyhow::anyhow!("间隔必须是非负整数"))?;
    if minutes == 0 {
        return Ok(0);
    }
    if !(MIN_UPDATE_INTERVAL_MINUTES..=MAX_UPDATE_INTERVAL_MINUTES).contains(&minutes) {
        bail!(
            "间隔需在 {MIN_UPDATE_INTERVAL_MINUTES}..={MAX_UPDATE_INTERVAL_MINUTES} 分钟内，0 表示关闭"
        );
    }
    Ok(minutes)
}

/// 订阅是否已到自动更新期：以「最近成功更新」与「最近尝试」的较晚者为基准
/// 加间隔。墙钟比较让睡眠/休眠唤醒后立即补跑，时钟回拨只顺延不风暴。
/// `now` 为当前 UNIX 秒。
pub(crate) fn subscription_due(meta: &ProfileMeta, now: u64) -> bool {
    let Some(_url) = meta.url.as_ref() else {
        return false;
    };
    if meta.update_interval_minutes == 0 {
        return false;
    }
    let base = meta.updated_at.max(meta.last_auto_attempt_at);
    // base 为 0（从未更新过且从未尝试）时立即视为到期，启动后首跳即补跑。
    now >= base.saturating_add(meta.update_interval_minutes * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一次性本机 HTTP 服务，也可模拟 HTTP 代理；超时退出避免测试失败后遗留线程。
    fn subscription_server(status: u16, body: &str) -> (String, std::thread::JoinHandle<String>) {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let body = body.to_owned();
        let task = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "未收到测试订阅请求");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                headers.push_str(&line);
                if line == "\r\n" {
                    if headers.starts_with("CONNECT ") && status == 200 {
                        stream
                            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                            .unwrap();
                        headers.clear();
                    } else {
                        break;
                    }
                }
            }
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            headers
        });
        (format!("http://{address}"), task)
    }

    #[test]
    fn subscription_download_prefers_proxy_without_direct_request() {
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        origin.set_nonblocking(true).unwrap();
        let (proxy, task) = subscription_server(200, "proxies: []");
        let route = SubscriptionProxy {
            preferred: proxy,
            direct: SubscriptionDirect::System,
        };
        let url = format!("http://{}/subscription", origin.local_addr().unwrap());
        assert_eq!(
            download_subscription(&url, Some(&route)).unwrap(),
            "proxies: []"
        );
        task.join().unwrap();
        assert_eq!(
            origin.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn subscription_download_retries_direct_and_redacts_combined_errors() {
        for status in [200, 503] {
            let (proxy, proxy_task) = subscription_server(502, "");
            let (origin, origin_task) = subscription_server(status, "proxies: []");
            let route = SubscriptionProxy {
                preferred: proxy,
                direct: SubscriptionDirect::System,
            };
            let result = download_subscription(
                &format!("{origin}/private-path?token=test-only"),
                Some(&route),
            );
            if status == 200 {
                assert_eq!(result.unwrap(), "proxies: []");
            } else {
                let error = format!("{:#}", result.unwrap_err());
                assert!(error.contains("代理") && error.contains("直连"));
                assert!(!error.contains("private-path") && !error.contains("test-only"));
            }
            proxy_task.join().unwrap();
            origin_task.join().unwrap();
        }
        let (origin, task) = subscription_server(200, "proxies: []");
        assert_eq!(download_subscription(&origin, None).unwrap(), "proxies: []");
        task.join().unwrap();
    }

    #[test]
    fn subscription_download_never_uses_system_route_when_tun_direct_is_unavailable() {
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        origin.set_nonblocking(true).unwrap();
        let url = format!("http://{}/subscription", origin.local_addr().unwrap());
        for status in [200, 502] {
            let (proxy, task) = subscription_server(status, "proxies: []");
            let route = SubscriptionProxy {
                preferred: proxy,
                direct: SubscriptionDirect::Unavailable,
            };
            let result = download_subscription(&url, Some(&route));
            if status == 200 {
                assert_eq!(result.unwrap(), "proxies: []");
            } else {
                assert!(format!("{:#}", result.unwrap_err()).contains("直连入口未就绪"));
            }
            task.join().unwrap();
            assert_eq!(
                origin.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }

    #[test]
    #[ignore = "使用随包内核验证订阅 DIRECT 重试；只启动回环服务，不启用系统 TUN"]
    fn subscription_direct_retry_bypasses_global_and_rule_modes() {
        use crate::mihomo::{
            MihomoProcess,
            config::{merge_runtime, validate_kernel_config, write_runtime},
        };
        use std::{net::TcpListener, path::Path, time::Duration};
        let root = std::env::temp_dir().join(format!("pure-clash-direct-retry-{}", Uuid::new_v4()));
        let mut paths = AppPaths::portable(&root);
        paths.kernel_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("kernel");
        fs::create_dir_all(&paths.mihomo_config_dir).unwrap();
        fs::create_dir_all(&paths.mihomo_data_dir).unwrap();
        let mut baseline = ensure_baseline(&paths).unwrap();
        let controller_port = TcpListener::bind("127.0.0.1:0").unwrap();
        let mixed_port = TcpListener::bind("127.0.0.1:0").unwrap();
        baseline.controller_addr = controller_port.local_addr().unwrap().to_string();
        baseline.mixed_port = mixed_port.local_addr().unwrap().port();
        baseline.tun_enable = true;
        let version = env!("PURE_CLASH_DEFAULT_MIHOMO_VERSION");
        // 模拟上次保存的端口已被其他程序占用，走实际启动前的端口迁移与落盘流程。
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        baseline.subscription_direct_port = occupied.local_addr().unwrap().port();
        crate::mihomo::config::save_baseline(&paths, &baseline).unwrap();
        fs::write(
            &paths.default_mihomo_config_file,
            "rules:\n- MATCH,REJECT\n",
        )
        .unwrap();
        let reservation =
            prepare_subscription_direct_startup(&paths, &mut baseline, version, None).unwrap();
        assert_ne!(
            baseline.subscription_direct_port,
            occupied.local_addr().unwrap().port()
        );
        assert_eq!(ensure_baseline(&paths).unwrap(), baseline);
        let runtime = fs::read_to_string(&paths.runtime_mihomo_config_file).unwrap();
        assert_eq!(
            runtime,
            merge_runtime("rules:\n- MATCH,REJECT\n", &baseline).unwrap()
        );
        validate_kernel_config(&paths, version, &runtime).unwrap();
        let mut runtime: serde_yaml::Value = serde_yaml::from_str(&runtime).unwrap();
        // 保留生产 DIRECT 入口，但不改系统路由，也不访问任何公共 DNS 或订阅。
        runtime["tun"]["enable"] = false.into();
        runtime["dns"]["enable"] = false.into();
        runtime["rules"] = serde_yaml::to_value(vec!["MATCH,REJECT"]).unwrap();
        write_runtime(&paths, &serde_yaml::to_string(&runtime).unwrap()).unwrap();
        drop(controller_port);
        drop(mixed_port);
        drop(reservation);
        let mut process = MihomoProcess::start(
            &paths,
            version,
            &paths.runtime_mihomo_config_file,
            false,
            false,
        )
        .unwrap();
        let controller = Controller::new(&baseline);
        for _ in 0..50 {
            if controller.version().is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        controller.select_proxy("GLOBAL", "REJECT").unwrap();
        let direct_proxy = baseline.subscription_direct_proxy_url();
        let route = SubscriptionProxy {
            preferred: format!("http://127.0.0.1:{}", baseline.mixed_port),
            direct: SubscriptionDirect::Proxy(direct_proxy.clone()),
        };
        for mode in ["global", "rule"] {
            controller.patch_mode(mode).unwrap();
            controller.check_subscription_direct(&direct_proxy).unwrap();
            assert!(
                controller
                    .check_subscription_direct(&format!(
                        "socks5://pure-clash:wrong-password@127.0.0.1:{}",
                        baseline.subscription_direct_port
                    ))
                    .is_err()
            );
            let (origin, task) = subscription_server(200, "proxies: []");
            // 普通入口被 REJECT 阻断，专用入口仍可下载；认证头不能传给订阅服务器。
            assert!(Controller::download_subscription(&origin, Some(&route.preferred)).is_err());
            assert!(
                Controller::download_subscription(
                    &origin,
                    Some(&format!(
                        "socks5://127.0.0.1:{}",
                        baseline.subscription_direct_port
                    ))
                )
                .is_err()
            );
            assert_eq!(
                download_subscription(&origin, Some(&route)).unwrap(),
                "proxies: []"
            );
            assert!(
                !task
                    .join()
                    .unwrap()
                    .to_lowercase()
                    .contains("proxy-authorization")
            );
            assert_eq!(controller.configs().unwrap().mode.as_str(), mode);
        }
        // 未认证的原始 SOCKS UDP 包不能再经专用入口转发。
        let target = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        target
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut packet = vec![0, 0, 0, 1, 127, 0, 0, 1];
        packet.extend_from_slice(&target.local_addr().unwrap().port().to_be_bytes());
        packet.extend_from_slice(b"unauthenticated-test");
        sender
            .send_to(&packet, ("127.0.0.1", baseline.subscription_direct_port))
            .unwrap();
        let error = target.recv_from(&mut [0; 128]).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        // 同一端口只有 TCP 监听，UDP 仍可绑定，避免把一次丢包误当成修复成功。
        let _udp =
            std::net::UdpSocket::bind(("127.0.0.1", baseline.subscription_direct_port)).unwrap();
        process.stop().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    fn due_meta(interval_minutes: u64, updated_at: u64, last_attempt: u64) -> ProfileMeta {
        ProfileMeta {
            id: "test".to_owned(),
            name: "测试订阅".to_owned(),
            url: Some("https://example.com/sub".to_owned()),
            added_at: 0,
            updated_at,
            update_interval_minutes: interval_minutes,
            last_auto_attempt_at: last_attempt,
        }
    }

    #[test]
    fn editing_subscription_only_requests_download_for_changed_url() {
        let mut meta = due_meta(60, 9_000, 8_000);
        let original = meta.clone();
        // 只改间隔或链接首尾空白，不下载，也不伪造内容更新时间。
        assert!(
            !edit_subscription_metadata(&mut meta, " https://example.com/sub ", "120", 10_000)
                .unwrap()
        );
        assert_eq!(meta.update_interval_minutes, 120);
        assert_eq!(meta.updated_at, original.updated_at);
        assert!(
            edit_subscription_metadata(&mut meta, "https://example.com/new", "0", 10_000).unwrap()
        );
        assert_eq!(meta.url.as_deref(), Some("https://example.com/new"));
        assert_eq!(meta.id, original.id);
        assert_eq!(meta.name, original.name);
        assert_eq!(meta.added_at, original.added_at);
        assert_eq!(meta.updated_at, original.updated_at);
        assert_eq!(meta.last_auto_attempt_at, original.last_auto_attempt_at);
        assert_eq!(meta.update_interval_minutes, 0);
    }

    #[test]
    fn invalid_subscription_edits_preserve_all_metadata() {
        let mut meta = due_meta(60, 9_000, 8_000);
        let original = meta.clone();
        // 新地址合法但间隔不合法时也不能只提交一半字段。
        for (url, interval) in [
            ("ftp://example.com/sub", "120"),
            ("", "0"),
            ("https://example.com/new", "-1"),
        ] {
            assert!(edit_subscription_metadata(&mut meta, url, interval, 10_000).is_err());
            assert_eq!(meta, original);
        }
        meta.url = None;
        let local = meta.clone();
        assert!(
            edit_subscription_metadata(&mut meta, "https://example.com/new", "60", 10_000).is_err()
        );
        assert_eq!(meta, local);
    }

    #[test]
    fn enabling_interval_on_never_updated_subscription_starts_from_save_time() {
        let mut meta = due_meta(0, 0, 0);
        assert!(
            !edit_subscription_metadata(&mut meta, "https://example.com/sub", "60", 10_000)
                .unwrap()
        );
        assert_eq!(meta.updated_at, 0);
        assert_eq!(meta.last_auto_attempt_at, 10_000);
        assert!(!subscription_due(&meta, 10_000));
        assert!(subscription_due(&meta, 13_600));
    }

    #[test]
    fn reorder_moves_in_both_directions_without_changing_metadata() {
        let original: Vec<_> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|id| {
                let mut meta = due_meta(60, 9_000, 8_000);
                meta.id = id.into();
                meta
            })
            .collect();
        let mut profiles = original.clone();
        assert!(reorder_profiles(&mut profiles, "a", "c"));
        assert_eq!(
            profiles,
            vec![
                original[1].clone(),
                original[2].clone(),
                original[0].clone(),
                original[3].clone()
            ]
        );
        assert!(reorder_profiles(&mut profiles, "a", "b"));
        assert_eq!(profiles, original);
        assert!(reorder_profiles(&mut profiles, "d", "a"));
        assert_eq!(profiles.first().unwrap().id, "d");
        assert!(reorder_profiles(&mut profiles, "d", "c"));
        assert_eq!(profiles, original);
        for (source, target) in [("a", "a"), ("missing", "b"), ("a", "missing")] {
            assert!(!reorder_profiles(&mut profiles, source, target));
            assert_eq!(profiles, original);
        }
        assert!(!reorder_profiles(&mut [], "a", "b"));
    }

    #[test]
    fn edited_url_and_order_survive_config_save_without_changing_active_profile() {
        let root =
            std::env::temp_dir().join(format!("pure-clash-profile-edit-{}", new_profile_id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("app.json");
        let mut first = due_meta(60, 9_000, 8_000);
        first.id = "first".into();
        let mut second = first.clone();
        second.id = "second".into();
        let mut config = crate::config::AppConfig {
            profiles: vec![first, second],
            active_profile: Some("first".into()),
            ..Default::default()
        };
        edit_subscription_metadata(
            &mut config.profiles[0],
            "https://example.com/new",
            "120",
            10_000,
        )
        .unwrap();
        assert!(reorder_profiles(&mut config.profiles, "first", "second"));
        config.save(&path).unwrap();
        let saved: crate::config::AppConfig =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, config);
        assert_eq!(saved.active_profile.as_deref(), Some("first"));
        assert_eq!(
            saved.profiles[1].url.as_deref(),
            Some("https://example.com/new")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parse_update_interval_accepts_bounds_and_rejects_others() {
        assert_eq!(parse_update_interval("").unwrap(), 0);
        assert_eq!(parse_update_interval("0").unwrap(), 0);
        assert_eq!(
            parse_update_interval(&MIN_UPDATE_INTERVAL_MINUTES.to_string()).unwrap(),
            MIN_UPDATE_INTERVAL_MINUTES
        );
        assert_eq!(
            parse_update_interval(&MAX_UPDATE_INTERVAL_MINUTES.to_string()).unwrap(),
            MAX_UPDATE_INTERVAL_MINUTES
        );
        assert_eq!(parse_update_interval(" 60 ").unwrap(), 60);
        assert!(parse_update_interval("abc").is_err());
        assert!(parse_update_interval("-1").is_err());
        assert!(parse_update_interval("999999999").is_err());
    }

    #[test]
    fn subscription_due_uses_latest_base() {
        // 间隔以分钟计：60 分钟 = 3600 秒。
        // 正常到期：updated_at(9000) + 3600 = 12600。
        assert!(subscription_due(&due_meta(60, 9_000, 0), 12_600));
        assert!(!subscription_due(&due_meta(60, 9_000, 0), 12_599));
        // 失败顺延：尝试时间(9999)比成功更新更晚，到期 = 9999 + 3600 = 13599。
        assert!(!subscription_due(&due_meta(60, 9_000, 9_999), 12_600));
        assert!(subscription_due(&due_meta(60, 9_000, 9_999), 13_599));
        // 关闭自动更新、本地导入（无 URL）永不到期。
        assert!(!subscription_due(&due_meta(0, 0, 0), 1_000_000));
        let mut local = due_meta(60, 0, 0);
        local.url = None;
        assert!(!subscription_due(&local, 1_000_000));
        // 从未更新也从未尝试（base 为 0）：10 分钟间隔在 600 秒时到期，
        // 启动后首跳即补跑错过的更新。
        assert!(subscription_due(&due_meta(10, 0, 0), 600));
        assert!(!subscription_due(&due_meta(10, 0, 0), 599));
    }

    #[test]
    fn url_validation_accepts_only_http_forms() {
        assert!(validate_subscription_url("https://example.com/sub").is_ok());
        assert!(validate_subscription_url("http://example.com/sub").is_ok());
        assert!(validate_subscription_url("ftp://example.com").is_err());
        assert!(validate_subscription_url("https://").is_err());
        assert!(validate_subscription_url("example.com").is_err());
    }

    #[test]
    fn default_name_extracts_host() {
        assert_eq!(
            default_name_from_url("https://p.example.com/clash/token"),
            Some("p.example.com".to_owned())
        );
        assert_eq!(default_name_from_url("not-a-url"), None);
        assert_eq!(default_name_from_url("https:///path"), None);
    }

    #[test]
    fn local_profile_uses_filename_and_has_no_subscription_url() {
        assert_eq!(
            default_name_from_path(Path::new("C:/configs/home.yaml")),
            Some("home".to_owned())
        );
        assert_eq!(default_name_from_path(Path::new(".yaml")), None);
        let meta = local_meta("本地配置".to_owned());
        assert_eq!(meta.name, "本地配置");
        assert!(meta.url.is_none());
    }

    #[test]
    fn local_config_reader_rejects_non_utf8_and_oversized_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pure-clash-local-profile-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("应创建本地配置测试目录");

        let valid = root.join("valid.yaml");
        fs::write(&valid, "rules:\n- MATCH,DIRECT\n").expect("应写入 UTF-8 配置");
        assert!(read_local_config(&valid).is_ok());

        let invalid = root.join("invalid.yaml");
        fs::write(&invalid, [0xff, 0xfe]).expect("应写入无效 UTF-8 配置");
        assert!(read_local_config(&invalid).is_err());

        let oversized = root.join("oversized.yaml");
        fs::File::create(&oversized)
            .and_then(|file| file.set_len(MAX_DOWNLOAD_BYTES + 1))
            .expect("应创建超限稀疏文件");
        assert!(read_local_config(&oversized).is_err());

        fs::remove_dir_all(root).expect("应清理本地配置测试目录");
    }

    #[test]
    fn profile_ids_are_random_and_filename_safe() {
        let first = new_profile_id();
        let second = new_profile_id();
        assert_ne!(first, second);
        assert!(first.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    }

    /// 回归测试：激活配置后 runtime.yaml 必须包含订阅节点。
    /// 此前“添加订阅”路径未写 runtime.yaml，导致内核仍加载默认配置。
    #[test]
    fn activation_writes_subscription_nodes_into_runtime() {
        use crate::mihomo::config::LocalBaseline;
        use crate::mihomo::config::{merge_runtime, write_runtime};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pure-clash-activate-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("应创建测试目录");
        let mut paths = AppPaths::portable(&root);
        paths.kernel_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("kernel");
        fs::create_dir_all(&paths.mihomo_config_dir).expect("应创建配置目录");
        fs::create_dir_all(&paths.profiles_dir).expect("应创建 profiles 目录");

        let baseline = LocalBaseline {
            mode: crate::mihomo::controller::Mode::Rule,
            mixed_port: 17890,
            subscription_direct_port: 17899,
            controller_addr: "127.0.0.1:19098".to_owned(),
            secret: "activate-secret".to_owned(),
            tun_enable: false,
            find_process_always: false,
            listeners: Vec::new(),
        };
        let profile_yaml = "\
proxies:
- name: 美国 Vless主力
  type: vless
  server: 1.2.3.4
  port: 443
  uuid: 00000000-0000-0000-0000-000000000000
proxy-groups:
- name: PROXY
  type: select
  proxies:
  - 美国 Vless主力
rules:
- MATCH,PROXY
";
        let id = new_profile_id();
        fs::write(profile_yaml_path(&paths, &id), profile_yaml).expect("应写入配置文件");

        // 模拟激活链路：读取配置 → 合并基线 → 落地 runtime.yaml。
        let runtime = merge_runtime(&read_profile(&paths, &id).expect("应读取配置"), &baseline)
            .expect("应合并");
        write_runtime(&paths, &runtime).expect("应写入运行时配置");

        let runtime_on_disk =
            fs::read_to_string(&paths.runtime_mihomo_config_file).expect("应存在 runtime.yaml");
        assert!(
            runtime_on_disk.contains("美国 Vless主力"),
            "runtime 应包含订阅节点"
        );
        assert!(
            runtime_on_disk.contains("mixed-port: 17890"),
            "本地端口应覆盖"
        );

        let _ = fs::remove_dir_all(root);
    }

    /// 端到端验证：真实订阅下载 → 校验 → 激活 → 内核启动 → controller
    /// 模式与节点操作。依赖网络与随包内核，使用 `cargo test -- --ignored` 运行。
    #[test]
    #[ignore = "需要网络与随包内核，验证真实订阅端到端链路"]
    fn real_subscription_end_to_end() {
        use crate::mihomo::MihomoProcess;
        use crate::mihomo::controller::Mode;

        // 真实订阅只从本机环境变量读取，禁止写入源码或提交记录。
        let subscription_url = std::env::var("PURE_CLASH_TEST_SUBSCRIPTION_URL")
            .expect("请在本机设置 PURE_CLASH_TEST_SUBSCRIPTION_URL 后运行此测试");

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 UNIX_EPOCH")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("pure-clash-e2e-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).expect("应创建测试目录");
        let mut paths = AppPaths::portable(&root);
        paths.kernel_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("kernel");
        fs::create_dir_all(&paths.config_dir).expect("应创建配置目录");
        fs::create_dir_all(&paths.mihomo_config_dir).expect("应创建 Mihomo 配置目录");
        fs::create_dir_all(&paths.mihomo_data_dir).expect("应创建数据目录");
        fs::create_dir_all(&paths.profiles_dir).expect("应创建 profiles 目录");

        // 预写本地基线，避免默认 mixed-port 与本机已运行实例冲突。
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("应分配代理端口");
        let proxy_port = listener.local_addr().expect("应读取端口").port();
        drop(listener);
        fs::write(
            &paths.local_mihomo_config_file,
            format!(
                "mixed-port: {proxy_port}\nexternal-controller: 127.0.0.1:19097\nsecret: e2e-secret\n"
            ),
        )
        .expect("应写入基线");

        let version = env!("PURE_CLASH_DEFAULT_MIHOMO_VERSION");

        // 1. 下载真实订阅并走完整校验管线。
        let content = download_subscription(&subscription_url, None).expect("应下载订阅");
        let id = new_profile_id();
        let runtime = validate_and_store(&paths, version, &id, &content).expect("订阅应通过校验");
        assert!(runtime.contains(&format!("mixed-port: {proxy_port}")));
        assert!(!runtime.contains("enable: true\ntun") && !runtime.contains("tun: true"));

        // 2. 合并产物写入 runtime.yaml 并启动内核。
        write_runtime(&paths, &runtime).expect("应写入运行时配置");
        let mut process = MihomoProcess::start(
            &paths,
            version,
            &paths.runtime_mihomo_config_file,
            false,
            true,
        )
        .expect("应启动内核");

        // 3. controller 就绪后验证配置生效。
        let baseline = ensure_baseline(&paths).expect("应读取基线");
        let controller = Controller::new(&baseline);
        let mut ready = false;
        for _ in 0..50 {
            if controller.version().is_ok() {
                ready = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(ready, "controller 应就绪");

        let config = controller.configs().expect("应读取配置");
        assert_eq!(config.mode, Mode::Rule);

        let proxies = controller.proxies(Mode::Rule).expect("应读取代理组");
        let proxy_group = proxies
            .groups
            .iter()
            .find(|group| group.name == "PROXY")
            .expect("订阅的 PROXY 组应存在");
        assert!(proxy_group.selectable, "PROXY 应是 Selector 组");
        assert!(
            proxy_group
                .nodes
                .iter()
                .any(|node| node.name.contains("美国")),
            "应包含订阅中的美国节点"
        );

        // 4. 切换运行模式与节点，验证真实生效。
        controller.patch_mode("global").expect("应切换全局模式");
        let config = controller.configs().expect("应读取配置");
        assert_eq!(config.mode, Mode::Global);

        let target = proxy_group
            .nodes
            .iter()
            .find(|node| node.name.contains("美国"))
            .expect("应有美国节点");
        controller
            .select_proxy("PROXY", &target.name)
            .expect("应切换节点");
        let proxies = controller.proxies(Mode::Global).expect("应读取代理组");
        let group = proxies
            .groups
            .iter()
            .find(|group| group.name == "PROXY")
            .expect("PROXY 应存在");
        assert_eq!(group.now, target.name);

        // 5. mixed-port 连通测试：通过本地代理端口发起真实请求。
        let proxy =
            ureq::Proxy::new(&format!("http://127.0.0.1:{proxy_port}")).expect("应构造代理");
        let agent = ureq::AgentBuilder::new()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(15))
            .build();
        let response = agent
            .get("http://cp.cloudflare.com/generate_204")
            .call()
            .expect("应通过 mixed-port 收到响应");
        assert_eq!(response.status(), 204, "generate_204 应返回 204");

        process.stop().expect("应停止内核");
        let _ = fs::remove_dir_all(root);
    }
}
