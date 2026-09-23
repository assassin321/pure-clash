//! 配置页：内置默认配置行、订阅列表与添加/更新/删除/激活操作。

use gpui::{AnyElement, Context, SharedString, Styled, div, px};
use rust_i18n::t;

use super::*;
use crate::assets::{ICON_CIRCLE_CHECK, ICON_FILE_CODE, ICON_FOLDER, ICON_SEND, ICON_SHIELD_CHECK};
use crate::config::ProfileMeta;
use crate::theme::{FontWeightExt, Palette};

/// GPUI 内部拖动载荷与预览；以稳定 ID 定位，不携带订阅链接。
#[derive(Clone)]
struct ProfileDrag {
    /// 用户配置的持久 ID；内置默认配置不参与拖动。
    id: String,
    /// 预览中显示的配置名称。
    name: SharedString,
    /// 当前主题配色。
    palette: Palette,
}

impl Render for ProfileDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .max_w(px(320.0))
            .rounded_md()
            .bg(self.palette.surface)
            .border_1()
            .border_color(self.palette.accent)
            .shadow_md()
            .text_sm()
            .text_color(self.palette.text)
            .child(div().truncate().child(self.name.clone()))
    }
}

pub(super) fn render_profiles(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let busy = app.profile_actions_locked();
    div()
        .p_6()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(section_heading(
                    tr("profiles.title"),
                    tr("profiles.detail"),
                    ICON_FILE_CODE,
                    palette,
                ))
                .child(
                    div()
                        .id("profiles-add")
                        .h(px(30.0))
                        .px_3()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .gap_1()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.accent)
                        .text_xs()
                        .font_medium()
                        .text_color(palette.surface)
                        .child(tr("profiles.add"))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_profile_form(cx))),
                ),
        )
        .when(app.profile_form_open, |page| {
            page.child(render_profile_form(app, palette, cx))
        })
        .when_some(app.profile_error.as_ref(), |page, error| {
            page.child(
                div()
                    .mt_3()
                    .p_3()
                    .rounded_sm()
                    .bg(palette.surface_alt)
                    .text_xs()
                    .text_color(rgb(0xd15b5b))
                    .child(error.clone()),
            )
        })
        .when_some(app.profile_busy.as_ref(), |page, busy| {
            page.child(
                div()
                    .mt_3()
                    .p_3()
                    .rounded_sm()
                    .bg(palette.surface_alt)
                    .text_xs()
                    .text_color(palette.accent)
                    .child(busy.clone()),
            )
        })
        .child(
            div()
                .mt_4()
                .flex()
                .flex_col()
                .gap_3()
                // 内置默认配置常驻首行：无激活订阅时即为选中态。
                .child(default_profile_row(app, palette, cx))
                .children({
                    // 链接与间隔编辑器紧跟被编辑的订阅行展开。
                    let mut rows: Vec<AnyElement> = Vec::new();
                    for (index, meta) in app.profiles.iter().enumerate() {
                        rows.push(profile_row(app, index, meta, palette, cx));
                        if app.editing_profile_index == Some(index) {
                            rows.push(profile_editor(app, palette, cx));
                        }
                    }
                    rows
                }),
        )
        .when(app.profiles.is_empty(), |page| {
            page.child(
                div()
                    .mt_4()
                    .p_8()
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .bg(palette.surface)
                    .border_1()
                    .border_color(palette.border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(palette.text)
                            .child(tr("profiles.empty_title")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted)
                            .child(tr("profiles.empty_detail")),
                    ),
            )
        })
        .into_any_element()
}

/// 添加配置的内联表单：名称可选，可下载 URL 订阅或选择本地 YAML。
fn render_profile_form(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let busy = app.profile_actions_locked();
    div()
        .mt_4()
        .p_4()
        .rounded_md()
        .flex()
        .flex_col()
        .gap_3()
        .bg(palette.surface)
        .border_1()
        .border_color(palette.accent)
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(palette.text)
                .child(tr("profiles.form_title")),
        )
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .text_sm()
                .text_color(palette.text)
                .line_height(px(20.))
                .child(app.profile_form_name.clone()),
        )
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .text_sm()
                .text_color(palette.text)
                .line_height(px(20.))
                .child(app.profile_form_url.clone()),
        )
        // 可选的自动更新间隔：留空或 0 表示关闭；非法值在提交时反馈。
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .text_sm()
                .text_color(palette.text)
                .line_height(px(20.))
                .child(app.profile_form_interval.clone()),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id("profile-form-submit")
                        .flex_1()
                        .h(px(32.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.accent)
                        .text_xs()
                        .font_medium()
                        .text_color(palette.surface)
                        .gap_1()
                        .child(icon(ICON_SEND, palette.surface, 14.0))
                        .child(tr("profiles.form_submit"))
                        .on_click(cx.listener(|this, _, _, cx| this.add_subscription(cx))),
                )
                .child(
                    div()
                        .id("profile-form-import-local")
                        .flex_1()
                        .h(px(32.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.accent_soft)
                        .text_xs()
                        .font_medium()
                        .text_color(palette.accent)
                        .child(icon(ICON_FOLDER, palette.accent, 14.0))
                        .child(tr("profiles.import_local"))
                        .on_click(cx.listener(|this, _, _, cx| this.import_local_profile(cx))),
                )
                .child(
                    div()
                        .id("profile-form-cancel")
                        .flex_1()
                        .h(px(32.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.surface_alt)
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("profiles.form_cancel"))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_profile_form(cx))),
                ),
        )
        .into_any_element()
}

