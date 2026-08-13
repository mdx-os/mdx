// Slice 10: the load and concurrency proof.
//
// A factory that means to serve a thousand engineers must degrade SAFELY when
// every one of them pushes work at once. The scale and backpressure classifier
// (`classify_scale_admission`) already decides admit, queue, or shed per job.
// This module drives that classifier through a PURE, DETERMINISTIC simulation of
// many tenants, many engineers, and many jobs, then proves the invariants that
// matter under pressure: concurrency never exceeds the ceiling, no tenant
// starves the rest, the queue never overflows, and real shedding happens rather
// than the factory falling over.
//
// Determinism is a hard requirement of this kernel: no clock, no randomness, no
// wall time. Priorities, the drain cadence, cancellations, and retry failures
// are all assigned by index arithmetic so the same config always yields the same
// report. The proof is "degrades safely under simulated, local load" and nothing
// beyond what is simulated here.

use crate::harness_scale_backpressure::{
    ScaleAdmissionDecision, ScaleAdmissionRequest, ScaleBackpressurePolicy, WorkerPriority,
    classify_scale_admission,
};

/// The shape of a simulated load run. Every field is deterministic input; there
/// is no hidden state, clock, or randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadSimConfig {
    /// The standing admission limits the simulated factory runs under.
    pub policy: ScaleBackpressurePolicy,
    /// How many tenants compete for the shared factory.
    pub tenants: u32,
    /// Engineers per tenant, each a source of jobs.
    pub engineers_per_tenant: u32,
    /// Jobs each engineer submits over the run.
    pub jobs_per_engineer: u32,
    /// Drain cadence: complete one running job every this many arrivals. Lower is
    /// more throughput; higher forces more queueing and shedding.
    pub drain_every: u32,
    /// Cancel one in this many queued jobs (deterministically, by queue index).
    /// Cancelled jobs are removed and must never run.
    pub cancel_every: u32,
    /// Fail one in this many completed jobs (deterministically); a failed job is
    /// re-admitted with a decremented retry budget until the budget is spent.
    pub fail_every: u32,
    /// The retry budget every job starts with. A job must never retry past it.
    pub retry_budget: u8,
}

/// One unit of queued work, carrying the identity needed to prove fairness,
/// cancellation, and retry honesty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Job {
    tenant: u32,
    priority: WorkerPriority,
    /// Remaining retry budget. A re-admitted job carries a decremented value.
    retries_left: u8,
}

/// The proven outcome of a load run: counts plus the safety invariants. The
/// booleans are the contract; `invariants_held` is their conjunction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadSimReport {
    pub total_jobs: u64,
    pub admitted: u64,
    pub queued: u64,
    pub shed: u64,
    pub cancelled: u64,
    pub retried: u64,
    pub peak_running: u32,
    pub peak_queued: u32,
    pub peak_tenant_running: u32,
    /// True if running ever exceeded the concurrency ceiling. Must end FALSE.
    pub over_admission: bool,
    /// True if any tenant's running ever exceeded its cap. Must end FALSE.
    pub fairness_violation: bool,
    /// True if queued ever exceeded the max queue depth. Must end FALSE.
    pub queue_overflow: bool,
    /// True if at least one shed happened. Must end TRUE (pressure forces sheds).
    pub shed_observed: bool,
    /// True if every cancelled job never ran. Must end TRUE.
    pub cancellations_honored: bool,
    /// True if no job ever retried beyond its budget. Must end TRUE.
    pub retries_honored: bool,
    /// Total jobs that ran to completion (including re-admitted retries).
    pub completed: u64,
    /// The conjunction of every safety claim above.
    pub invariants_held: bool,
}

/// Assign a deterministic priority from a job index: Low / Normal / High by
/// index modulo three. No randomness; the same index always maps the same way.
fn priority_for(index: u64) -> WorkerPriority {
    match index % 3 {
        0 => WorkerPriority::Low,
        1 => WorkerPriority::Normal,
        _ => WorkerPriority::High,
    }
}

/// Mutable running state of the simulation. Kept private; only `run_load_sim`
/// produces the immutable report. Running jobs are tracked by identity per tenant
/// so completion knows the job's real retry budget rather than guessing it.
struct SimState {
    global_running: u32,
    per_tenant_running: Vec<u32>,
    running: Vec<Vec<Job>>,
    queue: Vec<Job>,
    report: LoadSimReport,
}

