use crate::capability::{DisplaySnapshot, PathRole, PathRow};
use crate::native::{
    CcdApi, CcdConstants, CcdFrame, DisplayConfigModeInfo, DisplayConfigPathInfo, Hotkey, Luid,
    MonotonicClock, ParentWatcher, PowerEvent, PowerObserver,
};
use crate::ScreenIdentity;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

pub fn path(
    internal_tech: bool,
    active: bool,
    id: u32,
    mode_idx: u32,
    adapter_low: u32,
) -> DisplayConfigPathInfo {
    let mut item = DisplayConfigPathInfo::default();
    item.flags = (if active {
        CcdConstants::DISPLAYCONFIG_PATH_ACTIVE
    } else {
        0
    }) | 8;
    item.target_info.output_technology = if internal_tech {
        CcdConstants::OUTPUT_TECHNOLOGY_INTERNAL
    } else {
        5
    };
    item.target_info.id = id;
    item.target_info.target_available = 1;
    item.target_info.adapter_id = Luid {
        low_part: adapter_low,
        high_part: 0,
    };
    item.source_info.id = id;
    item.source_info.adapter_id = item.target_info.adapter_id;
    item.source_info.mode_info_idx = mode_idx;
    item.target_info.mode_info_idx = 0x00020003;
    item
}

pub fn path_default(internal_tech: bool, id: u32) -> DisplayConfigPathInfo {
    path(internal_tech, true, id, 0x0001FFFF, 1)
}

pub fn id(target_id: u32, adapter: &str, monitor: &str) -> ScreenIdentity {
    ScreenIdentity::new(adapter, target_id, monitor)
}

pub fn row(
    role: PathRole,
    active: bool,
    target_id: u32,
    name: &str,
    adapter_path: &str,
    monitor_path: &str,
    adapter_luid: &str,
) -> PathRow {
    let internal_tech = role == PathRole::Internal;
    PathRow {
        index: target_id as i32,
        active,
        flags: if active {
            CcdConstants::DISPLAYCONFIG_PATH_ACTIVE
        } else {
            0
        },
        source_id: target_id,
        target_id,
        adapter_luid: adapter_luid.into(),
        output_technology: if internal_tech {
            CcdConstants::OUTPUT_TECHNOLOGY_INTERNAL
        } else {
            5
        },
        internal: internal_tech,
        source_name: format!(r"\\.\DISPLAY{target_id}"),
        adapter_path: adapter_path.into(),
        monitor_name: name.into(),
        monitor_path: monitor_path.into(),
        placeholder: monitor_path
            .to_ascii_uppercase()
            .contains("DEFAULT_MONITOR"),
        role,
        edid_manufacture_id: 0,
        edid_product_code_id: 0,
    }
}

pub fn row_simple(role: PathRole, target_id: u32, name: &str, monitor_path: &str) -> PathRow {
    row(
        role,
        true,
        target_id,
        name,
        r"PCI\VEN_8086",
        monitor_path,
        "0000000000000001",
    )
}

struct FakeCcdInner {
    paths: Vec<DisplayConfigPathInfo>,
    modes: Vec<DisplayConfigModeInfo>,
    rows: Vec<PathRow>,
    validate_rc: i32,
    apply_rc: i32,
    next_apply_rc: Option<i32>,
    mutate_on_failure: bool,
    clone_rc: i32,
    internal_rc: i32,
    flags: Vec<u32>,
    capture_error: Option<String>,
    stale_captures_remaining: i32,
    after_apply: Option<Box<dyn FnMut(&mut FakeCcdInner)>>,
    visible_paths: Vec<DisplayConfigPathInfo>,
    visible_rows: Vec<PathRow>,
    has_visible: bool,
}

pub struct FakeCcd {
    inner: RefCell<FakeCcdInner>,
}

