use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use veil_engine::CcdConstants;

use crate::app_log;
use crate::session::TrayCommand;
use crate::wake::UiWake;

pub(crate) fn build_tray() -> (Option<TrayIcon>, MenuId, MenuId, MenuId) {
    let menu = Menu::new();
    let open_item = MenuItem::new("打开面板", true, None);
    let restore_item = MenuItem::new("恢复全部", true, None);
    let exit_item = MenuItem::new("退出", true, None);
    let open_id = open_item.id().clone();
    let restore_id = restore_item.id().clone();
    let exit_id = exit_item.id().clone();
    let _ = menu.append(&open_item);
    let _ = menu.append(&restore_item);
    let _ = menu.append(&exit_item);
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Veil（紧急恢复：{}）", CcdConstants::HOTKEY_TEXT))
        .with_icon(tray_icon_for(false))
        .build()
    {
        Ok(icon) => Some(icon),
        Err(err) => {
            app_log(&format!("托盘图标未创建：{err}"));
            None
        }
    };
    (tray, open_id, restore_id, exit_id)
}

pub(crate) fn install_tray_handlers(
    wake: &Arc<UiWake>,
    commands: &Arc<Mutex<Vec<TrayCommand>>>,
    open_id: MenuId,
    restore_id: MenuId,
    exit_id: MenuId,
) {
    let menu_wake = Arc::clone(wake);
    let menu_commands = commands.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        let command = if ev.id == open_id {
            Some(TrayCommand::Open)
        } else if ev.id == restore_id {
            Some(TrayCommand::RestoreAll)
        } else if ev.id == exit_id {
            Some(TrayCommand::Exit)
        } else {
            None
        };
        if let Some(command) = command {
            menu_commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(command);
        }
        menu_wake.ping();
    }));

    let icon_wake = Arc::clone(wake);
    let icon_commands = commands.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if matches!(
            ev,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
        ) {
            icon_commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(TrayCommand::Open);
            icon_wake.ping();
        }
    }));
}

pub(crate) fn should_update_tray_icon(previous: Option<bool>, current: bool) -> bool {
    previous != Some(current)
}

pub(crate) fn tray_icon_for(holding: bool) -> Icon {
    let bytes = if holding {
        include_bytes!("../../../assets/icon/tray-holding.png").as_slice()
    } else {
        include_bytes!("../../../assets/icon/tray-idle.png").as_slice()
    };
    let rgba = image::load_from_memory(bytes)
        .expect("tray image")
        .to_rgba8();
    Icon::from_rgba(rgba.into_raw(), 16, 16).expect("tray icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_updates_only_on_state_change() {
        assert!(should_update_tray_icon(None, false));
        assert!(!should_update_tray_icon(Some(false), false));
        assert!(should_update_tray_icon(Some(false), true));
        assert!(!should_update_tray_icon(Some(true), true));
        assert!(should_update_tray_icon(Some(true), false));
    }
}
