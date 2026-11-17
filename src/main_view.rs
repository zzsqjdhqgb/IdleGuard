//! The settings window.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, Render, Window, WindowControlArea, div, px, rgba};

use crate::config::{self, Settings};
use crate::platform::{autostart, window};
use crate::state::Shared;
use crate::ui::{self, TimeField};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeKind {
    Info,
    Success,
    Warning,
    Error,
}

struct Notice {
    kind: NoticeKind,
    text: String,
}

pub struct MainView {
    shared: Rc<Shared>,
    shutdown_input: Entity<TimeField>,
    reminder_input: Entity<TimeField>,
    countdown_input: Entity<TimeField>,
    auto_start: bool,
    notice: Option<Notice>,
}

impl MainView {
    pub fn new(
        shared: Rc<Shared>,
        initial: Settings,
        auto_start: bool,
        startup_warnings: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shutdown_input = cx.new(|cx| TimeField::new(0, initial.shutdown_seconds, cx));
        let reminder_input = cx.new(|cx| TimeField::new(1, initial.reminder_seconds, cx));
        let countdown_input = cx.new(|cx| TimeField::new(2, initial.countdown_seconds, cx));

        let notice = Some(if startup_warnings.is_empty() {
            Notice {
                kind: NoticeKind::Info,
                text: "正在检测全局用户输入；任何鼠标或键盘操作都会重新计时。".to_string(),
            }
        } else {
            Notice {
                kind: NoticeKind::Warning,
                text: startup_warnings.join(" "),
            }
        });

        Self {
            shared,
            shutdown_input,
            reminder_input,
            countdown_input,
            auto_start,
            notice,
        }
    }

    pub fn shutdown_focus_handle(&self, cx: &gpui::App) -> gpui::FocusHandle {
        self.shutdown_input.read(cx).focus_handle()
    }

    fn set_notice(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notice = Some(Notice {
            kind,
            text: text.into(),
        });
    }

    fn collect_settings(&self, cx: &Context<Self>) -> Settings {
        Settings {
            shutdown_seconds: self.shutdown_input.read(cx).seconds(),
            reminder_seconds: self.reminder_input.read(cx).seconds(),
            countdown_seconds: self.countdown_input.read(cx).seconds(),
            auto_start: self.auto_start,
        }
    }

    fn save_settings(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let settings = self.collect_settings(cx);
        if let Err(error) = settings.validate() {
            self.set_notice(NoticeKind::Error, error.to_string());
            cx.notify();
            return;
        }

        match config::save(&self.shared.config_path, &settings) {
            Ok(()) => {
                self.shared.settings.replace(settings.clone());
                // Normalize whatever shorthand the user typed into a canonical
                // display value.
                self.shutdown_input.update(cx, |field, cx| {
                    field.set_seconds(settings.shutdown_seconds);
                    cx.notify();
                });
                self.reminder_input.update(cx, |field, cx| {
                    field.set_seconds(settings.reminder_seconds);
                    cx.notify();
                });
                self.countdown_input.update(cx, |field, cx| {
                    field.set_seconds(settings.countdown_seconds);
                    cx.notify();
                });
                self.set_notice(NoticeKind::Success, "设置已保存并立即生效。");
            }
            Err(error) => {
                self.set_notice(NoticeKind::Error, error);
            }
        }
        cx.notify();
    }

    fn toggle_auto_start(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let target = !self.auto_start;
        match autostart::set_enabled(target) {
            Ok(()) => {
                self.auto_start = target;
                let saved = {
                    let mut applied = self.shared.settings.borrow().clone();
                    applied.auto_start = target;
                    self.shared.settings.replace(applied.clone());
                    applied
                };
                let save_warning = config::save(&self.shared.config_path, &saved).err();
                if let Some(error) = save_warning {
                    self.set_notice(
                        NoticeKind::Warning,
                        format!(
                            "开机自启已{}，但配置写入失败：{error}",
                            if target { "开启" } else { "关闭" }
                        ),
                    );
                } else {
                    self.set_notice(
                        NoticeKind::Success,
                        if target {
                            "已开启开机自启。登录 Windows 后将自动在托盘中启动。"
                        } else {
                            "已关闭开机自启。"
                        },
                    );
                }
            }
            Err(error) => {
                self.set_notice(
                    NoticeKind::Error,
                    format!(
                        "{}开机自启失败：{error}",
                        if target { "开启" } else { "关闭" }
                    ),
                );
            }
        }
        cx.notify();
    }

