//! Application bootstrap, window composition, tray command bridge, and the
//! idle timer loop.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Application, Bounds, SharedString, TitlebarOptions, WeakEntity,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, point, px, size,
};

use crate::config::{self, Settings};
use crate::diag;
use crate::main_view::MainView;
use crate::overlay::{OverlayContent, OverlayState, OverlayView};
use crate::platform::{autostart, icon, idle, shutdown, single_instance, tray, window};
use crate::state::Shared;
use crate::timer::{self, OverlayPhase};

const APP_TITLE: &str = "无操作自动关机";

/// Entry point invoked by `main.rs` on Windows.
pub fn run() {
    let instance_guard = match single_instance::acquire() {
        Ok(Some(guard)) => Some(guard),
        Ok(None) => return,
        Err(error) => {
            diag::error("单实例检查失败，将继续启动", &error);
            None
        }
    };

    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    let loaded = config::load();
    let mut settings = loaded.settings;
    let mut startup_warnings = Vec::new();
    if let Some(warning) = loaded.warning {
        startup_warnings.push(warning);
    }

    match autostart::is_enabled() {
        Ok(enabled) => settings.auto_start = enabled,
        Err(error) => {
            startup_warnings.push(format!("无法读取开机自启状态，暂按配置文件显示：{error}"));
        }
    }

    let config_path = loaded.path;

    Application::new().run(move |cx| {
        if let Err(error) = start_ui(cx, config_path, settings, startup_warnings) {
            diag::error("启动失败", &error);
            window::show_error_dialog("无操作自动关机 - 启动失败", &error);
            cx.quit();
        }
    });

    drop(instance_guard);
}

fn start_ui(
    cx: &mut App,
    config_path: std::path::PathBuf,
    settings: Settings,
    startup_warnings: Vec<String>,
) -> Result<(), String> {
    let shared = Rc::new(Shared {
        config_path,
        settings: RefCell::new(settings.clone()),
        launch: std::time::Instant::now(),
        main_hwnd: Default::default(),
        overlay_hwnd: Default::default(),
        exiting: Default::default(),
        app_icon: Default::default(),
    });

    let app_icon = icon::create_app_icon(64);
    if let Some(icon) = app_icon {
        shared.app_icon.set(icon.0 as isize);
    }

    // Create the hidden overlay first. Its first frame is rendered while the
    // window is still hidden, so the first ShowWindow paints complete content.
    let overlay_view = cx.new(|_| OverlayView::new());
    let overlay_bounds = overlay_bounds(cx)?;
    let overlay_shared = shared.clone();
    let _overlay_handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(overlay_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("IdleShutdownOverlay")),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                focus: false,
                show: false,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            {
                let view = overlay_view.clone();
                let icon = app_icon;
                move |window, cx| {
                    if let Some(hwnd) = window::hwnd_of(window) {
                        overlay_shared.overlay_hwnd.set(window::hwnd_to_isize(hwnd));
                        window::configure_overlay(hwnd);
                        if let Some(icon) = icon {
                            window::set_window_icon(hwnd, icon);
                        }
                    }
                    window.on_window_should_close(cx, |_window, _cx| false);
                    view.clone()
                }
            },
        )
        .map_err(|error| format!("无法创建悬浮窗：{error}"))?;

    let overlay_hwnd = shared.overlay_hwnd();
    if !overlay_hwnd.0.is_null() {
        window::place_overlay_bottom(overlay_hwnd, 430.0, 176.0);
    }

    let main_view = cx.new(|cx| {
        MainView::new(
            shared.clone(),
            settings.clone(),
            settings.auto_start,
            startup_warnings,
            cx,
        )
    });
    let first_focus = main_view.read(cx).shutdown_focus_handle(cx);

    let main_shared = shared.clone();
    let _main_handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    main_window_size(cx),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from(APP_TITLE)),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                focus: true,
                show: true,
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: false,
                is_minimizable: true,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            {
                let view = main_view.clone();
                let icon = app_icon;
                move |window, cx| {
                    if let Some(hwnd) = window::hwnd_of(window) {
                        main_shared.main_hwnd.set(window::hwnd_to_isize(hwnd));
                        if let Some(icon) = icon {
                            window::set_window_icon(hwnd, icon);
                        }
                    }
                    window.on_window_should_close(cx, |window, _cx| {
                        // Closing the settings window must never quit the
                        // application; it only hides to the tray.
                        if let Some(hwnd) = window::hwnd_of(window) {
                            window::hide_window(hwnd);
                        }
                        false
                    });
                    window.focus(&first_focus);
                    view.clone()
                }
            },
        )
        .map_err(|error| format!("无法创建主窗口：{error}"))?;

    // The overlay is intentionally not linked to the main window. It stays
    // hidden until the timer publishes a visible phase.

    cx.activate(true);

    let tray = tray::Tray::start(shared.main_hwnd.get())
        .map_err(|error| format!("无法创建任务栏托盘图标：{error}"))?;
    let commands = tray.commands().clone();
    let tray_slot = Rc::new(RefCell::new(Some(tray)));
    let tray_for_commands = tray_slot.clone();
    let shared_for_commands = shared.clone();

    // Normal exit goes through the tray command handler, which removes the
    // icon before quitting. This observer is a final safety net for any other
    // quit path (for example an OS session ending). The cleanup is idempotent.
    {
        let cleanup_shared = shared.clone();
        let cleanup_tray = tray_slot.clone();
        cx.on_app_quit(move |_cx| {
            let shared = cleanup_shared.clone();
            let tray_slot = cleanup_tray.clone();
            async move {
                let overlay_hwnd = shared.overlay_hwnd();
                if !overlay_hwnd.0.is_null() {
                    window::hide_overlay(overlay_hwnd);
                }
                if let Some(mut tray) = tray_slot.borrow_mut().take() {
                    tray.remove();
                }
                let icon_handle = shared.app_icon.replace(0);
                if icon_handle != 0 {
                    icon::destroy_icon(windows::Win32::UI::WindowsAndMessaging::HICON(
                        icon_handle as *mut std::ffi::c_void,
                    ));
                }
            }
        })
        .detach();
    }

    cx.spawn(async move |cx| {
        while let Ok(command) = commands.recv_async().await {
            if shared_for_commands.exiting.get() {
                break;
            }
            match command {
                tray::TrayCommand::ToggleMainWindow => {
                    let hwnd = shared_for_commands.main_hwnd();
                    window::toggle_main_window(hwnd);
                }
                tray::TrayCommand::Exit => {
                    clean_up_and_quit(cx, &shared_for_commands, &tray_for_commands);
                    break;
                }
            }
        }
    })
    .detach();

    spawn_timer(cx, shared.clone(), overlay_view.downgrade());
    Ok(())
}

