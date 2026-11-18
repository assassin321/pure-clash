//! 设置页：内核与系统代理、界面语言、主题与运行目录信息。

use gpui::{Animation, AnimationExt, AnyElement, Context, SharedString, Styled, div, px};

use super::overview::integration_error_banner;
use super::*;
use crate::assets::{
    ICON_FOLDER, ICON_GIT_BRANCH, ICON_PLUS, ICON_REFRESH_CW, ICON_SETTINGS, ICON_SHIELD_CHECK,
};
use crate::config::{CustomListener, Language};
use crate::theme::{FontWeightExt, Palette};

pub(super) fn render_settings(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    div()
        .p_6()
        .when_some(app.integration_error.as_ref(), |page, error| {
            page.child(integration_error_banner(error, palette))
        })
        .child(
            div()
                .p_4()
                .rounded_md()
                .bg(palette.surface)
                .border_1()
                .border_color(palette.border)
                .child(section_heading(
                    tr("settings.kernel_proxy"),
                    tr("settings.integration_detail"),
                    ICON_SETTINGS,
                    palette,
                ))
                .child(setting_row(
                    "setting-autostart",
                    tr("settings.autostart"),
                    if app.autostart_available {
                        tr("settings.autostart_detail")
                    } else {
                        tr("settings.autostart_unavailable")
                    },
                    app.autostart_enabled,
                    app.autostart_available,
                    palette,
                    cx.listener(|this, _, _, cx| this.toggle_autostart(cx)),
                ))
                .child(setting_row(
                    "setting-system-proxy",
                    tr("settings.system_proxy"),
                    tr("settings.system_proxy_detail"),
                    app.system_proxy,
                    true,
                    palette,
                    cx.listener(|this, _, _, cx| this.toggle_system_proxy(cx)),
                ))
                .child(setting_row(
                    "setting-tun",
                    tr("settings.tun"),
                    if app.tun_switch_state() == SwitchState::Starting {
                        tr("settings.tun_starting_detail")
                    } else {
                        tr("settings.tun_detail")
                    },
                    app.tun_switch_state(),
                    app.core_operable(),
                    palette,
                    cx.listener(|this, _, _, cx| this.toggle_tun(cx)),
                ))
                .child(setting_row(
                    "setting-find-process",
                    tr("settings.find_process"),
                    tr("settings.find_process_detail"),
                    app.config.find_process_always,
                    app.baseline.is_some(),
                    palette,
                    cx.listener(|this, _, _, cx| this.toggle_find_process(cx)),
                ))
                .child(setting_row(
                    "setting-dark-theme",
                    tr("settings.dark_theme"),
                    tr("settings.dark_theme_detail"),
                    app.config.theme.is_dark(),
                    true,
                    palette,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_theme(cx);
                    }),
                ))
                .child(language_setting_row(app.config.language, palette, cx)),
        )
        .child(render_listeners_section(app, palette, cx))
        .child(
            div()
                .mt_4()
                .p_4()
                .rounded_md()
                .bg(palette.surface)
                .border_1()
                .border_color(palette.border)
                .child(section_heading(
                    tr("settings.geodata"),
                    tr("settings.geodata_detail"),
                    ICON_SHIELD_CHECK,
                    palette,
                ))
                .child(geodata_update_row(app, palette, cx)),
        )
        .child(
            div()
                .mt_4()
                .p_4()
                .rounded_md()
                .bg(palette.surface)
                .border_1()
                .border_color(palette.border)
                .child(section_heading(
                    tr("settings.runtime_dir"),
                    tr("settings.runtime_dir_detail"),
                    ICON_FOLDER,
                    palette,
                ))
                .child(path_row(
                    tr("settings.main_config"),
                    app.paths.config_display(),
                    palette,
                ))
                .child(path_row(
                    tr("settings.data_dir"),
                    app.paths.data_display(),
                    palette,
                ))
                .child(path_row(
                    tr("settings.mihomo_config"),
                    app.paths.runtime_mihomo_config_display(),
                    palette,
                ))
                .child(path_row(
                    tr("settings.mihomo_data"),
                    app.paths.mihomo_data_display(),
                    palette,
                ))
                .child(path_row(
                    tr("settings.controller"),
                    SharedString::from(
                        app.baseline
                            .as_ref()
                            .map(|baseline| baseline.controller_addr.clone())
                            .unwrap_or_else(|| "—".to_owned()),
                    ),
                    palette,
                )),
        )
        .into_any_element()
}