impl FakeCcd {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(FakeCcdInner {
                paths: vec![],
                modes: vec![],
                rows: vec![],
                validate_rc: 0,
                apply_rc: 0,
                next_apply_rc: None,
                mutate_on_failure: false,
                clone_rc: 0,
                internal_rc: 0,
                flags: vec![],
                capture_error: None,
                stale_captures_remaining: 0,
                after_apply: None,
                visible_paths: vec![],
                visible_rows: vec![],
                has_visible: false,
            }),
        }
    }

    pub fn with_paths_rows(
        paths: Vec<DisplayConfigPathInfo>,
        modes: Vec<DisplayConfigModeInfo>,
        rows: Vec<PathRow>,
    ) -> Self {
        let ccd = Self::new();
        {
            let mut inner = ccd.inner.borrow_mut();
            inner.paths = paths;
            inner.modes = modes;
            inner.rows = rows;
        }
        ccd
    }

    pub fn set_validate_rc(&self, rc: i32) {
        self.inner.borrow_mut().validate_rc = rc;
    }
    pub fn set_apply_rc(&self, rc: i32) {
        self.inner.borrow_mut().apply_rc = rc;
    }
    pub fn set_internal_rc(&self, rc: i32) {
        self.inner.borrow_mut().internal_rc = rc;
    }
    pub fn set_clone_rc(&self, rc: i32) {
        self.inner.borrow_mut().clone_rc = rc;
    }
    pub fn mutate_on_failure(&self, value: bool) {
        self.inner.borrow_mut().mutate_on_failure = value;
    }
    pub fn set_next_apply_rc(&self, rc: i32) {
        self.inner.borrow_mut().next_apply_rc = Some(rc);
    }
    pub fn set_capture_error(&self, msg: impl Into<String>) {
        self.inner.borrow_mut().capture_error = Some(msg.into());
    }
    pub fn set_stale_captures(&self, n: i32) {
        self.inner.borrow_mut().stale_captures_remaining = n;
    }
    pub fn set_after_apply<F>(&self, mut f: F)
    where
        F: FnMut(&mut FakeCcdInnerView) + 'static,
    {
        self.inner.borrow_mut().after_apply = Some(Box::new(move |inner| {
            let mut view = FakeCcdInnerView {
                paths: &mut inner.paths,
                rows: &mut inner.rows,
            };
            f(&mut view);
        }));
    }
    pub fn clear_after_apply(&self) {
        self.inner.borrow_mut().after_apply = None;
    }
    pub fn set_paths_rows(&self, paths: Vec<DisplayConfigPathInfo>, rows: Vec<PathRow>) {
        let mut inner = self.inner.borrow_mut();
        inner.paths = paths;
        inner.rows = rows;
    }
    pub fn update_path_target(&self, index: usize, target_id: u32) {
        let mut inner = self.inner.borrow_mut();
        inner.paths[index].target_info.id = target_id;
        inner.rows[index].target_id = target_id;
    }
    pub fn deactivate_path(&self, index: usize) {
        let mut inner = self.inner.borrow_mut();
        inner.paths[index].flags = 8;
        inner.rows[index].active = false;
    }
    pub fn activate_path(&self, index: usize) {
        let mut inner = self.inner.borrow_mut();
        inner.paths[index].flags |= CcdConstants::DISPLAYCONFIG_PATH_ACTIVE;
        inner.rows[index].active = true;
    }
    pub fn flags(&self) -> Vec<u32> {
        self.inner.borrow().flags.clone()
    }
    pub fn applied(&self) -> bool {
        self.inner
            .borrow()
            .flags
            .iter()
            .any(|f| f & CcdConstants::SDC_APPLY != 0)
    }
    pub fn validated(&self) -> bool {
        self.inner
            .borrow()
            .flags
            .iter()
            .any(|f| f & CcdConstants::SDC_VALIDATE != 0)
    }
    pub fn paths(&self) -> Vec<DisplayConfigPathInfo> {
        self.inner.borrow().paths.clone()
    }
    pub fn modes(&self) -> Vec<DisplayConfigModeInfo> {
        self.inner.borrow().modes.clone()
    }
    pub fn rows(&self) -> Vec<PathRow> {
        self.inner.borrow().rows.clone()
    }
}