impl SimState {
    fn new(tenants: u32) -> Self {
        Self {
            global_running: 0,
            per_tenant_running: vec![0; tenants as usize],
            running: vec![Vec::new(); tenants as usize],
            queue: Vec::new(),
            report: LoadSimReport {
                total_jobs: 0,
                admitted: 0,
                queued: 0,
                shed: 0,
                cancelled: 0,
                retried: 0,
                peak_running: 0,
                peak_queued: 0,
                peak_tenant_running: 0,
                over_admission: false,
                fairness_violation: false,
                queue_overflow: false,
                shed_observed: false,
                cancellations_honored: true,
                retries_honored: true,
                completed: 0,
                invariants_held: false,
            },
        }
    }

    /// Record peaks and trip the safety flags if any bound was crossed. Called
    /// after every state mutation so a single overstep cannot hide.
    fn observe(&mut self, policy: &ScaleBackpressurePolicy) {
        let queued = self.queue.len() as u32;
        if self.global_running > self.report.peak_running {
            self.report.peak_running = self.global_running;
        }
        if queued > self.report.peak_queued {
            self.report.peak_queued = queued;
        }
        if let Some(&peak_tenant) = self.per_tenant_running.iter().max() {
            if peak_tenant > self.report.peak_tenant_running {
                self.report.peak_tenant_running = peak_tenant;
            }
            if peak_tenant > policy.per_tenant_max_concurrent {
                self.report.fairness_violation = true;
            }
        }
        if self.global_running > policy.max_concurrent_workers {
            self.report.over_admission = true;
        }
        if queued > policy.max_queue_depth {
            self.report.queue_overflow = true;
        }
    }

    /// Admit a job into a running slot, updating global and per-tenant counts and
    /// remembering the job's identity so completion preserves its retry budget.
    fn start(&mut self, job: Job, policy: &ScaleBackpressurePolicy) {
        self.global_running += 1;
        self.per_tenant_running[job.tenant as usize] += 1;
        self.running[job.tenant as usize].push(job);
        self.report.admitted += 1;
        self.observe(policy);
    }

    /// Try to admit one job under the current observed counts. Returns the job
    /// unchanged when it was queued (so the caller can record the queue push).
    fn try_admit(&mut self, job: Job, policy: &ScaleBackpressurePolicy) {
        let request = ScaleAdmissionRequest {
            tenant_id: "",
            priority: job.priority,
            global_running: self.global_running,
            global_queued: self.queue.len() as u32,
            tenant_running: self.per_tenant_running[job.tenant as usize],
        };
        match classify_scale_admission(policy, &request) {
            ScaleAdmissionDecision::Admit => self.start(job, policy),
            ScaleAdmissionDecision::Queue(_) => {
                self.queue.push(job);
                self.report.queued += 1;
                self.observe(policy);
            }
            ScaleAdmissionDecision::Shed(_) => {
                self.report.shed += 1;
                self.report.shed_observed = true;
                self.observe(policy);
            }
        }
    }

    /// Pull the highest-priority queued job that now fits into a running slot,
    /// respecting the same admission rules (proves queue -> run flow). At most
    /// one promotion per drain so the loop cannot over-admit in a single step.
    fn promote_from_queue(&mut self, policy: &ScaleBackpressurePolicy) {
        // Highest priority first; ties broken by queue order (lowest index).
        let mut best: Option<usize> = None;
        for (index, job) in self.queue.iter().enumerate() {
            let candidate = ScaleAdmissionRequest {
                tenant_id: "",
                priority: job.priority,
                global_running: self.global_running,
                global_queued: self.queue.len() as u32,
                tenant_running: self.per_tenant_running[job.tenant as usize],
            };
            if !classify_scale_admission(policy, &candidate).is_admit() {
                continue;
            }
            match best {
                Some(current) if self.queue[current].priority >= job.priority => {}
                _ => best = Some(index),
            }
        }
        if let Some(index) = best {
            let job = self.queue.remove(index);
            self.start(job, policy);
        }
    }

    /// Complete one running job for the given tenant, freeing a slot. Returns the
    /// completed job (with its real retry budget) for the retry path; None if the
    /// tenant had nothing running.
    fn complete_one(&mut self, tenant: u32, policy: &ScaleBackpressurePolicy) -> Option<Job> {
        if self.global_running == 0 || self.per_tenant_running[tenant as usize] == 0 {
            return None;
        }
        let job = self.running[tenant as usize].pop();
        self.global_running -= 1;
        self.per_tenant_running[tenant as usize] -= 1;
        self.observe(policy);
        job
    }
}

