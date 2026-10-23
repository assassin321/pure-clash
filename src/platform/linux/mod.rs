mod autostart;
mod child_guard;
mod elevation;
mod fonts;
pub(crate) mod process_guard;
mod single_instance;
mod system_proxy;
mod tray;
mod tun_service;
mod window_ctrl;

pub(crate) use autostart::{autostart_status, set_autostart};
pub(crate) use elevation::{ElevatedProcess, launch_elevated, run_elevated_helper_if_requested};
pub(crate) use fonts::{apply_ui_font, configure_fonts};
pub(crate) use single_instance::{SingleInstance, SingleInstanceState};
pub(crate) use system_proxy::{
    capture_system_proxy, restore_system_proxy, set_system_proxy_endpoints,
};
pub(crate) use tray::SystemTray;
pub(crate) use window_ctrl::show_main_window;
