//! The always-on-top, click-through status overlay.
//!
//! GPUI renders the overlay into a normal top-level window. The Win32 styles
//! that make it non-interactive are applied by `platform::window` immediately
//! after creation; this view only owns the pixels.

use gpui::prelude::*;
use gpui::{Context, Render, Window, canvas, div, point, px, rgba};
use gpui::{PathBuilder, PathStyle, StrokeOptions};

use crate::timer;
use crate::ui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverlayContent {
    Reminder,
    Countdown {
        remaining_seconds: u64,
        total_seconds: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayState {
    pub content: OverlayContent,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            content: OverlayContent::Reminder,
        }
    }
}

pub struct OverlayView {
    state: OverlayState,
}

impl OverlayView {
    pub fn new() -> Self {
        Self {
            state: OverlayState::default(),
        }
    }

    pub fn set_state(&mut self, state: OverlayState, cx: &mut Context<Self>) {
        if self.state != state {
            self.state = state;
            cx.notify();
        }
    }
}

impl Render for OverlayView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.state.content;
        let card = div()
            .font_family(ui::FONT_FAMILY)
            .flex()
            .items_center()
            .gap(px(22.))
            .w_full()
            .h_full()
            .px(px(24.))
            .py(px(18.))
            .rounded_3xl()
            .bg(rgba(0x080C12F2))
            .border_1()
            .border_color(rgba(0xFFFFFF1A))
            .shadow_2xl();

        match content {
            OverlayContent::Reminder => card.child(reminder_content()),
            OverlayContent::Countdown {
                remaining_seconds,
                total_seconds,
            } => card.child(countdown_content(remaining_seconds, total_seconds)),
        }
    }
}

fn reminder_content() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(20.))
        .w_full()
        .child(mouse_motif())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(ui::ACCENT))
                        .child("闲置提醒"),
                )
                .child(
                    div()
                        .text_size(px(19.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(ui::TEXT))
                        .child("请晃动鼠标以继续使用"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgba(ui::TEXT_MUTED))
                        .child("检测到持续无操作，达到设定时间后将自动关机"),
                ),
        )
}

fn countdown_content(remaining_seconds: u64, total_seconds: u64) -> impl IntoElement {
    let urgent = remaining_seconds <= 10;
    let accent = if urgent { ui::DANGER } else { ui::ACCENT };
    let progress = if total_seconds == 0 {
        0.0
    } else {
        (remaining_seconds as f64 / total_seconds as f64).clamp(0.0, 1.0) as f32
    };
    let display = timer::format_countdown(remaining_seconds);

    div()
        .flex()
        .items_center()
        .gap(px(22.))
        .w_full()
        .child(
            div()
                .relative()
                .flex_none()
                .size(px(116.))
                .child(progress_ring(progress, accent))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(if display.len() > 5 { px(21.) } else { px(27.) })
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgba(ui::TEXT))
                                .child(display.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(9.5))
                                .text_color(rgba(ui::TEXT_MUTED))
                                .child("剩余时间"),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(accent))
                        .child(if urgent {
                            "即将关机"
                        } else {
                            "关机倒计时已开始"
                        }),
                )
                .child(
                    div()
                        .text_size(px(19.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(ui::TEXT))
                        .child("请晃动鼠标或按任意键取消"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgba(ui::TEXT_MUTED))
                        .child(format!("系统将在 {display} 后执行关机")),
                ),
        )
}

fn mouse_motif() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .flex_none()
        .child(
            div()
                .w(px(34.))
                .h(px(50.))
                .rounded_full()
                .border_2()
                .border_color(rgba(0x38BDF8AA))
                .flex()
                .items_center()
                .justify_center()
                .pt(px(9.))
                .child(
                    div()
                        .w(px(3.))
                        .h(px(10.))
                        .rounded_full()
                        .bg(rgba(ui::ACCENT)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .w(px(9.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(rgba(0x38BDF866)),
                )
                .child(
                    div()
                        .w(px(13.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(rgba(0x38BDF888)),
                )
                .child(
                    div()
                        .w(px(7.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(rgba(0x38BDF844)),
                ),
        )
}

fn progress_ring(progress: f32, accent: u32) -> impl IntoElement {
    let progress = progress.clamp(0.0, 1.0);
    div().size(px(116.)).child(
        canvas(
            move |_, _, _| {},
            move |bounds, _, window, _| {
                let center = bounds.center();
                let radius = (bounds.size.width.min(bounds.size.height) / 2.0) - px(6.);
                let line_width = px(7.);

                // Filled disc behind the ring.
                if let Some(disc) = circle_path(center, radius + px(2.)) {
                    window.paint_path(disc, rgba(0xFFFFFF06));
                }

                // Track ring.
                if let Some(track) = circle_stroke(center, radius, line_width) {
                    window.paint_path(track, rgba(0xFFFFFF1F));
                }

                if progress <= 0.001 {
                    return;
                }

                // Remaining-progress arc, starting at 12 o'clock and moving
                // clockwise. A near-full circle is drawn as two arcs to avoid
                // degenerate SVG arc handling.
                let mut builder = PathBuilder::stroke(line_width).with_style(PathStyle::Stroke(
                    StrokeOptions::default().with_line_width(7.0),
                ));
                let start = point(center.x, center.y - radius);
                builder.move_to(start);
                if progress >= 0.999 {
                    let bottom = point(center.x, center.y + radius);
                    builder.arc_to(point(radius, radius), px(0.), false, true, bottom);
                    builder.arc_to(point(radius, radius), px(0.), false, true, start);
                } else {
                    let angle = std::f32::consts::TAU * progress;
                    let end = point(
                        center.x + radius * angle.sin(),
                        center.y - radius * angle.cos(),
                    );
                    builder.arc_to(
                        point(radius, radius),
                        px(0.),
                        angle > std::f32::consts::PI,
                        true,
                        end,
                    );
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, rgba(accent));
                }
            },
        )
        .size(px(116.)),
    )
}

fn circle_path(
    center: gpui::Point<gpui::Pixels>,
    radius: gpui::Pixels,
) -> Option<gpui::Path<gpui::Pixels>> {
    let mut builder = PathBuilder::fill();
    builder.move_to(point(center.x, center.y - radius));
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center.x, center.y + radius),
    );
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center.x, center.y - radius),
    );
    builder.close();
    builder.build().ok()
}

fn circle_stroke(
    center: gpui::Point<gpui::Pixels>,
    radius: gpui::Pixels,
    width: gpui::Pixels,
) -> Option<gpui::Path<gpui::Pixels>> {
    let mut builder = PathBuilder::stroke(width).with_style(PathStyle::Stroke(
        StrokeOptions::default().with_line_width(7.0),
    ));
    builder.move_to(point(center.x, center.y - radius));
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center.x, center.y + radius),
    );
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center.x, center.y - radius),
    );
    builder.build().ok()
}