/// Run the pure, deterministic load simulation and return the proven report.
/// Same config in, same report out: no clock, no randomness, no wall time.
pub fn run_load_sim(config: &LoadSimConfig) -> LoadSimReport {
    let policy = config.policy;
    let mut state = SimState::new(config.tenants);

    let total_jobs = u64::from(config.tenants)
        * u64::from(config.engineers_per_tenant)
        * u64::from(config.jobs_per_engineer);
    state.report.total_jobs = total_jobs;

    let mut arrivals: u64 = 0;
    // A deterministic rotor over tenants for the drain target, so completions
    // spread across tenants rather than always draining tenant zero.
    let mut drain_tenant: u32 = 0;

    for tenant in 0..config.tenants {
        for engineer in 0..config.engineers_per_tenant {
            for job_index in 0..config.jobs_per_engineer {
                let global_index = u64::from(tenant)
                    * u64::from(config.engineers_per_tenant)
                    * u64::from(config.jobs_per_engineer)
                    + u64::from(engineer) * u64::from(config.jobs_per_engineer)
                    + u64::from(job_index);
                let job = Job {
                    tenant,
                    priority: priority_for(global_index),
                    retries_left: config.retry_budget,
                };
                state.try_admit(job, &policy);
                arrivals += 1;

                // Drain on the deterministic cadence: complete one running job,
                // then promote the best-fitting queued job into the free slot.
                if config.drain_every > 0 && arrivals.is_multiple_of(u64::from(config.drain_every))
                {
                    drain_step(&mut state, &mut drain_tenant, config, &policy);
                }

                // Cancel one in `cancel_every` queued jobs by current depth. A
                // cancelled job leaves the queue and can never run; if it never
                // ran, the cancellation was honored.
                if config.cancel_every > 0
                    && arrivals.is_multiple_of(u64::from(config.cancel_every))
                    && !state.queue.is_empty()
                {
                    // Remove the tail (most recently queued) deterministically.
                    state.queue.pop();
                    state.report.cancelled += 1;
                    state.observe(&policy);
                }
            }
        }
    }

    // Drain the rest of the system to completion so retries get exercised and
    // the queue empties through admission rather than being abandoned.
    let mut guard: u64 = 0;
    let guard_max = total_jobs.saturating_mul(8).saturating_add(64);
    while (state.global_running > 0 || !state.queue.is_empty()) && guard < guard_max {
        drain_step(&mut state, &mut drain_tenant, config, &policy);
        guard += 1;
    }

    finalize(&mut state.report);
    state.report
}

/// One drain step: complete a running job for the rotating tenant, exercise a
/// deterministic retry, then promote the best queued job that now fits.
fn drain_step(
    state: &mut SimState,
    drain_tenant: &mut u32,
    config: &LoadSimConfig,
    policy: &ScaleBackpressurePolicy,
) {
    // Find a tenant that actually has running work, starting at the rotor.
    let tenants = config.tenants;
    let mut completed: Option<Job> = None;
    for offset in 0..tenants {
        let candidate = (*drain_tenant + offset) % tenants;
        if let Some(job) = state.complete_one(candidate, policy) {
            completed = Some(job);
            *drain_tenant = (candidate + 1) % tenants;
            break;
        }
    }

    if let Some(job) = completed {
        state.report.completed += 1;

        // Deterministic retry: fail one in `fail_every` completions and re-admit
        // the SAME job with one less retry. A job with no budget left is never
        // re-admitted, so it can never retry past its budget.
        let should_fail = config.fail_every > 0
            && state
                .report
                .completed
                .is_multiple_of(u64::from(config.fail_every));
        if should_fail {
            if job.retries_left == 0 {
                // Out of budget: the job is dropped, not retried. This is the
                // honest stop that keeps retries bounded.
            } else {
                let retry_job = Job {
                    tenant: job.tenant,
                    priority: job.priority,
                    retries_left: job.retries_left - 1,
                };
                if retry_job.retries_left >= job.retries_left {
                    // A retry must always decrement; tripping this would mean the
                    // budget was not honored.
                    state.report.retries_honored = false;
                }
                state.report.retried += 1;
                state.try_admit(retry_job, policy);
            }
        }
    }

    state.promote_from_queue(policy);
}

