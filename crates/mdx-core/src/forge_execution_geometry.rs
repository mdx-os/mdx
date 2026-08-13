pub const DIRECT_RUN_MAX_WORKERS: u32 = 4;
pub const FLEET_GEOMETRY_MAX_WORKERS: u32 = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeExecutionGeometry {
    pub requested_workers: u32,
    pub effective_workers: u32,
    pub lane: &'static str,
    pub route: &'static str,
    pub reason: &'static str,
    pub fleet_required: bool,
}

impl ForgeExecutionGeometry {
    pub fn fleet_required_str(&self) -> &'static str {
        if self.fleet_required { "true" } else { "false" }
    }

    pub fn selection_group_kind(&self) -> &'static str {
        match self.lane {
            "bounded_parallel_exploration" => "parallel_candidates",
            "fleet_stream" | "fleet_required" => "fleet_streams",
            "fleet_integration" => "fleet_integration",
            "repair_loop" => "repair_candidates",
            "review_panel" => "reviewer_panel",
            _ => "single_worker",
        }
    }
}

pub fn forge_execution_geometry_for_width(
    requested_workers: u32,
) -> Result<ForgeExecutionGeometry, String> {
    if requested_workers == 0 {
        return Err("fleet_width must be at least 1 worker.".to_string());
    }
    if requested_workers > FLEET_GEOMETRY_MAX_WORKERS {
        return Err(format!(
            "fleet_width={} exceeds the current governed fleet ceiling of {} workers.",
            requested_workers, FLEET_GEOMETRY_MAX_WORKERS
        ));
    }
    if requested_workers > DIRECT_RUN_MAX_WORKERS {
        return Ok(ForgeExecutionGeometry {
            requested_workers,
            effective_workers: requested_workers,
            lane: "fleet_required",
            route: "/forge/fleet-plans.json",
            reason: "width_requires_governed_fleet_plan_or_checkpointed_mission",
            fleet_required: true,
        });
    }
    if requested_workers == 1 {
        return Ok(ForgeExecutionGeometry {
            requested_workers,
            effective_workers: 1,
            lane: "single_worker",
            route: "/forge/runs.json",
            reason: "direct_single_worker_run",
            fleet_required: false,
        });
    }
    Ok(ForgeExecutionGeometry {
        requested_workers,
        effective_workers: requested_workers,
        lane: "bounded_parallel_exploration",
        route: "/forge/runs.json",
        reason: "direct_run_width_recorded_for_bounded_parallel_execution",
        fleet_required: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_execution_geometry_routes_width_to_the_right_lane() {
        let single = forge_execution_geometry_for_width(1).expect("single");
        assert_eq!(single.lane, "single_worker");
        assert_eq!(single.route, "/forge/runs.json");
        assert_eq!(single.selection_group_kind(), "single_worker");
        assert!(!single.fleet_required);

        let bounded = forge_execution_geometry_for_width(4).expect("bounded");
        assert_eq!(bounded.lane, "bounded_parallel_exploration");
        assert_eq!(bounded.route, "/forge/runs.json");
        assert_eq!(bounded.selection_group_kind(), "parallel_candidates");
        assert!(!bounded.fleet_required);

        let wide = forge_execution_geometry_for_width(8).expect("wide");
        assert_eq!(wide.lane, "fleet_required");
        assert_eq!(wide.route, "/forge/fleet-plans.json");
        assert_eq!(wide.selection_group_kind(), "fleet_streams");
        assert!(wide.fleet_required);

        assert!(forge_execution_geometry_for_width(0).is_err());
        assert!(forge_execution_geometry_for_width(FLEET_GEOMETRY_MAX_WORKERS + 1).is_err());
    }
}