    fn reload_settings(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let loaded = config::load();
        let mut warning = loaded.warning;
        let mut update_auto_start = loaded.settings.auto_start;
        match autostart::is_enabled() {
            Ok(enabled) => update_auto_start = enabled,
            Err(error) => {
                let message = format!("读取开机自启状态失败：{error}");
                warning = Some(match warning {
                    Some(existing) => format!("{existing} {message}"),
                    None => message,
                });
            }
        }

        let loaded_settings = Settings {
            auto_start: update_auto_start,
            ..loaded.settings
        };
        self.auto_start = update_auto_start;
        self.shared.settings.replace(loaded_settings.clone());
        self.shutdown_input.update(cx, |field, cx| {
            field.set_seconds(loaded_settings.shutdown_seconds);
            cx.notify();
        });
        self.reminder_input.update(cx, |field, cx| {
            field.set_seconds(loaded_settings.reminder_seconds);
            cx.notify();
        });
        self.countdown_input.update(cx, |field, cx| {
            field.set_seconds(loaded_settings.countdown_seconds);
            cx.notify();
        });
        if let Some(warning) = warning {
            self.set_notice(NoticeKind::Warning, warning);
        } else {
            self.set_notice(NoticeKind::Success, "已从配置文件重新载入设置。");
        }
        cx.notify();
    }

    fn render_notice(&self) -> gpui::AnyElement {
        let (icon, background, border, text_color) = match self.notice.as_ref().map(|n| n.kind) {
            Some(NoticeKind::Success) => ("✓", 0x34D39914, 0x34D39940, 0x6EE7B7),
            Some(NoticeKind::Warning) => ("!", 0xFBBF2414, 0xFBBF2440, 0xFCD34D),
            Some(NoticeKind::Error) => ("×", 0xF8717114, 0xF8717140, 0xFCA5A5),
            Some(NoticeKind::Info) | None => ("i", 0xFFFFFF0A, 0xFFFFFF14, ui::TEXT_MUTED),
        };

        let text = self
            .notice
            .as_ref()
            .map(|notice| notice.text.clone())
            .unwrap_or_else(|| {
                "正在检测全局用户输入；任何鼠标或键盘操作都会重新计时。".to_string()
            });

        div()
            .flex()
            .items_start()
            .gap(px(8.))
            .px(px(10.))
            .py(px(8.))
            .rounded_lg()
            .bg(rgba(background))
            .border_1()
            .border_color(rgba(border))
            .child(
                div()
                    .flex_none()
                    .w(px(16.))
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgba(border))
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0x081018))
                    .child(icon),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(11.5))
                    .line_height(px(16.))
                    .text_color(rgba(text_color))
                    .child(text),
            )
            .into_any_element()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shutdown_row = duration_row(
            "关机时间",
            "持续无操作达到该时长后执行关机",
            self.shutdown_input.clone(),
            "shutdown-minus",
            "shutdown-plus",
        );
        let reminder_row = duration_row(
            "提醒时间",
            "达到该时长后显示悬浮提醒",
            self.reminder_input.clone(),
            "reminder-minus",
            "reminder-plus",
        );
        let countdown_row = duration_row(
            "倒计时时间",
            "关机前该时长开始显示圆形倒计时",
            self.countdown_input.clone(),
            "countdown-minus",
            "countdown-plus",
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(ui::FONT_FAMILY)
            .bg(rgba(ui::WINDOW_BG))
            .rounded_2xl()
            .border_1()
            .border_color(rgba(ui::BORDER))
            .overflow_hidden()
            .child(self.render_titlebar())
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .w_full()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(ui::TEXT))
                                    .child("自动关机设置"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgba(ui::TEXT_MUTED))
                                    .child("所有鼠标移动、鼠标按键和键盘输入都会重新开始计时。"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(shutdown_row)
                            .child(reminder_row)
                            .child(countdown_row),
                    )
                    .child(
                        div()
                            .id("autostart-row")
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .px(px(16.))
                            .py(px(12.))
                            .rounded_xl()
                            .bg(rgba(ui::CARD_BG))
                            .border_1()
                            .border_color(rgba(ui::BORDER))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(ui::TEXT))
                                            .child("开机自启"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgba(ui::TEXT_MUTED))
                                            .child("登录 Windows 后自动启动"),
                                    ),
                            )
                            .child(auto_start_switch(self.auto_start))
                            .on_click(cx.listener(Self::toggle_auto_start)),
                    )
                    .child(self.render_notice()),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(24.))
                    .pt(px(10.))
                    .pb(px(20.))
                    .border_t_1()
                    .border_color(rgba(ui::BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .max_w(px(252.))
                                            .truncate()
                                            .text_size(px(11.))
                                            .text_color(rgba(ui::TEXT_MUTED))
                                            .child(format!(
                                                "配置：{}",
                                                self.shared.config_path.display()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgba(ui::TEXT_DIM))
                                            .child("关闭窗口只会隐藏到托盘；右键托盘图标可退出。"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(render_reload_button(cx))
                                    .child(render_save_button(cx)),
                            ),
                    ),
            )
    }
}