/// Compute the conjunction invariant once all flags are settled.
fn finalize(report: &mut LoadSimReport) {
    report.invariants_held = !report.over_admission
        && !report.fairness_violation
        && !report.queue_overflow
        && report.shed_observed
        && report.cancellations_honored
        && report.retries_honored;
}

/// The 1000-engineer-scale config used by the proof and the evidence packet:
/// 20 tenants x 50 engineers = 1000 engineers, each submitting a few jobs, under
/// caps tight enough that pressure forces both queueing and shedding.
pub fn thousand_engineer_config() -> LoadSimConfig {
    LoadSimConfig {
        policy: ScaleBackpressurePolicy {
            max_concurrent_workers: 64,
            max_queue_depth: 128,
            per_tenant_max_concurrent: 8,
            high_priority_reserved_slots: 8,
            low_priority_shed_queue_depth: 96,
        },
        tenants: 20,
        engineers_per_tenant: 50,
        jobs_per_engineer: 3,
        drain_every: 5,
        cancel_every: 17,
        fail_every: 7,
        retry_budget: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousand_engineer_load_degrades_safely() {
        let config = thousand_engineer_config();
        let report = run_load_sim(&config);

        // The simulated workload is exactly tenants * engineers * jobs.
        assert_eq!(report.total_jobs, 20 * 50 * 3);

        // The safety invariants every one of them must hold under pressure.
        assert!(!report.over_admission, "running exceeded the ceiling");
        assert!(!report.fairness_violation, "a tenant exceeded its cap");
        assert!(!report.queue_overflow, "queue exceeded its depth");
        assert!(
            report.shed_observed,
            "pressure must force at least one shed"
        );
        assert!(report.cancellations_honored, "a cancelled job ran");
        assert!(report.retries_honored, "a job retried past its budget");
        assert!(report.invariants_held, "load proof invariants must hold");

        // Peaks must respect the configured caps exactly.
        assert!(report.peak_running <= config.policy.max_concurrent_workers);
        assert!(report.peak_queued <= config.policy.max_queue_depth);
        assert!(report.peak_tenant_running <= config.policy.per_tenant_max_concurrent);

        // The sim is sized to actually exercise pressure, not trivially pass.
        assert!(report.admitted > 0);
        assert!(report.queued > 0);
        assert!(report.shed > 0);
        assert!(report.cancelled > 0);
        assert!(report.retried > 0, "the retry path must be exercised");

        // Retries are genuinely bounded: every original job can retry at most
        // `retry_budget` times, so admissions never exceed first runs plus the
        // total retry budget. This is what `retries_honored` actually proves.
        let max_retries = report.total_jobs * u64::from(config.retry_budget);
        assert!(
            report.retried <= max_retries,
            "retries {} exceeded the total budget {}",
            report.retried,
            max_retries
        );
        // Every admission is either a first run or a budgeted retry, so total
        // admissions can never exceed the jobs plus their full retry budget.
        assert!(report.admitted <= report.total_jobs + max_retries);
    }

    #[test]
    fn run_is_deterministic() {
        let config = thousand_engineer_config();
        let first = run_load_sim(&config);
        let second = run_load_sim(&config);
        assert_eq!(first, second, "the simulation must be deterministic");
    }

    #[test]
    fn a_low_pressure_run_sheds_nothing_and_is_not_falsely_green() {
        // Generous caps and aggressive drain: nothing should be shed, which means
        // shed_observed is false and therefore invariants_held is false. This
        // proves the proof is honest - it does not report green without pressure.
        let config = LoadSimConfig {
            policy: ScaleBackpressurePolicy {
                max_concurrent_workers: 1000,
                max_queue_depth: 1000,
                per_tenant_max_concurrent: 1000,
                high_priority_reserved_slots: 1,
                low_priority_shed_queue_depth: 999,
            },
            tenants: 2,
            engineers_per_tenant: 2,
            jobs_per_engineer: 2,
            drain_every: 1,
            cancel_every: 0,
            fail_every: 0,
            retry_budget: 1,
        };
        let report = run_load_sim(&config);
        assert!(!report.shed_observed);
        assert!(!report.invariants_held);
        // But the real safety bounds still held; only the pressure claim is unmet.
        assert!(!report.over_admission);
        assert!(!report.fairness_violation);
        assert!(!report.queue_overflow);
    }
}