pub struct FakeCcdInnerView<'a> {
    pub paths: &'a mut Vec<DisplayConfigPathInfo>,
    pub rows: &'a mut Vec<PathRow>,
}

impl Default for FakeCcd {
    fn default() -> Self {
        Self::new()
    }
}

impl CcdApi for FakeCcd {
    fn query_raw(
        &self,
        _flags: u32,
    ) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String> {
        let inner = self.inner.borrow();
        Ok((inner.paths.clone(), inner.modes.clone()))
    }

    fn capture(&self, _flags: u32) -> Result<CcdFrame, String> {
        let mut inner = self.inner.borrow_mut();
        if let Some(err) = inner.capture_error.clone() {
            return Err(err);
        }
        if inner.stale_captures_remaining > 0 && inner.has_visible {
            inner.stale_captures_remaining -= 1;
            return Ok(CcdFrame {
                paths: inner.visible_paths.clone(),
                modes: inner.modes.clone(),
                snapshot: DisplaySnapshot::new(inner.visible_rows.clone(), 0),
            });
        }
        inner.visible_paths = inner.paths.clone();
        inner.visible_rows = inner.rows.clone();
        inner.has_visible = true;
        Ok(CcdFrame {
            paths: inner.paths.clone(),
            modes: inner.modes.clone(),
            snapshot: DisplaySnapshot::new(inner.rows.clone(), 0),
        })
    }

    fn set(
        &self,
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        flags: u32,
    ) -> Result<i32, String> {
        let mut inner = self.inner.borrow_mut();
        inner.flags.push(flags);
        if flags & CcdConstants::SDC_SAVE_TO_DATABASE != 0 {
            return Err("SAVE_TO_DATABASE".into());
        }
        if flags & CcdConstants::SDC_APPLY != 0 {
            let rc = inner.next_apply_rc.take().unwrap_or(inner.apply_rc);
            if rc != 0 && !inner.mutate_on_failure {
                return Ok(rc);
            }
            inner.paths = paths.to_vec();
            inner.modes = modes.to_vec();
            let applied_paths = inner.paths.clone();
            for row in &mut inner.rows {
                let path = applied_paths.iter().find(|p| {
                    p.target_info.id == row.target_id
                        && p.target_info.adapter_id.to_hex() == row.adapter_luid
                });
                row.active = path
                    .map(|p| p.flags & CcdConstants::DISPLAYCONFIG_PATH_ACTIVE != 0)
                    .unwrap_or(false);
                row.flags = path.map(|p| p.flags).unwrap_or(0);
            }
            if let Some(mut cb) = inner.after_apply.take() {
                cb(&mut inner);
                inner.after_apply = Some(cb);
            }
            return Ok(rc);
        }
        Ok(inner.validate_rc)
    }

    fn set_topology(&self, topology_flags: u32) -> Result<i32, String> {
        let mut inner = self.inner.borrow_mut();
        inner.flags.push(topology_flags);
        if topology_flags & CcdConstants::SDC_TOPOLOGY_CLONE != 0 {
            return Ok(inner.clone_rc);
        }
        Ok(inner.internal_rc)
    }
}

#[derive(Default)]
pub struct FakeHotkey {
    pub register_success: bool,
    pub pressed: bool,
    pub registered: bool,
    pub register_calls: i32,
}

impl FakeHotkey {
    pub fn new() -> Self {
        Self {
            register_success: true,
            ..Default::default()
        }
    }
}

impl Hotkey for FakeHotkey {
    fn try_register(&mut self) -> bool {
        self.register_calls += 1;
        self.registered = self.register_success;
        self.register_success
    }
    fn unregister(&mut self) {
        self.registered = false;
    }
    fn was_pressed(&mut self) -> bool {
        if !self.pressed {
            return false;
        }
        self.pressed = false;
        true
    }
}

#[derive(Default)]
pub struct FakeClock {
    pub seconds: f64,
}