impl MainView {
    fn render_titlebar(&self) -> gpui::AnyElement {
        div()
            .flex_none()
            .h(px(46.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgba(ui::BORDER))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(18.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(app_mark())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(ui::TEXT))
                                    .child("无操作自动关机"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(rgba(ui::TEXT_DIM))
                                    .child("IdleShutdown"),
                            ),
                    ),
            )
            .child(title_button("minimize", "−", false, |window, _| {
                window.minimize_window()
            }))
            .child(title_button("close", "×", true, |window, _| {
                if let Some(hwnd) = window::hwnd_of(window) {
                    window::hide_window(hwnd);
                }
            }))
            .into_any_element()
    }
}

fn duration_row(
    label: &'static str,
    description: &'static str,
    field: Entity<TimeField>,
    minus_id: &'static str,
    plus_id: &'static str,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .px(px(16.))
        .py(px(11.))
        .rounded_xl()
        .bg(rgba(ui::CARD_BG))
        .border_1()
        .border_color(rgba(ui::BORDER))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(ui::TEXT))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgba(ui::TEXT_MUTED))
                        .child(description),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(ui::step_button(minus_id, "−", field.clone(), -60))
                .child(field.clone())
                .child(ui::step_button(plus_id, "+", field, 60)),
        )
}

fn app_mark() -> impl IntoElement {
    div()
        .flex_none()
        .size(px(26.))
        .rounded_lg()
        .bg(rgba(0x38BDF81A))
        .border_1()
        .border_color(rgba(0x38BDF860))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(11.))
                .rounded_full()
                .border_2()
                .border_color(rgba(ui::ACCENT))
                .flex()
                .items_center()
                .justify_center()
                .child(div().size(px(3.)).rounded_full().bg(rgba(ui::ACCENT))),
        )
}

fn auto_start_switch(enabled: bool) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(44.))
        .h(px(24.))
        .rounded_full()
        .bg(if enabled {
            rgba(0x38BDF8CC)
        } else {
            rgba(0xFFFFFF1F)
        })
        .px(px(3.))
        .flex()
        .items_center()
        .when(enabled, |this| this.justify_end())
        .cursor_pointer()
        .child(div().size(px(18.)).rounded_full().bg(if enabled {
            rgba(0x081018)
        } else {
            rgba(0xD5DEE8)
        }))
}

fn render_reload_button(cx: &mut Context<MainView>) -> impl IntoElement {
    div()
        .id("reload-settings-button")
        .flex()
        .items_center()
        .justify_center()
        .h(px(40.))
        .px(px(16.))
        .rounded_lg()
        .bg(rgba(0xFFFFFF0A))
        .border_1()
        .border_color(rgba(ui::BORDER))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(ui::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xFFFFFF14)).text_color(rgba(ui::TEXT)))
        .active(|style| style.opacity(0.75))
        .on_click(cx.listener(MainView::reload_settings))
        .child("重新载入")
}

fn render_save_button(cx: &mut Context<MainView>) -> impl IntoElement {
    div()
        .id("save-settings-button")
        .flex()
        .items_center()
        .justify_center()
        .h(px(42.))
        .px(px(24.))
        .rounded_lg()
        .bg(rgba(ui::ACCENT))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(0x07121A))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0x7DD3FC)))
        .active(|style| style.opacity(0.82))
        .on_click(cx.listener(MainView::save_settings))
        .child("保存并应用")
}

fn title_button(
    id: &'static str,
    label: &'static str,
    danger: bool,
    handler: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let base = div()
        .id(id)
        .w(px(46.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(17.))
        .text_color(rgba(ui::TEXT_MUTED))
        .cursor_pointer()
        .hover(move |style| {
            if danger {
                style.bg(rgba(0xF8717122)).text_color(rgba(0xFCA5A5))
            } else {
                style.bg(rgba(0xFFFFFF0D)).text_color(rgba(ui::TEXT))
            }
        })
        .active(|style| style.opacity(0.7))
        .on_click(move |_, window, cx| handler(window, cx));
    base.child(label)
}
