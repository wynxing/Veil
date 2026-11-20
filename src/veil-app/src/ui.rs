use std::path::Path;

use egui::{Color32, CornerRadius, Margin, RichText, Stroke, Vec2};
use veil_engine::{CcdConstants, ScreenIdentity, ScreenItem, VddModePreference, VDD_MODE_PRESETS};

use crate::operation_worker::Action;
use crate::session::SessionState;
use crate::update;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyTone {
    Available,
    Idle,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenTone {
    Active,
    Holding,
    Processing,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThemePalette {
    pub light: bool,
    pub ok: Color32,
    pub holding: Color32,
    pub busy: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub muted: Color32,
    pub accent_fill: Color32,
    pub accent_text: Color32,
    pub card_fill: Color32,
    pub card_stroke: Color32,
    pub alert_fill: Color32,
    pub alert_stroke: Color32,
    pub danger_fill: Color32,
    pub danger_stroke: Color32,
    pub pill_fill: Color32,
}

impl ThemePalette {
    pub(crate) fn for_mode(light: bool) -> Self {
        if light {
            Self {
                light: true,
                ok: Color32::from_rgb(21, 128, 61),
                holding: Color32::from_rgb(180, 83, 9),
                busy: Color32::from_rgb(29, 78, 216),
                warning: Color32::from_rgb(180, 83, 9),
                danger: Color32::from_rgb(185, 28, 28),
                muted: Color32::from_rgb(100, 116, 139),
                accent_fill: Color32::from_rgb(37, 99, 235),
                accent_text: Color32::WHITE,
                card_fill: Color32::from_rgb(255, 255, 255),
                card_stroke: Color32::from_rgb(226, 232, 240),
                alert_fill: Color32::from_rgb(255, 251, 235),
                alert_stroke: Color32::from_rgb(217, 119, 6),
                danger_fill: Color32::from_rgb(254, 242, 242),
                danger_stroke: Color32::from_rgb(220, 38, 38),
                pill_fill: Color32::from_rgb(241, 245, 249),
            }
        } else {
            Self {
                light: false,
                ok: Color32::from_rgb(74, 222, 128),
                holding: Color32::from_rgb(251, 191, 36),
                busy: Color32::from_rgb(96, 165, 250),
                warning: Color32::from_rgb(251, 191, 36),
                danger: Color32::from_rgb(248, 113, 113),
                muted: Color32::from_rgb(148, 163, 184),
                accent_fill: Color32::from_rgb(37, 99, 235),
                accent_text: Color32::WHITE,
                card_fill: Color32::from_rgb(30, 34, 42),
                card_stroke: Color32::from_rgb(51, 58, 70),
                alert_fill: Color32::from_rgb(48, 38, 18),
                alert_stroke: Color32::from_rgb(217, 119, 6),
                danger_fill: Color32::from_rgb(56, 24, 28),
                danger_stroke: Color32::from_rgb(220, 38, 38),
                pill_fill: Color32::from_rgb(42, 48, 60),
            }
        }
    }
}

pub(crate) fn prefers_light_theme() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE, REG_DWORD,
    };
    let subkey = crate::to_wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value_name = crate::to_wide("AppsUseLightTheme");
    let mut key = std::ptr::null_mut();
    let open = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if open != ERROR_SUCCESS || key.is_null() {
        return false;
    }
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut value_type: u32 = 0;
    let query = unsafe {
        RegQueryValueExW(
            key,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            &mut data as *mut u32 as *mut u8,
            &mut size,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    query == ERROR_SUCCESS && value_type == REG_DWORD && data != 0
}

pub(crate) fn apply_theme(ctx: &egui::Context, light: bool) {
    let mut visuals = if light {
        let mut v = egui::Visuals::light();
        v.panel_fill = Color32::from_rgb(248, 250, 252);
        v
    } else {
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::from_rgb(20, 23, 29);
        v
    };
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    ctx.set_style(style);
}

pub(crate) fn classify_hotkey(status: &str) -> (HotkeyTone, &'static str) {
    if status.contains("不可用") {
        (HotkeyTone::Unavailable, "热键不可用")
    } else if status.contains("可用") {
        (HotkeyTone::Available, "热键可用")
    } else if status.contains("空闲") {
        (HotkeyTone::Idle, "热键待命")
    } else {
        (HotkeyTone::Idle, "热键未知")
    }
}

pub(crate) fn classify_screen_tone(item: &ScreenItem) -> ScreenTone {
    match item.confirmed.as_str() {
        "失败" => ScreenTone::Failed,
        "处理中" => ScreenTone::Processing,
        "已关闭" => ScreenTone::Holding,
        "已显示" => ScreenTone::Active,
        _ => ScreenTone::Unknown,
    }
}

pub(crate) enum UiCommand {
    Submit {
        action: Action,
        target: Option<ScreenIdentity>,
        label: &'static str,
    },
    CancelKeepOff,
    ConfirmReply(bool),
    SetVddMode(VddModePreference),
    SetStartup(bool),
    OpenUpdate(String),
    ExitApp,
    OpenLogLocation,
    OpenSessionsDirectory,
    CopyDiagnosticSummary,
}

pub(crate) fn build_diagnostic_summary(
    state: &SessionState,
    vdd_mode: VddModePreference,
    startup: bool,
) -> String {
    let mut lines = vec![
        format!("Veil 版本：{}", update::local_version()),
        format!("架构：{}", std::env::consts::ARCH),
        format!("热键状态：{}", state.hotkey_status),
        format!(
            "会话状态：{}（保持关闭中={}，操作线程异常={}）",
            if state.has_session { "活跃" } else { "空闲" },
            state.holding,
            state.worker_failed
        ),
        format!("虚拟桌面分辨率偏好：{}", vdd_mode_label(vdd_mode)),
        format!("开机自启：{}", if startup { "开" } else { "关" }),
        format!(
            "辅助虚拟输出：{}",
            if !state.auxiliary.visible {
                "已安装/已接管"
            } else if state.auxiliary.enabled {
                "未安装设备（驱动包可用）"
            } else {
                "缺少驱动包"
            }
        ),
        format!("物理显示器数量：{}", state.screens.len()),
    ];
    for (index, screen) in state.screens.iter().enumerate() {
        lines.push(format!(
            "  #{}: [{}] 确认={} 要求={} 可关={} 可恢复={}",
            index + 1,
            screen.kind,
            screen.confirmed,
            screen.wanted,
            screen.can_keep_off,
            screen.can_restore
        ));
    }
    lines.push(r"日志路径：%TEMP%\Veil-app.log / %TEMP%\Veil-driver-helper.log".into());
    lines.push(r"会话目录：%LOCALAPPDATA%\Veil".into());
    lines.join("\n")
}

pub(crate) fn vdd_mode_label(mode: VddModePreference) -> String {
    match mode {
        VddModePreference::MatchPhysical => "沿用被关闭的物理屏".into(),
        VddModePreference::Fixed { width, height } => format!("{width}×{height}"),
    }
}

pub(crate) fn open_log_location() -> bool {
    let log = std::env::temp_dir().join("Veil-app.log");
    if log.exists() {
        let arg = format!("/select,\"{}\"", log.display());
        shell_open("explorer.exe", Some(&arg))
    } else {
        open_directory(&std::env::temp_dir())
    }
}

pub(crate) fn open_sessions_directory() -> bool {
    let root = veil_engine::SessionPaths::root();
    let _ = std::fs::create_dir_all(&root);
    open_directory(&root)
}

fn open_directory(path: &Path) -> bool {
    shell_open(&path.to_string_lossy(), None)
}

fn shell_open(target: &str, params: Option<&str>) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = crate::to_wide("open");
    let target_w = crate::to_wide(target);
    let params_w = params.map(crate::to_wide);
    let rc = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target_w.as_ptr(),
            params_w.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    rc as usize > 32
}

pub(crate) fn render_panel(
    ctx: &egui::Context,
    palette: ThemePalette,
    state: &SessionState,
    vdd_mode: VddModePreference,
    startup: bool,
    update_offer: Option<&update::UpdateOffer>,
) -> Vec<UiCommand> {
    let mut actions = Vec::new();
    let busy = !state.pending.is_empty() || state.auto_stage || state.worker_failed;

    egui::TopBottomPanel::top("veil-header")
        .frame(
            egui::Frame::new()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(Margin::symmetric(14, 12)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Veil").size(19.0).strong());
                ui.label(
                    RichText::new(format!("v{}", update::local_version()))
                        .size(11.5)
                        .color(palette.muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (tone, badge_text) = classify_hotkey(&state.hotkey_status);
                    let badge_color = match tone {
                        HotkeyTone::Available => palette.ok,
                        HotkeyTone::Idle => palette.muted,
                        HotkeyTone::Unavailable => palette.danger,
                    };
                    let badge = egui::Frame::new()
                        .fill(palette.pill_fill)
                        .stroke(Stroke::new(1.0_f32, badge_color.gamma_multiply(0.55)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                status_dot(ui, badge_color);
                                ui.label(RichText::new(badge_text).size(11.5).color(badge_color));
                            });
                        });
                    badge.response.on_hover_text(format!(
                        "{}\n紧急热键：{}",
                        state.hotkey_status,
                        CcdConstants::HOTKEY_TEXT
                    ));
                });
            });

            ui.add_space(4.0);
            let (hotkey_tone, _) = classify_hotkey(&state.hotkey_status);
            let hotkey_unavailable = hotkey_tone == HotkeyTone::Unavailable;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    RichText::new("紧急恢复")
                        .size(11.5)
                        .color(if hotkey_unavailable {
                            palette.danger
                        } else {
                            palette.muted
                        }),
                );
                egui::Frame::new()
                    .fill(if hotkey_unavailable {
                        palette.danger_fill
                    } else {
                        palette.pill_fill
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if hotkey_unavailable {
                            palette.danger_stroke
                        } else {
                            palette.card_stroke
                        },
                    ))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        let key_text = RichText::new(CcdConstants::HOTKEY_TEXT)
                            .size(11.5)
                            .strong();
                        ui.label(if hotkey_unavailable {
                            key_text.color(palette.danger)
                        } else {
                            key_text
                        });
                    });
                if hotkey_unavailable {
                    ui.label(
                        RichText::new("注册失败，已禁止关屏")
                            .size(11.0)
                            .color(palette.danger),
                    );
                }
            });

            if let Some(offer) = update_offer {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(palette.card_fill)
                    .stroke(Stroke::new(1.0_f32, palette.accent_fill.gamma_multiply(0.6)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("发现新版本 {}", offer.version))
                                    .size(12.5)
                                    .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("查看更新").clicked() {
                                        actions.push(UiCommand::OpenUpdate(offer.url.clone()));
                                    }
                                },
                            );
                        });
                    });
            }

            if !state.detail.is_empty() {
                ui.add_space(6.0);
                let is_error = state.worker_failed || state.last_operation_error.is_some();
                if is_error {
                    egui::Frame::new()
                        .fill(palette.danger_fill)
                        .stroke(Stroke::new(1.0_f32, palette.danger_stroke))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&state.detail).size(12.5).color(palette.danger));
                        });
                } else {
                    ui.label(RichText::new(&state.detail).size(12.0).color(palette.muted));
                }
            }

            if !state.pending.is_empty() || state.auto_stage {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(palette.card_fill)
                    .stroke(Stroke::new(1.0_f32, palette.busy.gamma_multiply(0.65)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(RichText::new(&state.stage).strong());
                                ui.label(
                                    RichText::new(format!(
                                        "已等待 {} 秒（当前阶段 {} 秒）",
                                        state.operation_started.elapsed().as_secs(),
                                        state.stage_started.elapsed().as_secs()
                                    ))
                                    .size(11.5)
                                    .color(palette.muted),
                                );
                            });
                        });
                        if state.cancel_requested {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("取消已收到，正在等待设备操作结束并确认恢复状态。")
                                    .size(12.0)
                                    .color(palette.warning),
                            );
                        } else if state
                            .pending
                            .iter()
                            .any(|(_, action)| *action == Action::KeepOff)
                        {
                            ui.add_space(4.0);
                            if ui.button("取消本次操作").clicked() {
                                actions.push(UiCommand::CancelKeepOff);
                            }
                        }
                    });
            }

            if let Some((reason, _)) = &state.confirmation {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(palette.alert_fill)
                    .stroke(Stroke::new(1.0_f32, palette.alert_stroke))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("需要确认显示驱动操作")
                                .size(13.0)
                                .strong()
                                .color(palette.warning),
                        );
                        ui.label(RichText::new(reason).size(12.5));
                        ui.label(
                            RichText::new("继续后将触发 Windows 权限确认（UAC）。")
                                .size(11.5)
                                .color(palette.muted),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let ok_btn = egui::Button::new(
                                RichText::new("继续").color(palette.accent_text),
                            )
                            .fill(palette.accent_fill);
                            if ui.add(ok_btn).clicked() {
                                actions.push(UiCommand::ConfirmReply(true));
                            }
                            if ui.button("取消").clicked() {
                                actions.push(UiCommand::ConfirmReply(false));
                            }
                        });
                    });
            }
        });

    egui::TopBottomPanel::bottom("veil-footer")
        .frame(
            egui::Frame::new()
                .fill(ctx.style().visuals.panel_fill)
                .stroke(Stroke::new(1.0_f32, palette.card_stroke))
                .inner_margin(Margin::symmetric(14, 10)),
        )
        .show(ctx, |ui| {
            if state.auxiliary.visible {
                if !state.auxiliary.hint.is_empty() {
                    ui.label(
                        RichText::new(&state.auxiliary.hint)
                            .size(12.0)
                            .color(palette.warning),
                    );
                }
                if ui
                    .add_enabled(
                        state.auxiliary.enabled && !busy,
                        egui::Button::new(&state.auxiliary.label),
                    )
                    .clicked()
                {
                    actions.push(UiCommand::Submit {
                        action: Action::Install,
                        target: None,
                        label: "正在安装辅助虚拟输出",
                    });
                }
                ui.add_space(4.0);
            }

            ui.horizontal(|ui| {
                let restore_button = if state.holding || state.has_session {
                    egui::Button::new(RichText::new("恢复全部").color(palette.accent_text))
                        .fill(palette.accent_fill)
                } else {
                    egui::Button::new("恢复全部")
                };
                if ui.add(restore_button).clicked() {
                    actions.push(UiCommand::Submit {
                        action: Action::RestoreAll,
                        target: None,
                        label: "正在恢复全部物理屏",
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("退出").clicked() {
                        actions.push(UiCommand::ExitApp);
                    }
                });
            });

            ui.add_space(4.0);
            egui::CollapsingHeader::new(RichText::new("设置与诊断").size(12.5))
                .id_salt("veil-settings-diagnostics")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    let mut selected_mode = vdd_mode;
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("辅助虚拟桌面分辨率").size(12.5))
                                .on_hover_text(
                                    "只影响关光全部物理屏后留下的虚拟桌面，不改物理屏；刷新率保持 60Hz。已经关着时改选，需先恢复再关一次生效。",
                                );
                            egui::ComboBox::from_id_salt("vdd-mode")
                                .selected_text(vdd_mode_label(selected_mode))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut selected_mode,
                                        VddModePreference::MatchPhysical,
                                        "沿用被关闭的物理屏",
                                    );
                                    for &(width, height) in VDD_MODE_PRESETS {
                                        ui.selectable_value(
                                            &mut selected_mode,
                                            VddModePreference::Fixed { width, height },
                                            format!("{width}×{height}"),
                                        );
                                    }
                                });
                        });
                    });
                    ui.label(
                        RichText::new("仅影响关光全部物理屏后的虚拟桌面（60Hz），已关时需恢复后重关生效。")
                            .size(11.0)
                            .color(palette.muted),
                    );
                    if selected_mode != vdd_mode {
                        actions.push(UiCommand::SetVddMode(selected_mode));
                    }

                    let mut startup_next = startup;
                    if ui
                        .checkbox(&mut startup_next, "开机自启（默认关）")
                        .changed()
                    {
                        actions.push(UiCommand::SetStartup(startup_next));
                    }

                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("打开日志").clicked() {
                            actions.push(UiCommand::OpenLogLocation);
                        }
                        if ui.small_button("会话目录").clicked() {
                            actions.push(UiCommand::OpenSessionsDirectory);
                        }
                        if ui.small_button("复制诊断摘要").clicked() {
                            actions.push(UiCommand::CopyDiagnosticSummary);
                        }
                    });
                });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(Margin::symmetric(14, 6)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("物理显示器")
                        .size(12.5)
                        .strong()
                        .color(palette.muted),
                );
                ui.label(
                    RichText::new(format!("({})", state.screens.len()))
                        .size(12.0)
                        .color(palette.muted),
                );
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.screens.is_empty() {
                        egui::Frame::new()
                            .fill(palette.card_fill)
                            .stroke(Stroke::new(1.0_f32, palette.card_stroke))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(12, 16))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("暂无可显示的物理显示器。")
                                        .size(13.0)
                                        .color(palette.muted),
                                );
                            });
                        return;
                    }
                    for item in &state.screens {
                        render_screen_card(ui, palette, item, busy, &mut actions);
                        ui.add_space(8.0);
                    }
                });
        });

    actions
}