/// 格式化更新时间为 UTC 日期；时间戳为 0 时显示“未更新”。
pub(super) fn format_profile_time(timestamp: u64) -> SharedString {
    if timestamp == 0 {
        return tr("profiles.never_updated");
    }
    // 无chrono 依赖，用天数推算 UTC 日期，展示足够用的粗粒度时间。
    let days = timestamp / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    SharedString::from(format!("{year:04}-{month:02}-{day:02}"))
}

/// 从 UNIX 天数转换为民用日期（Howard Hinnant 算法）。
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 内置默认配置行：仅含 DIRECT 出站；无激活订阅时处于选中态，点击切回。
fn default_profile_row(
    app: &PureClash,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = app.active_profile.is_none();
    let busy = app.profile_actions_locked();
    div()
        .id("profile-builtin")
        .min_h(px(76.0))
        .p_4()
        .rounded_md()
        .flex()
        .items_center()
        .gap_3()
        .map(|row| {
            if busy {
                row.opacity(0.7)
            } else {
                row.cursor_pointer()
            }
        })
        .bg(palette.surface)
        .border_1()
        .border_color(if active {
            palette.accent
        } else {
            palette.border
        })
        .child(
            div()
                .size_9()
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .bg(if active {
                    palette.accent_soft
                } else {
                    palette.surface_alt
                })
                .child(icon(
                    if active {
                        ICON_CIRCLE_CHECK
                    } else {
                        ICON_SHIELD_CHECK
                    },
                    if active {
                        palette.accent
                    } else {
                        palette.muted
                    },
                    17.0,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.text)
                        .child(tr("profiles.builtin_name")),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("profiles.builtin_detail")),
                ),
        )
        .child(
            div().flex().items_center().gap_2().child(
                div()
                    .text_xs()
                    .text_color(if active {
                        palette.success
                    } else {
                        palette.muted
                    })
                    .child(tr(if active {
                        "profiles.active"
                    } else {
                        "profiles.activate"
                    })),
            ),
        )
        .on_click(cx.listener(|this, _, _, cx| this.activate_default_profile(cx)))
        .into_any_element()
}