impl MonotonicClock for FakeClock {
    fn seconds(&self) -> f64 {
        self.seconds
    }
}

pub struct SharedClock {
    pub seconds: RefCell<f64>,
}

impl SharedClock {
    pub fn new(v: f64) -> Self {
        Self {
            seconds: RefCell::new(v),
        }
    }
    pub fn set(&self, v: f64) {
        *self.seconds.borrow_mut() = v;
    }
}

impl MonotonicClock for SharedClock {
    fn seconds(&self) -> f64 {
        *self.seconds.borrow()
    }
}

#[derive(Default)]
pub struct FakeParent {
    pub alive: bool,
    pub alive_error: Option<String>,
}

impl FakeParent {
    pub fn new() -> Self {
        Self {
            alive: true,
            alive_error: None,
        }
    }
}

impl ParentWatcher for FakeParent {
    fn is_alive(&self, _pid: i32) -> Result<bool, String> {
        if let Some(err) = &self.alive_error {
            return Err(err.clone());
        }
        Ok(self.alive)
    }
}

pub struct SharedParent {
    pub alive: RefCell<bool>,
    pub error: RefCell<Option<String>>,
}

impl SharedParent {
    pub fn new() -> Self {
        Self {
            alive: RefCell::new(true),
            error: RefCell::new(None),
        }
    }
}

impl ParentWatcher for SharedParent {
    fn is_alive(&self, _pid: i32) -> Result<bool, String> {
        if let Some(err) = self.error.borrow().clone() {
            return Err(err);
        }
        Ok(*self.alive.borrow())
    }
}

#[derive(Default)]
pub struct FakePower {
    pub events: RefCell<VecDeque<PowerEvent>>,
}

impl FakePower {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: PowerEvent) {
        self.events.borrow_mut().push_back(event);
    }
}

impl PowerObserver for FakePower {
    fn poll(&mut self) -> PowerEvent {
        self.events
            .borrow_mut()
            .pop_front()
            .unwrap_or(PowerEvent::None)
    }
}

impl PowerObserver for Rc<FakePower> {
    fn poll(&mut self) -> PowerEvent {
        self.events
            .borrow_mut()
            .pop_front()
            .unwrap_or(PowerEvent::None)
    }
}

impl CcdApi for Rc<FakeCcd> {
    fn query_raw(
        &self,
        flags: u32,
    ) -> Result<(Vec<DisplayConfigPathInfo>, Vec<DisplayConfigModeInfo>), String> {
        (**self).query_raw(flags)
    }
    fn capture(&self, flags: u32) -> Result<CcdFrame, String> {
        (**self).capture(flags)
    }
    fn set(
        &self,
        paths: &[DisplayConfigPathInfo],
        modes: &[DisplayConfigModeInfo],
        flags: u32,
    ) -> Result<i32, String> {
        (**self).set(paths, modes, flags)
    }
    fn set_topology(&self, topology_flags: u32) -> Result<i32, String> {
        (**self).set_topology(topology_flags)
    }
}

impl Hotkey for RefCell<FakeHotkey> {
    fn try_register(&mut self) -> bool {
        self.get_mut().try_register()
    }
    fn unregister(&mut self) {
        self.get_mut().unregister()
    }
    fn was_pressed(&mut self) -> bool {
        self.get_mut().was_pressed()
    }
}

pub struct RcHotkey(pub Rc<RefCell<FakeHotkey>>);

impl Hotkey for RcHotkey {
    fn try_register(&mut self) -> bool {
        self.0.borrow_mut().try_register()
    }
    fn unregister(&mut self) {
        self.0.borrow_mut().unregister()
    }
    fn was_pressed(&mut self) -> bool {
        self.0.borrow_mut().was_pressed()
    }
}

impl MonotonicClock for Rc<SharedClock> {
    fn seconds(&self) -> f64 {
        (**self).seconds()
    }
}

impl ParentWatcher for Rc<SharedParent> {
    fn is_alive(&self, pid: i32) -> Result<bool, String> {
        (**self).is_alive(pid)
    }
}