fn render_screen_card(
    ui: &mut egui::Ui,
    palette: ThemePalette,
    item: &ScreenItem,
    busy: bool,
    actions: &mut Vec<UiCommand>,
) {
    let tone = classify_screen_tone(item);
    let tone_color = match tone {
        ScreenTone::Active => palette.ok,
        ScreenTone::Holding => palette.holding,
        ScreenTone::Processing => palette.busy,
        ScreenTone::Failed => palette.danger,
        ScreenTone::Unknown => palette.muted,
    };
    let border_color = match tone {
        ScreenTone::Holding => palette.holding.gamma_multiply(0.65),
        ScreenTone::Failed => palette.danger.gamma_multiply(0.75),
        _ => palette.card_stroke,
    };
    egui::Frame::new()
        .fill(palette.card_fill)
        .stroke(Stroke::new(1.0_f32, border_color))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_dot(ui, tone_color);
                ui.label(RichText::new(&item.name).size(14.0).strong());
                egui::Frame::new()
                    .fill(palette.pill_fill)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(6, 1))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&item.kind).size(11.0).color(palette.muted));
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(&item.status_text)
                            .size(12.0)
                            .color(tone_color),
                    );
                });
            });

            if !item.block_reason.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&item.block_reason)
                        .size(12.0)
                        .color(palette.warning),
                );
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let keep_off_enabled = item.can_keep_off && !busy;
                let keep_off_btn = if keep_off_enabled {
                    egui::Button::new(RichText::new("保持关闭").color(palette.accent_text))
                        .fill(palette.accent_fill)
                } else {
                    egui::Button::new("保持关闭")
                };
                if ui.add_enabled(keep_off_enabled, keep_off_btn).clicked() {
                    actions.push(UiCommand::Submit {
                        action: Action::KeepOff,
                        target: Some(item.identity.clone()),
                        label: "正在检查显示状态和恢复能力",
                    });
                }

                let restore_enabled = item.can_restore && !busy;
                if ui
                    .add_enabled(restore_enabled, egui::Button::new("恢复"))
                    .clicked()
                {
                    actions.push(UiCommand::Submit {
                        action: Action::RestoreOne,
                        target: Some(item.identity.clone()),
                        label: "正在恢复所选物理屏",
                    });
                }
            });
        });
}

fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::DEFAULT_DETAIL;

    #[test]
    fn hotkey_badge_classifies_available_idle_and_unavailable() {
        assert_eq!(
            classify_hotkey("Ctrl+Alt+Shift+F10：可用"),
            (HotkeyTone::Available, "热键可用")
        );
        assert_eq!(
            classify_hotkey("Ctrl+Alt+Shift+F10：不可用"),
            (HotkeyTone::Unavailable, "热键不可用")
        );
        assert_eq!(
            classify_hotkey("Ctrl+Alt+Shift+F10：空闲，下次关屏前启用"),
            (HotkeyTone::Idle, "热键待命")
        );
        assert_eq!(
            classify_hotkey("Ctrl+Alt+Shift+F10：未知"),
            (HotkeyTone::Idle, "热键未知")
        );
    }

    #[test]
    fn screen_status_classifies_all_states() {
        let make = |confirmed: &str| ScreenItem {
            identity: ScreenIdentity::new("luid", 1, r"\\?\DISPLAY#SENSITIVE#1"),
            name: "内置屏".into(),
            kind: "内置".into(),
            wanted: "开启".into(),
            confirmed: confirmed.into(),
            can_keep_off: true,
            can_restore: false,
            status_text: String::new(),
            block_reason: String::new(),
        };
        assert_eq!(classify_screen_tone(&make("已显示")), ScreenTone::Active);
        assert_eq!(classify_screen_tone(&make("已关闭")), ScreenTone::Holding);
        assert_eq!(
            classify_screen_tone(&make("处理中")),
            ScreenTone::Processing
        );
        assert_eq!(classify_screen_tone(&make("失败")), ScreenTone::Failed);
        assert_eq!(classify_screen_tone(&make("未知")), ScreenTone::Unknown);
    }

    #[test]
    fn diagnostic_summary_excludes_raw_device_paths() {
        let mut state = SessionState::new(DEFAULT_DETAIL.into(), "Ctrl+Alt+Shift+F10：空闲".into());
        state.screens.push(ScreenItem {
            identity: ScreenIdentity::new(
                "000000000000ABCD",
                42,
                r"\\?\DISPLAY#SECRET_EDID_SERIAL#4&12345678",
            ),
            name: "Dell U2723QE".into(),
            kind: "外接".into(),
            wanted: "开启".into(),
            confirmed: "已显示".into(),
            can_keep_off: true,
            can_restore: false,
            status_text: "已显示（开启）".into(),
            block_reason: String::new(),
        });
        let summary = build_diagnostic_summary(&state, VddModePreference::MatchPhysical, false);
        assert!(summary.contains("Veil 版本："));
        assert!(summary.contains("物理显示器数量：1"));
        assert!(!summary.contains("SECRET_EDID_SERIAL"));
        assert!(!summary.contains("000000000000ABCD"));
        assert!(!summary.contains(r"\\?\DISPLAY"));
    }

    #[test]
    fn theme_palettes_differ_between_light_and_dark() {
        let light = ThemePalette::for_mode(true);
        let dark = ThemePalette::for_mode(false);
        assert!(light.light);
        assert!(!dark.light);
        assert_ne!(light.card_fill, dark.card_fill);
        assert_ne!(light.warning, dark.warning);
    }
}