fn main_window_size(cx: &App) -> gpui::Size<gpui::Pixels> {
    let preferred_height = 650.0;
    let display_height = cx
        .primary_display()
        .map(|display| f32::from(display.bounds().size.height))
        .unwrap_or(preferred_height + 80.0);
    // Leave room for the taskbar and window shadows on short displays; the
    // settings body is scrollable when the window has to be smaller.
    let height = (display_height - 80.0).clamp(500.0, preferred_height);
    size(px(580.), px(height))
}

fn overlay_bounds(cx: &App) -> Result<Bounds<gpui::Pixels>, String> {
    let display = cx
        .primary_display()
        .or_else(|| cx.displays().into_iter().next())
        .ok_or_else(|| "未找到可用显示器。".to_string())?;
    let display_bounds = display.bounds();
    let width = 430.0;
    let height = 176.0;
    let display_origin_x = f32::from(display_bounds.origin.x);
    let display_origin_y = f32::from(display_bounds.origin.y);
    let display_width = f32::from(display_bounds.size.width);
    let display_height = f32::from(display_bounds.size.height);
    let x = display_origin_x + (display_width - width) / 2.0;
    // Sit above the taskbar on typical 1080p and 768p layouts.
    let y = display_origin_y + display_height - height - 72.0;
    Ok(Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width), px(height)),
    })
}