fn profile_row(
    app: &PureClash,
    index: usize,
    meta: &ProfileMeta,
    palette: Palette,
    cx: &mut Context<PureClash>,
) -> AnyElement {
    let active = app.active_profile.as_deref() == Some(meta.id.as_str());
    // 行内编辑或后台自动更新进行中时锁定全部行的操作。
    let busy = app.profile_actions_locked();
    let target_id = meta.id.clone();
    let source = if meta.url.is_some() {
        tr("profiles.source_subscription")
    } else {
        tr("profiles.source_local")
    };
    // 副标题在“来源 · 更新时间”后追加自动更新状态。
    let mut detail = t!(
        "profiles.updated",
        source = source.to_string(),
        updated = format_profile_time(meta.updated_at).to_string()
    )
    .into_owned();
    if meta.update_interval_minutes > 0 {
        detail.push_str(" · ");
        detail.push_str(
            &t!(
                "profiles.auto_interval",
                interval = meta.update_interval_minutes.to_string()
            )
            .into_owned(),
        );
    }
    if meta.last_auto_attempt_at > meta.updated_at {
        detail.push_str(" · ");
        detail.push_str(&tr("profiles.auto_failed"));
    }
    div()
        .id(SharedString::from(format!("profile-{}", meta.id)))
        .min_h(px(76.0))
        .p_4()
        .rounded_md()
        .flex()
        .items_center()
        .gap_3()
        .map(|row| {
            if busy {
                row.opacity(0.7)
            } else {
                row.cursor_pointer()
            }
        })
        .bg(palette.surface)
        .border_1()
        .border_color(if active {
            palette.accent
        } else {
            palette.border
        })
        .when(!busy, |row| {
            // 高亮目标行；释放后按当前稳定 ID 排序，不会因旧下标移动错误的配置。
            row.drag_over::<ProfileDrag>(move |style, _, _, _| {
                style.bg(palette.accent_soft).border_color(palette.accent)
            })
            .on_drop(cx.listener(move |this, dragged: &ProfileDrag, _, cx| {
                cx.stop_propagation();
                this.reorder_profiles(&dragged.id, &target_id, cx);
            }))
        })
        .child(
            div()
                .id(SharedString::from(format!("profile-drag-{}", meta.id)))
                .w(px(18.0))
                .h_9()
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                // 六点手柄直接绘制，避免引入只用于拖动的图标依赖。
                .children((0..3).map(|_| {
                    div().flex().gap_1().children(
                        (0..2).map(|_| div().size(px(3.0)).rounded_full().bg(palette.muted)),
                    )
                }))
                .on_click(|_, _, cx| cx.stop_propagation())
                .when(!busy, |handle| {
                    handle.cursor_grab().on_drag(
                        ProfileDrag {
                            id: meta.id.clone(),
                            name: meta.name.clone().into(),
                            palette,
                        },
                        |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                    )
                }),
        )
        .child(
            div()
                .size_9()
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .bg(if active {
                    palette.accent_soft
                } else {
                    palette.surface_alt
                })
                .child(icon(
                    if active {
                        ICON_CIRCLE_CHECK
                    } else {
                        ICON_FILE_CODE
                    },
                    if active {
                        palette.accent
                    } else {
                        palette.muted
                    },
                    17.0,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(palette.text)
                        .child(meta.name.clone()),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(detail),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .children(if active {
                    vec![
                        div()
                            .text_xs()
                            .text_color(palette.success)
                            .child(tr("profiles.active"))
                            .into_any_element(),
                    ]
                } else {
                    vec![
                        div()
                            .text_xs()
                            .text_color(palette.muted)
                            .child(tr("profiles.activate"))
                            .into_any_element(),
                    ]
                })
                // 链接与更新间隔共用编辑入口；行内按钮不冒泡触发激活。
                .children(meta.url.as_ref().map(|_| {
                    div()
                        .id(SharedString::from(format!("profile-edit-{index}")))
                        .px_2()
                        .h(px(26.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .map(|button| {
                            if busy || app.profile_form_open {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.surface_alt)
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("profiles.edit"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.edit_profile(index, cx)
                        }))
                        .into_any_element()
                }))
                .children(meta.url.as_ref().map(|_| {
                    div()
                        .id(SharedString::from(format!("profile-update-{index}")))
                        .px_2()
                        .h(px(26.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.surface_alt)
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("profiles.update"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.update_profile(index, cx);
                        }))
                        .into_any_element()
                }))
                .children(Some({
                    div()
                        .id(SharedString::from(format!("profile-delete-{index}")))
                        .px_2()
                        .h(px(26.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                button.cursor_pointer()
                            }
                        })
                        .bg(palette.surface_alt)
                        .text_xs()
                        .text_color(rgb(0xd15b5b))
                        .child(tr("profiles.delete"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_profile(index, cx);
                        }))
                        .into_any_element()
                })),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.activate_profile_clicked(index, cx)))
        .into_any_element()
}

/// 订阅行内编辑器：链接修改后立即更新，仅改间隔时不触发下载。
fn profile_editor(app: &PureClash, palette: Palette, cx: &mut Context<PureClash>) -> AnyElement {
    div()
        .id("profile-editor")
        .p_4()
        .rounded_md()
        .flex()
        .flex_col()
        .gap_2()
        .bg(palette.surface)
        .border_1()
        .border_color(palette.accent)
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(palette.text)
                .child(tr("profiles.url_label")),
        )
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .text_sm()
                .text_color(palette.text)
                .line_height(px(20.))
                .child(app.profile_edit_url.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(palette.muted)
                .child(tr("profiles.url_edit_hint")),
        )
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(palette.text)
                .child(tr("profiles.interval_label")),
        )
        .child(
            div()
                .text_xs()
                .text_color(palette.muted)
                .child(tr("profiles.interval_hint")),
        )
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(palette.surface_alt)
                .border_1()
                .border_color(palette.border)
                .text_sm()
                .text_color(palette.text)
                .line_height(px(20.))
                .child(app.profile_form_interval.clone()),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id("profile-interval-save")
                        .flex_1()
                        .h(px(32.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .bg(palette.accent)
                        .text_xs()
                        .font_medium()
                        .text_color(palette.surface)
                        .child(tr("profiles.interval_save"))
                        .on_click(cx.listener(|this, _, _, cx| this.save_profile_edits(cx))),
                )
                .child(
                    div()
                        .id("profile-interval-cancel")
                        .flex_1()
                        .h(px(32.0))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .bg(palette.surface_alt)
                        .text_xs()
                        .text_color(palette.muted)
                        .child(tr("profiles.interval_cancel"))
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_profile_edit(cx))),
                ),
        )
        .into_any_element()
}