/// Geo 数据是随包资源的用户态副本；更新按钮只替换数据目录，不修改安装目录。
fn geodata_update_row(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let revision = short_revision(&app.geodata_info.revision);
    let updated = super::profiles::format_profile_time(app.geodata_info.updated_at);
    div()
        .mt_2()
        .min_h(px(64.0))
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(palette.border)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.text)
                        .child(tr("settings.geodata_files")),
                )
                .child(
                    div().mt_1().text_xs().text_color(palette.muted).child(
                        t!(
                            "settings.geodata_version",
                            revision = revision,
                            updated = updated.to_string()
                        )
                        .into_owned(),
                    ),
                )
                .children(app.geodata_status.as_ref().map(|status| {
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(status.clone())
                })),
        )
        .child(
            div()
                .id("update-geodata")
                .h(px(30.0))
                .px_3()
                .rounded_sm()
                .flex()
                .items_center()
                .gap_2()
                .map(|button| {
                    if app.geodata_updating {
                        button.opacity(0.6)
                    } else {
                        button.cursor_pointer()
                    }
                })
                .bg(palette.accent)
                .text_xs()
                .font_medium()
                .text_color(palette.surface)
                .child(icon(ICON_REFRESH_CW, palette.surface, 13.0))
                .child(if app.geodata_updating {
                    tr("settings.geodata_updating")
                } else {
                    tr("settings.geodata_update")
                })
                .on_click(cx.listener(|this, _, _, cx| this.update_geodata(cx))),
        )
        .into_any_element()
}

fn setting_row(
    id: &'static str,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    state: impl Into<SwitchState>,
    available: bool,
    palette: Palette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let title = title.into();
    let detail = detail.into();
    let state = state.into();
    let enabled = state == SwitchState::On;
    let pending = state == SwitchState::Starting;
    // 启动中保持文案可读，用居中的呼吸滑块表达过渡；不注册点击事件。
    let thumb = div().size_4().rounded_full().bg(palette.surface);
    let thumb = if pending {
        thumb
            .with_animation(
                "tun-starting-thumb",
                Animation::new(std::time::Duration::from_millis(1200))
                    .repeat()
                    .with_max_fps(20.0),
                |thumb, progress| {
                    thumb.opacity(0.55 + 0.45 * (progress * std::f32::consts::TAU).cos().abs())
                },
            )
            .into_any_element()
    } else {
        thumb.into_any_element()
    };
    div()
        .min_h(px(62.0))
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(palette.border)
        .opacity(if available || pending { 1.0 } else { 0.5 })
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.text)
                        .child(title),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(detail),
                ),
        )
        .when(pending, |row| {
            row.child(
                div()
                    .text_xs()
                    .text_color(palette.accent)
                    .child(state.label()),
            )
        })
        .child(
            div()
                .id(id)
                .w(px(38.0))
                .h(px(22.0))
                .p(px(3.0))
                .rounded_full()
                .flex()
                .items_center()
                .bg(if pending || enabled {
                    palette.accent
                } else {
                    palette.border
                })
                .when(pending, |toggle| toggle.justify_center())
                .when(enabled, |toggle| toggle.justify_end())
                .when(!pending && !enabled, |toggle| toggle.justify_start())
                .child(thumb)
                .when(available && !pending, |toggle| {
                    toggle.cursor_pointer().on_click(handler)
                }),
        )
        .into_any_element()
}