fn spawn_timer(cx: &mut App, shared: Rc<Shared>, overlay: WeakEntity<OverlayView>) {
    cx.spawn(async move |cx| {
        let mut overlay_shown = false;
        let mut previous_input_tick = idle::last_input_tick();
        loop {
            if shared.exiting.get() {
                break;
            }

            let settings = shared.settings.borrow().clone();
            let since_launch = shared.launch.elapsed();
            let idle_snapshot = match idle::snapshot() {
                Some(snapshot) => snapshot,
                None => {
                    diag::log("GetLastInputInfo 失败，暂停本次判定。");
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let activity_detected =
                previous_input_tick.is_some_and(|tick| tick != idle_snapshot.last_input_tick);
            previous_input_tick = Some(idle_snapshot.last_input_tick);

            let hwnd = shared.overlay_hwnd();
            if hwnd.0.is_null() {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                continue;
            }

            // User activity always wins over an already-visible overlay,
            // including the case where the countdown is about to finish.
            if activity_detected {
                if overlay_shown {
                    window::hide_overlay(hwnd);
                    overlay_shown = false;
                }
                // Do not re-show in the same tick in which the input was
                // observed; re-evaluate deadlines from the fresh input time on
                // the next tick.
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                continue;
            }

            let effective_idle = timer::effective_idle(idle_snapshot.idle, since_launch);
            let state = timer::evaluate(&settings, effective_idle);

            if state.shutdown_due {
                // Re-read the real global idle time immediately before
                // initiating shutdown. New input in the polling interval
                // cancels the pending shutdown.
                if !shutdown_still_due(&settings, &shared) {
                    if overlay_shown {
                        window::hide_overlay(hwnd);
                        overlay_shown = false;
                    }
                    continue;
                }
                if overlay_shown {
                    window::hide_overlay(hwnd);
                }
                diag::log("达到无操作关机阈值，正在发起系统关机。");
                if let Err(error) = shutdown::request_shutdown() {
                    diag::error("发起关机失败", &error);
                    shutdown::show_failure_dialog(&error);
                }
                break;
            }

            match state.phase {
                OverlayPhase::Hidden => {
                    if overlay_shown {
                        window::hide_overlay(hwnd);
                        overlay_shown = false;
                    }
                }
                OverlayPhase::Reminder => {
                    let next_state = OverlayState {
                        content: OverlayContent::Reminder,
                    };
                    let _ = overlay.update(cx, |view, cx| view.set_state(next_state, cx));
                    if !overlay_shown {
                        window::show_overlay(hwnd);
                        overlay_shown = true;
                        // A hidden GPUI window may not have produced a frame
                        // after the state update; request one after showing so
                        // the first visible frame is never stale.
                        let _ = overlay.update(cx, |_view, cx| cx.notify());
                    }
                }
                OverlayPhase::Countdown {
                    remaining_seconds, ..
                } => {
                    let next_state = OverlayState {
                        content: OverlayContent::Countdown {
                            remaining_seconds,
                            total_seconds: settings.countdown_seconds,
                        },
                    };
                    let _ = overlay.update(cx, |view, cx| view.set_state(next_state, cx));
                    if !overlay_shown {
                        window::show_overlay(hwnd);
                        overlay_shown = true;
                        let _ = overlay.update(cx, |_view, cx| cx.notify());
                    }
                }
            }

            let poll_interval = if overlay_shown {
                // Fast enough to hide immediately on new input and to update
                // the visible countdown smoothly.
                Duration::from_millis(150)
            } else {
                // While hidden, poll at a low rate but wake up in time for a
                // short reminder/countdown window instead of sleeping through
                // it. GetLastInputInfo itself is a very cheap user32 query.
                timer::next_deadline_delay(&settings, effective_idle)
                    .map(|until_deadline| {
                        until_deadline.clamp(Duration::from_millis(50), Duration::from_millis(750))
                    })
                    .unwrap_or(Duration::from_millis(750))
            };
            cx.background_executor().timer(poll_interval).await;
        }
    })
    .detach();
}

fn shutdown_still_due(settings: &Settings, shared: &Shared) -> bool {
    let Some(raw_idle) = idle::idle_duration() else {
        return false;
    };
    let idle = timer::effective_idle(raw_idle, shared.launch.elapsed());
    timer::evaluate(settings, idle).shutdown_due
}

fn clean_up_and_quit(
    cx: &mut gpui::AsyncApp,
    shared: &Rc<Shared>,
    tray_slot: &Rc<RefCell<Option<tray::Tray>>>,
) {
    if shared.exiting.replace(true) {
        return;
    }
    diag::log("收到退出命令，正在清理窗口与托盘。");

    let overlay_hwnd = shared.overlay_hwnd();
    if !overlay_hwnd.0.is_null() {
        window::hide_overlay(overlay_hwnd);
    }
    let main_hwnd = shared.main_hwnd();
    if !main_hwnd.0.is_null() {
        window::hide_window(main_hwnd);
    }

    if let Some(mut tray) = tray_slot.borrow_mut().take() {
        tray.remove();
    }

    let icon_handle = shared.app_icon.replace(0);
    if icon_handle != 0 {
        icon::destroy_icon(windows::Win32::UI::WindowsAndMessaging::HICON(
            icon_handle as *mut std::ffi::c_void,
        ));
    }

    let _ = cx.update(|app| app.quit());
}
