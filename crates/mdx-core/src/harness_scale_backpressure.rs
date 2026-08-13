// Slice F: the scale and backpressure contract.
//
// A factory serving many engineers cannot admit unbounded concurrent worker
// builds; under load it must degrade SAFELY - bound concurrency, bound the
// queue, keep one tenant from starving the rest, reserve headroom for high
// priority work, and shed low priority work before it overloads rather than
// falling over. This module is the contract and the pure admission classifier
// only. It runs nothing and leases nothing; it decides admit, queue, or shed.
//
// Fail-closed under pressure: when capacity is uncertain the decision favors
// queue or shed over admit. It never admits past the configured ceiling.

/// The priority of a unit of work competing for a worker slot. Ordered:
/// Low < Normal < High.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkerPriority {
    Low,
    Normal,
    High,
}

impl WorkerPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

/// The standing limits the factory admits worker builds under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleBackpressurePolicy {
    /// The hard ceiling on concurrently running worker builds.
    pub max_concurrent_workers: u32,
    /// The hard ceiling on queued (accepted, not yet running) builds.
    pub max_queue_depth: u32,
    /// No single tenant may run more than this many builds at once - fairness so
    /// one tenant cannot starve the rest.
    pub per_tenant_max_concurrent: u32,
    /// Running slots kept in reserve for High priority work; below-High work
    /// will not consume the last reserved slots and is queued instead.
    pub high_priority_reserved_slots: u32,
    /// When the queue is at least this deep, Low priority work is shed rather
    /// than queued - backpressure that protects the queue for real work.
    pub low_priority_shed_queue_depth: u32,
}

impl ScaleBackpressurePolicy {
    fn is_valid(&self) -> Option<&'static str> {
        if self.max_concurrent_workers == 0 {
            return Some("invalid_policy:max_concurrent_zero");
        }
        if self.per_tenant_max_concurrent == 0 {
            return Some("invalid_policy:per_tenant_zero");
        }
        // Reserving more than (or all of) the slots would deadlock below-High work.
        if self.high_priority_reserved_slots >= self.max_concurrent_workers {
            return Some("invalid_policy:reserve_exceeds_capacity");
        }
        None
    }
}

/// The live counts an admission decision is made against - observed, not assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleAdmissionRequest<'a> {
    pub tenant_id: &'a str,
    pub priority: WorkerPriority,
    /// Worker builds running right now, across all tenants.
    pub global_running: u32,
    /// Builds queued right now, across all tenants.
    pub global_queued: u32,
    /// Builds running right now for this request's tenant.
    pub tenant_running: u32,
}

/// The admission decision. Admit runs now; Queue is accepted but waits; Shed is
/// backpressure - rejected now, with a reason, so the caller can retry later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScaleAdmissionDecision {
    Admit,
    Queue(String),
    Shed(String),
}

impl ScaleAdmissionDecision {
    pub fn is_admit(&self) -> bool {
        matches!(self, Self::Admit)
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Admit => None,
            Self::Queue(reason) | Self::Shed(reason) => Some(reason),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admit => "admit",
            Self::Queue(_) => "queue",
            Self::Shed(_) => "shed",
        }
    }
}