fn language_setting_row(
    language: Language,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    div()
        .min_h(px(62.0))
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(palette.border)
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.text)
                        .child(tr("settings.language")),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("settings.language_detail")),
                ),
        )
        .child(
            div()
                .flex()
                .p_1()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .child(language_option(
                    Language::Chinese,
                    tr("settings.language_chinese"),
                    language,
                    palette,
                    cx,
                ))
                .child(language_option(
                    Language::English,
                    tr("settings.language_english"),
                    language,
                    palette,
                    cx,
                )),
        )
        .into_any_element()
}

fn language_option(
    language: Language,
    label: SharedString,
    selected: Language,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = language == selected;
    div()
        .id(match language {
            Language::Chinese => "language-zh-cn",
            Language::English => "language-en-us",
        })
        .h(px(28.0))
        .px_3()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .bg(if active {
            palette.surface
        } else {
            palette.surface_alt
        })
        .text_color(if active {
            palette.accent
        } else {
            palette.muted
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| this.set_language(language, cx)))
        .into_any_element()
}

fn path_row(
    label: impl Into<SharedString>,
    path: impl Into<SharedString>,
    palette: Palette,
) -> AnyElement {
    let label = label.into();
    let path = path.into();
    div()
        .min_h(px(42.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(palette.border)
        .child(div().text_sm().text_color(palette.text).child(label))
        .child(
            div()
                .max_w(px(380.0))
                .text_xs()
                .text_color(palette.muted)
                .text_ellipsis()
                .whitespace_nowrap()
                .child(path),
        )
        .into_any_element()
}

fn render_listeners_section(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let mixed_port = app.baseline.as_ref().map_or(7890, |b| b.mixed_port);
    let default_desc = format!("127.0.0.1:{mixed_port} (mixed)");

    let mut container = div()
        .mt_4()
        .p_4()
        .rounded_md()
        .bg(palette.surface)
        .border_1()
        .border_color(palette.border)
        .child(section_heading(
            tr("settings.listeners"),
            tr("settings.listeners_detail"),
            ICON_GIT_BRANCH,
            palette,
        ))
        .child(
            div()
                .min_h(px(42.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(palette.border)
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(palette.muted)
                        .child("mixed-port (Default)"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(palette.text)
                        .child(SharedString::from(default_desc)),
                ),
        );

    for listener in &app.config.listeners {
        container = container.child(render_listener_row(listener, palette, cx));
    }

    container = container.child(render_listener_add_form(app, palette, cx));
    container.into_any_element()
}

fn render_listener_row(
    listener: &CustomListener,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let id_for_del = listener.id.clone();
    let title = format!(
        "{} ({}:{})",
        listener.name, listener.listener_type, listener.port
    );
    let current_rule = listener.rule.clone();

    div()
        .min_h(px(48.0))
        .py_2()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .border_b_1()
        .border_color(palette.border)
        .child(
            div()
                .flex_1()
                .text_sm()
                .font_medium()
                .text_color(palette.text)
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .p_0p5()
                        .rounded_sm()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .child(listener_rule_chip(
                            &listener.id,
                            None,
                            tr("settings.listener_mode_default"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(listener_rule_chip(
                            &listener.id,
                            Some("rule"),
                            tr("settings.listener_mode_rule"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(listener_rule_chip(
                            &listener.id,
                            Some("global"),
                            tr("settings.listener_mode_global"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(listener_rule_chip(
                            &listener.id,
                            Some("direct"),
                            tr("settings.listener_mode_direct"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        )),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("del-listener-{}", listener.id)))
                        .h(px(26.0))
                        .px_2p5()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("settings.delete"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_custom_listener(id_for_del.clone(), cx);
                        })),
                ),
        )
        .into_any_element()
}

fn listener_rule_chip(
    listener_id: &str,
    rule_val: Option<&'static str>,
    label: SharedString,
    current: Option<&str>,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = match (rule_val, current) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    let id_owned = listener_id.to_owned();
    let target_rule = rule_val.map(|s| s.to_owned());
    let chip_id = SharedString::from(format!(
        "listener-rule-{}-{}",
        listener_id,
        rule_val.unwrap_or("default")
    ));

    div()
        .id(chip_id)
        .h(px(24.0))
        .px_2()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .bg(if active {
            palette.surface
        } else {
            palette.surface_alt
        })
        .text_color(if active {
            palette.accent
        } else {
            palette.muted
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.update_custom_listener_rule(id_owned.clone(), target_rule.clone(), cx);
        }))
        .into_any_element()
}

fn render_listener_add_form(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let current_type = app.listener_form_type.clone();
    let current_rule = app.listener_form_rule.clone();

    div()
        .mt_3()
        .pt_2()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(140.0))
                        .p_1p5()
                        .rounded_sm()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .text_xs()
                        .text_color(palette.text)
                        .line_height(px(18.0))
                        .child(app.listener_form_name.clone()),
                )
                .child(
                    div()
                        .w(px(90.0))
                        .p_1p5()
                        .rounded_sm()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .text_xs()
                        .text_color(palette.text)
                        .line_height(px(18.0))
                        .child(app.listener_form_port.clone()),
                )
                .child(
                    div()
                        .flex()
                        .p_0p5()
                        .rounded_sm()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .child(form_type_option("http", &current_type, palette, cx))
                        .child(form_type_option("socks", &current_type, palette, cx))
                        .child(form_type_option("mixed", &current_type, palette, cx)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .p_0p5()
                        .rounded_sm()
                        .bg(palette.surface_alt)
                        .border_1()
                        .border_color(palette.border)
                        .child(form_rule_option(
                            None,
                            tr("settings.listener_mode_default"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(form_rule_option(
                            Some("rule"),
                            tr("settings.listener_mode_rule"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(form_rule_option(
                            Some("global"),
                            tr("settings.listener_mode_global"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        ))
                        .child(form_rule_option(
                            Some("direct"),
                            tr("settings.listener_mode_direct"),
                            current_rule.as_deref(),
                            palette,
                            cx,
                        )),
                )
                .child(
                    div()
                        .id("add-custom-listener-btn")
                        .h(px(28.0))
                        .px_3()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .bg(palette.accent)
                        .text_xs()
                        .font_medium()
                        .text_color(palette.surface)
                        .child(icon(ICON_PLUS, palette.surface, 13.0))
                        .child(tr("settings.save_and_reload"))
                        .on_click(cx.listener(|this, _, _, cx| this.add_custom_listener(cx))),
                ),
        )
        .into_any_element()
}

fn form_type_option(
    type_name: &'static str,
    current: &str,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = current == type_name;
    div()
        .id(SharedString::from(format!(
            "form-listener-type-{type_name}"
        )))
        .h(px(24.0))
        .px_2p5()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .bg(if active {
            palette.surface
        } else {
            palette.surface_alt
        })
        .text_color(if active {
            palette.accent
        } else {
            palette.muted
        })
        .child(type_name)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_listener_form_type(type_name.to_owned(), cx);
        }))
        .into_any_element()
}

fn form_rule_option(
    rule_val: Option<&'static str>,
    label: SharedString,
    current: Option<&str>,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = match (rule_val, current) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    let target = rule_val.map(|s| s.to_owned());
    let id = SharedString::from(format!(
        "form-listener-rule-{}",
        rule_val.unwrap_or("default")
    ));
    div()
        .id(id)
        .h(px(24.0))
        .px_2()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .bg(if active {
            palette.surface
        } else {
            palette.surface_alt
        })
        .text_color(if active {
            palette.accent
        } else {
            palette.muted
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_listener_form_rule(target.clone(), cx);
        }))
        .into_any_element()
}
