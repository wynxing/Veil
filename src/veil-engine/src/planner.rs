use crate::native::{CcdApi, CcdConstants};
use crate::topology::PathOps;
use crate::ScreenIdentity;

#[derive(Clone, Debug)]
pub struct ValidatePlanResult {
    pub rc: i32,
    pub disabled_count: i32,
    pub remaining_active: i32,
    pub adjusted_origin: bool,
    pub used_apply: bool,
}

impl ValidatePlanResult {
    pub fn ok(&self) -> bool {
        self.rc == 0 && self.disabled_count > 0 && self.remaining_active > 0 && !self.used_apply
    }
}

pub struct DisplayPlanner;

impl DisplayPlanner {
    pub fn validate_deactivate(
        ccd: &dyn CcdApi,
        selected: &[ScreenIdentity],
        adjust_origin: bool,
    ) -> Result<ValidatePlanResult, String> {
        let frame = ccd.capture(CcdConstants::QUERY_FLAGS)?;
        let mut identities: Vec<ScreenIdentity> =
            frame.snapshot.paths.iter().map(|p| p.identity()).collect();
        if identities.len() != frame.paths.len() {
            identities = frame
                .paths
                .iter()
                .map(|p| {
                    ScreenIdentity::new(p.target_info.adapter_id.to_hex(), p.target_info.id, "")
                })
                .collect();
        }
        let prepared = PathOps::deactivate(
            &frame.paths,
            &frame.modes,
            &identities,
            selected,
            adjust_origin,
        )?;
        if !prepared.can_apply() {
            return Ok(ValidatePlanResult {
                rc: CcdConstants::ERROR_SUCCESS,
                disabled_count: prepared.disabled_count,
                remaining_active: prepared.remaining_active,
                adjusted_origin: prepared.adjusted_origin,
                used_apply: false,
            });
        }
        let flags = CcdConstants::VALIDATE_FLAGS;
        if flags & CcdConstants::SDC_APPLY != 0 {
            return Err("VALIDATE wrapper must never include SDC_APPLY".into());
        }
        let rc = ccd.set(&prepared.paths, &prepared.modes, flags)?;
        Ok(ValidatePlanResult {
            rc,
            disabled_count: prepared.disabled_count,
            remaining_active: prepared.remaining_active,
            adjusted_origin: prepared.adjusted_origin,
            used_apply: false,
        })
    }
}
