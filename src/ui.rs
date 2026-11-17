//! Shared visual language and the compact duration editor used by the settings
//! window and the floating overlay.

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, Focusable, FontWeight, KeyDownEvent, MouseButton, Render, Window, div,
    px, rgba,
};

use crate::timer;

/// Fonts that ship with Windows 10/11 and cover both Latin text and Simplified
/// Chinese. GPUI's DirectWrite backend falls back automatically if the UI font
/// is unavailable.
pub const FONT_FAMILY: &str = "Microsoft YaHei UI";

pub const WINDOW_BG: u32 = 0x0B1017F2;
pub const CARD_BG: u32 = 0xFFFFFF08;
pub const BORDER: u32 = 0xFFFFFF14;
pub const TEXT: u32 = 0xF2F6FA;
pub const TEXT_MUTED: u32 = 0x93A1B3;
pub const TEXT_DIM: u32 = 0x637081;
pub const ACCENT: u32 = 0x38BDF8;
pub const DANGER: u32 = 0xF87171;

/// A compact `HH:MM:SS` numeric input.
///
/// Users type digits; the value is formatted as a time as they type, and the
/// whole field is selected when clicked. Up/Down adjust one second (Shift adds
/// a minute). The field accepts up to eight digits, which covers 9,999 hours.
pub struct TimeField {
    id: usize,
    focus_handle: gpui::FocusHandle,
    digits: String,
    replace_on_type: bool,
}

impl TimeField {
    pub fn new(index: usize, seconds: u64, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let mut field = Self {
            id: index,
            focus_handle,
            digits: String::new(),
            replace_on_type: true,
        };
        field.set_seconds(seconds);
        field
    }

    pub fn focus_handle(&self) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }

    pub fn seconds(&self) -> u64 {
        if self.digits.is_empty() {
            return 0;
        }
        let digits = self.digits.as_str();
        let len = digits.len();
        let seconds = if len >= 2 {
            digits[len - 2..].parse::<u64>().unwrap_or(0)
        } else {
            digits.parse::<u64>().unwrap_or(0)
        };
        let minutes = if len >= 4 {
            digits[len - 4..len - 2].parse::<u64>().unwrap_or(0)
        } else if len == 3 {
            digits[..1].parse::<u64>().unwrap_or(0)
        } else {
            0
        };
        let hours = if len > 4 {
            digits[..len - 4].parse::<u64>().unwrap_or(0)
        } else {
            0
        };
        hours
            .saturating_mul(3600)
            .saturating_add(minutes.saturating_mul(60))
            .saturating_add(seconds)
    }

    pub fn set_seconds(&mut self, seconds: u64) {
        self.digits = timer::format_hms(seconds).replace(':', "");
        while self.digits.len() > 1 && self.digits.starts_with('0') {
            self.digits.remove(0);
        }
        if self.digits == "0" {
            self.digits.clear();
        }
        self.replace_on_type = true;
    }

    pub fn adjust_seconds(&mut self, delta_seconds: i64) {
        let current = self.seconds() as i64;
        let next = current.saturating_add(delta_seconds).max(0) as u64;
        self.set_seconds(next);
    }

    fn display_text(&self) -> String {
        timer::format_hms(self.seconds())
    }

    fn on_mouse_down(
        &mut self,
        _: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.replace_on_type = true;
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let modifiers = &event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform {
            return;
        }

        match event.keystroke.key.as_str() {
            "backspace" => {
                if self.replace_on_type {
                    self.digits.clear();
                    self.replace_on_type = false;
                } else {
                    self.digits.pop();
                }
                cx.notify();
            }
            "delete" => {
                self.digits.clear();
                self.replace_on_type = false;
                cx.notify();
            }
            "up" => {
                let step = if modifiers.shift { 60 } else { 1 };
                self.adjust_seconds(step);
                cx.notify();
            }
            "down" => {
                let step = if modifiers.shift { 60 } else { 1 };
                self.adjust_seconds(-step);
                cx.notify();
            }
            _ => {
                let mut typed_digits = Vec::new();
                if let Some(text) = &event.keystroke.key_char {
                    typed_digits.extend(text.chars().filter(char::is_ascii_digit));
                } else if let Some(digit) = key_digit(&event.keystroke.key) {
                    typed_digits.push(digit);
                }

                for digit in typed_digits {
                    if self.replace_on_type {
                        self.digits.clear();
                        self.replace_on_type = false;
                    }
                    if self.digits.len() < 8 {
                        self.digits.push(digit);
                    }
                    cx.notify();
                }
            }
        }
    }
}

impl Focusable for TimeField {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TimeField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let display = self.display_text();
        div()
            .id(("time-field", self.id))
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .h(px(40.))
            .w(px(166.))
            .px_3()
            .rounded_lg()
            .bg(if focused {
                rgba(0x38BDF81A)
            } else {
                rgba(0xFFFFFF0A)
            })
            .border_1()
            .border_color(if focused {
                rgba(0x38BDF880)
            } else {
                rgba(BORDER)
            })
            .cursor_pointer()
            .hover(|style| style.border_color(rgba(0x38BDF860)))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .font_family(FONT_FAMILY)
                    .text_size(px(17.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if focused { rgba(ACCENT) } else { rgba(TEXT) })
                    .child(display),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgba(TEXT_DIM))
                    .child("时:分:秒"),
            )
    }
}

fn key_digit(key: &str) -> Option<char> {
    if key.len() == 1 {
        let character = key.chars().next()?;
        return character.is_ascii_digit().then_some(character);
    }
    key.strip_prefix("numpad")
        .and_then(|number| number.parse::<char>().ok())
        .filter(char::is_ascii_digit)
}

/// Small `-` / `+` button for nudging a time field.
pub fn step_button(
    id: &'static str,
    label: &'static str,
    field: Entity<TimeField>,
    delta_seconds: i64,
) -> impl gpui::IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.))
        .rounded_md()
        .bg(rgba(0xFFFFFF0A))
        .border_1()
        .border_color(rgba(BORDER))
        .text_size(px(16.))
        .text_color(rgba(TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xFFFFFF14)).text_color(rgba(TEXT)))
        .active(|style| style.opacity(0.7))
        .on_click(move |_, _, cx| {
            field.update(cx, |field, cx| {
                field.adjust_seconds(delta_seconds);
                cx.notify();
            });
        })
        .child(label)
}
