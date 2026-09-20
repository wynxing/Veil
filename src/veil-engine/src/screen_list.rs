use crate::capability::{resolved_screen_name, BundledVddAvailability, Gate, KeepOffAction};
use crate::session::HeartbeatFile;
use crate::{DisplaySnapshot, ScreenIdentity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuxiliaryInstallItem {
    pub visible: bool,
    pub enabled: bool,
    pub label: String,
    pub hint: String,
}

impl AuxiliaryInstallItem {
    pub const LABEL: &'static str = "安装辅助输出";
    pub const MISSING_PAYLOAD: &'static str =
        "缺少已校验的辅助虚拟输出驱动包。请用安装器安装，或在仓库放入 installer/payload。";

    pub fn from_availability(bundled_vdd: impl Into<BundledVddAvailability>) -> Self {
        match bundled_vdd.into() {
            BundledVddAvailability::Installed => Self {
                visible: false,
                enabled: false,
                label: Self::LABEL.into(),
                hint: String::new(),
            },
            BundledVddAvailability::PayloadOnly => Self {
                visible: true,
                enabled: true,
                label: Self::LABEL.into(),
                hint: String::new(),
            },
            BundledVddAvailability::Absent => Self {
                visible: true,
                enabled: false,
                label: Self::LABEL.into(),
                hint: Self::MISSING_PAYLOAD.into(),
            },
        }
    }
}

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
        bundled_vdd: impl Into<crate::capability::BundledVddAvailability>,
        recovery_ready: bool,
        hotkey_registered: bool,
    ) -> Vec<ScreenItem> {
        let bundled_vdd = bundled_vdd.into();
        let mut items = Vec::new();
        for row in snapshot.physical_screens() {
            let hb = heartbeat.and_then(|h| {
                h.screens.iter().find(|s| {
                    row.identity().matches(&ScreenIdentity::new(
                        &s.adapter_luid,
                        s.target_id,
                        &s.monitor_path,
                    ))
                })
            });
            let wanted = hb.map(|s| s.wanted.clone()).unwrap_or_else(|| {
                if pending_wanted.iter().any(|id| id.matches(&row.identity())) {
                    "保持关闭".into()
                } else {
                    "开启".into()
                }
            });
            let mut confirmed = hb.map(|s| s.confirmed.clone()).unwrap_or_else(|| {
                if row.active {
                    "已显示".into()
                } else {
                    "未知".into()
                }
            });
            if confirmed == "失败" && hb.is_none() {
                confirmed = "失败".into();
            }
            let mut already = pending_wanted.to_vec();
            if let Some(h) = heartbeat {
                for s in h.screens.iter().filter(|s| s.wanted == "保持关闭") {
                    already.push(ScreenIdentity::new(
                        &s.adapter_luid,
                        s.target_id,
                        &s.monitor_path,
                    ));
                }
            }
            let plan = if !recovery_ready {
                None
            } else if !hotkey_registered {
                None
            } else {
                Some(Gate::screen_keep_off_plan(
                    snapshot,
                    &row.identity(),
                    &already,
                    bundled_vdd,
                ))
            };
            let hard_block = if !recovery_ready {
                Some("恢复进程未就绪。".into())
            } else if !hotkey_registered {
                Some("紧急热键不可用。".into())
            } else if plan
                .as_ref()
                .is_some_and(|p| p.action == KeepOffAction::Blocked)
            {
                plan.as_ref().and_then(|p| p.block_reason.clone())
            } else {
                None
            };
            let can_keep_off = wanted != "保持关闭" && hard_block.is_none();
            let can_restore = wanted == "保持关闭";
            let notice = hard_block.or_else(|| {
                if plan.as_ref().is_some_and(|p| {
                    matches!(
                        p.action,
                        KeepOffAction::EnableBundledVdd | KeepOffAction::InstallBundledVdd
                    )
                }) {
                    plan.and_then(|p| p.block_reason)
                } else {
                    hb.map(|s| s.detail.clone()).filter(|s| !s.is_empty())
                }
            });
            items.push(ScreenItem {
                identity: row.identity(),
                name: resolved_screen_name(
                    Some(&row.monitor_name),
                    Some(&row.source_name),
                    hb.map(|s| s.name.as_str()),
                    &row.monitor_path,
                    false,
                ),
                kind: row.kind_label().into(),
                wanted: wanted.clone(),
                confirmed: confirmed.clone(),
                can_keep_off,
                can_restore,
                status_text: format!("{confirmed}（{wanted}）"),
                block_reason: notice.unwrap_or_default(),
            });
        }
        if let Some(h) = heartbeat {
            for hb_row in &h.screens {
                let id = ScreenIdentity::new(
                    &hb_row.adapter_luid,
                    hb_row.target_id,
                    &hb_row.monitor_path,
                );
                if items.iter().any(|i| i.identity.matches(&id)) {
                    continue;
                }
                if hb_row.wanted != "保持关闭" && hb_row.confirmed != "已关闭" {
                    continue;
                }
                items.push(ScreenItem {
                    identity: id.clone(),
                    name: resolved_screen_name(
                        Some(&hb_row.name),
                        None,
                        None,
                        &hb_row.monitor_path,
                        true,
                    ),
                    kind: if hb_row.kind.trim().is_empty() {
                        "物理".into()
                    } else {
                        hb_row.kind.clone()
                    },
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