/// The pure admission classifier. Precedence: an invalid policy sheds; a tenant
/// at its concurrency cap queues (or sheds if the queue is also full); otherwise
/// admit when a non-reserved running slot is free; otherwise queue while the
/// queue has room (shedding Low priority once the queue is deep); otherwise shed.
pub fn classify_scale_admission(
    policy: &ScaleBackpressurePolicy,
    request: &ScaleAdmissionRequest<'_>,
) -> ScaleAdmissionDecision {
    if let Some(reason) = policy.is_valid() {
        return ScaleAdmissionDecision::Shed(reason.to_string());
    }
    let queue_has_room = request.global_queued < policy.max_queue_depth;

    // Fairness first: a tenant at its cap does not get a running slot, even if
    // global capacity exists. It waits in the queue, or sheds if the queue is full.
    if request.tenant_running >= policy.per_tenant_max_concurrent {
        if queue_has_room {
            return ScaleAdmissionDecision::Queue("tenant_concurrency_cap".to_string());
        }
        return ScaleAdmissionDecision::Shed("tenant_cap_and_queue_full".to_string());
    }

    // Global running capacity, keeping reserved headroom for High priority work.
    if request.global_running < policy.max_concurrent_workers {
        let free = policy.max_concurrent_workers - request.global_running;
        let reserved = policy.high_priority_reserved_slots;
        if request.priority < WorkerPriority::High && free <= reserved {
            // The only free slots are reserved for High priority work; queue.
            if queue_has_room {
                return ScaleAdmissionDecision::Queue("reserved_for_high_priority".to_string());
            }
            return ScaleAdmissionDecision::Shed(
                "reserved_for_high_priority_queue_full".to_string(),
            );
        }
        return ScaleAdmissionDecision::Admit;
    }

    // At running capacity: queue if there is room, with low-priority backpressure.
    if queue_has_room {
        if request.priority == WorkerPriority::Low
            && request.global_queued >= policy.low_priority_shed_queue_depth
        {
            return ScaleAdmissionDecision::Shed("backpressure_low_priority".to_string());
        }
        return ScaleAdmissionDecision::Queue("at_capacity_queued".to_string());
    }
    ScaleAdmissionDecision::Shed("queue_full".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ScaleBackpressurePolicy {
        ScaleBackpressurePolicy {
            max_concurrent_workers: 10,
            max_queue_depth: 20,
            per_tenant_max_concurrent: 4,
            high_priority_reserved_slots: 2,
            low_priority_shed_queue_depth: 10,
        }
    }

    fn request() -> ScaleAdmissionRequest<'static> {
        ScaleAdmissionRequest {
            tenant_id: "tenant_a",
            priority: WorkerPriority::Normal,
            global_running: 0,
            global_queued: 0,
            tenant_running: 0,
        }
    }

    #[test]
    fn admits_when_there_is_unreserved_running_capacity() {
        let decision = classify_scale_admission(&policy(), &request());
        assert_eq!(decision, ScaleAdmissionDecision::Admit);
        assert!(decision.is_admit());
    }

    #[test]
    fn a_tenant_at_its_cap_queues_then_sheds() {
        let p = policy();
        let mut at_cap = request();
        at_cap.tenant_running = 4;
        assert_eq!(
            classify_scale_admission(&p, &at_cap).reason(),
            Some("tenant_concurrency_cap")
        );
        // ...and sheds when the queue is also full.
        at_cap.global_queued = 20;
        assert_eq!(
            classify_scale_admission(&p, &at_cap),
            ScaleAdmissionDecision::Shed("tenant_cap_and_queue_full".to_string())
        );
    }

    #[test]
    fn below_high_priority_work_will_not_take_the_reserved_slots() {
        let p = policy();
        // 9 running, 1 free, 2 reserved for High -> Normal is queued.
        let mut tight = request();
        tight.global_running = 9;
        assert_eq!(
            classify_scale_admission(&p, &tight).reason(),
            Some("reserved_for_high_priority")
        );
        // High priority takes the reserved slot.
        tight.priority = WorkerPriority::High;
        assert_eq!(
            classify_scale_admission(&p, &tight),
            ScaleAdmissionDecision::Admit
        );
    }

    #[test]
    fn at_capacity_work_queues_until_the_queue_fills() {
        let p = policy();
        let mut full = request();
        full.global_running = 10;
        full.global_queued = 5;
        assert_eq!(
            classify_scale_admission(&p, &full).reason(),
            Some("at_capacity_queued")
        );
        full.global_queued = 20;
        assert_eq!(
            classify_scale_admission(&p, &full),
            ScaleAdmissionDecision::Shed("queue_full".to_string())
        );
    }

    #[test]
    fn low_priority_is_shed_under_queue_pressure_while_normal_still_queues() {
        let p = policy();
        let mut pressured = request();
        pressured.global_running = 10;
        pressured.global_queued = 12; // >= low_priority_shed_queue_depth (10)
        pressured.priority = WorkerPriority::Low;
        assert_eq!(
            classify_scale_admission(&p, &pressured),
            ScaleAdmissionDecision::Shed("backpressure_low_priority".to_string())
        );
        // Normal priority still queues under the same pressure.
        pressured.priority = WorkerPriority::Normal;
        assert_eq!(classify_scale_admission(&p, &pressured).as_str(), "queue");
    }

    #[test]
    fn an_invalid_policy_sheds_rather_than_admitting() {
        let mut bad = policy();
        bad.max_concurrent_workers = 0;
        assert_eq!(
            classify_scale_admission(&bad, &request()),
            ScaleAdmissionDecision::Shed("invalid_policy:max_concurrent_zero".to_string())
        );

        let mut reserve_too_big = policy();
        reserve_too_big.high_priority_reserved_slots = 10;
        assert_eq!(
            classify_scale_admission(&reserve_too_big, &request()).reason(),
            Some("invalid_policy:reserve_exceeds_capacity")
        );
    }
}
