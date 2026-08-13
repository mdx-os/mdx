// The server-side glue for execution-posture receipts: observe the real
// environment switches this process is running under and ask the kernel to
// record them when they changed. Called at boot and at the gates where
// posture matters (run start, live delivery), so every flip of a
// safety-relevant switch lands on the ledger exactly once, with the
// trigger that observed it. Deliberately NOT called at server boot: a
// boot receipt would land before the deterministic local seed and shift
// every seeded receipt id (the smoke matrix pins those ids).
use mdx_core::{ForgeExecutionPosture, MdxKernel};
use std::sync::{Arc, RwLock};

fn env_is(name: &str, value: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some(value)
}

pub(crate) fn current_posture() -> ForgeExecutionPosture {
    let host_exec = if !host_checks_enabled() {
        "container"
    } else {
        crate::forge_exec_sandbox::posture_label(crate::forge_exec_sandbox::host_exec_mode())
    };
    ForgeExecutionPosture {
        host_exec: host_exec.to_string(),
        source_host_live: env_is("MDX_FORGE_SOURCE_HOST_LIVE", "1"),
        run_start_enabled: !env_is("MDX_FORGE_RUN_START_ENABLED", "0"),
        fleet_resume: env_is("MDX_FLEET_RESUME", "1"),
        kernel_snapshot: env_is("MDX_KERNEL_SNAPSHOT", "1"),
    }
}

fn host_checks_enabled() -> bool {
    match std::env::var("MDX_FORGE_HOST_CHECKS") {
        Ok(value) => value == "1",
        Err(_) => !env_is("MDX_FORGE_DISABLE_HOST_CHECKS", "1"),
    }
}

/// Observe and record the posture if it changed. Never fails the caller:
/// posture recording is an audit duty, not a gate, so a poisoned lock is
/// swallowed rather than turning a delivery or run start into an error.
pub(crate) fn record_current_posture(kernel: &Arc<RwLock<MdxKernel>>, trigger: &str) {
    let posture = current_posture();
    if let Ok(mut kernel) = kernel.write() {
        let _ = kernel.record_forge_execution_posture(
            "local_tenant",
            "agent:forge_execution_posture",
            trigger,
            &posture,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_twice_with_an_unchanged_environment_mints_one_receipt() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        record_current_posture(&kernel, "run_start_first");
        record_current_posture(&kernel, "run_start");
        let kernel = kernel.read().expect("kernel");
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("forge.execution_posture.recorded")
                .len(),
            1,
            "an unchanged posture must not mint a second receipt"
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
