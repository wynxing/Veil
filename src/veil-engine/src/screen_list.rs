use crate::capability::{Gate, PathRole};
use crate::session::HeartbeatFile;
use crate::{DisplaySnapshot, ScreenIdentity};

#[derive(Clone, Debug)]
pub struct ScreenItem {
    pub identity: ScreenIdentity,
    pub name: String,
    pub kind: String,
    pub wanted: String,
    pub confirmed: String,
    pub can_keep_off: bool,
    pub can_restore: bool,
    pub status_text: String,
    pub block_reason: String,
}

impl ScreenItem {
    pub fn keep_off_automation_id(&self) -> String {
        format!("KeepOffButton-{}", self.identity.target_id)
    }
    pub fn restore_automation_id(&self) -> String {
        format!("RestoreButton-{}", self.identity.target_id)
    }
}

pub struct ScreenListBuilder;

impl ScreenListBuilder {
    pub fn build(
        snapshot: &DisplaySnapshot,
        heartbeat: Option<&HeartbeatFile>,
        pending_wanted: &[ScreenIdentity],
        bundled_vdd_installed: bool,
        recovery_ready: bool,
        hotkey_registered: bool,
    ) -> Vec<ScreenItem> {
        let mut items = Vec::new();
        for row in snapshot.physical_screens() {
            let hb = heartbeat.and_then(|h| {
                h.screens.iter().find(|s| {
                    row.identity()
                        .matches(&ScreenIdentity::new(&s.adapter_luid, s.target_id, &s.monitor_path))
                })
            });
            let wanted = hb
                .map(|s| s.wanted.clone())
                .unwrap_or_else(|| {
                    if pending_wanted.iter().any(|id| id.matches(&row.identity())) {
                        "保持关闭".into()
                    } else {
                        "开启".into()
                    }
                });
            let mut confirmed = hb
                .map(|s| s.confirmed.clone())
                .unwrap_or_else(|| if row.active { "已显示".into() } else { "未知".into() });
            if confirmed == "失败" && hb.is_none() {
                confirmed = "失败".into();
            }
            let mut already = pending_wanted.to_vec();
            if let Some(h) = heartbeat {
                for s in h.screens.iter().filter(|s| s.wanted == "保持关闭") {
                    already.push(ScreenIdentity::new(&s.adapter_luid, s.target_id, &s.monitor_path));
                }
            }
            let block = if !recovery_ready {
                Some("恢复进程未就绪。".into())
            } else if !hotkey_registered {
                Some("紧急热键不可用。".into())
            } else {
                Gate::screen_keep_off_block_reason(snapshot, &row.identity(), &already, bundled_vdd_installed)
            };
            let can_keep_off = wanted != "保持关闭" && block.is_none();
            let can_restore = wanted == "保持关闭";
            items.push(ScreenItem {
                identity: row.identity(),
                name: row.display_name(),
                kind: if row.role == PathRole::Internal {
                    "内置".into()
                } else {
                    "外接".into()
                },
                wanted: wanted.clone(),
                confirmed: confirmed.clone(),
                can_keep_off,
                can_restore,
                status_text: format!("{confirmed}（{wanted}）"),
                block_reason: block.unwrap_or_else(|| hb.map(|s| s.detail.clone()).unwrap_or_default()),
            });
        }
        if let Some(h) = heartbeat {
            for hb_row in &h.screens {
                let id = ScreenIdentity::new(&hb_row.adapter_luid, hb_row.target_id, &hb_row.monitor_path);
                if items.iter().any(|i| i.identity.matches(&id)) {
                    continue;
                }
                if hb_row.wanted != "保持关闭" && hb_row.confirmed != "已关闭" {
                    continue;
                }
                items.push(ScreenItem {
                    identity: id,
                    name: if hb_row.name.trim().is_empty() {
                        "已关闭的物理屏".into()
                    } else {
                        hb_row.name.clone()
                    },
                    kind: "物理".into(),
                    wanted: hb_row.wanted.clone(),
                    confirmed: hb_row.confirmed.clone(),
                    can_keep_off: false,
                    can_restore: hb_row.wanted == "保持关闭" || hb_row.confirmed == "已关闭",
                    status_text: format!("{}（{}）", hb_row.confirmed, hb_row.wanted),
                    block_reason: hb_row.detail.clone(),
                });
            }
        }
        items
    }
}
