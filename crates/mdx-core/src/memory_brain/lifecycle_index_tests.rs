use super::*;

fn lifecycle_event(
    memory_id: String,
    event_id: String,
    lifecycle_state: &'static str,
) -> MemoryLifecycleEvent {
    MemoryLifecycleEvent {
        event_id,
        tenant_id: TenantId::new("tenant_local"),
        memory_id,
        action: lifecycle_state,
        lifecycle_state,
        reason: "lifecycle index regression fixture".to_string(),
        source_receipt_id: "receipt_seed".to_string(),
        valid_from_receipt_timestamp: "t".to_string(),
        receipt_id: "receipt_lifecycle".to_string(),
    }
}

#[test]
fn lifecycle_state_index_matches_reverse_scan_with_linear_event_work() {
    let record_count = 2_000usize;
    let memory_ids = (0..record_count)
        .map(|index| format!("memory_{index:04}"))
        .collect::<Vec<_>>();
    let mut events = memory_ids
        .iter()
        .enumerate()
        .map(|(index, memory_id)| {
            lifecycle_event(
                memory_id.clone(),
                format!("event_{index:04}"),
                if index.is_multiple_of(3) {
                    "stale"
                } else {
                    "active"
                },
            )
        })
        .collect::<Vec<_>>();
    // Replacement must preserve the existing last-event-wins rule.
    events.push(lifecycle_event(
        memory_ids[0].clone(),
        "event_latest".to_string(),
        "superseded",
    ));

    let naive_start = std::time::Instant::now();
    let mut naive_event_visits = 0usize;
    let naive_states = memory_ids
        .iter()
        .map(|memory_id| {
            events
                .iter()
                .rev()
                .find(|event| {
                    naive_event_visits += 1;
                    event.memory_id == *memory_id
                })
                .map(|event| event.lifecycle_state)
                .unwrap_or("active")
        })
        .collect::<Vec<_>>();
    let naive_elapsed = naive_start.elapsed();

    let index_start = std::time::Instant::now();
    let index = build_latest_memory_lifecycle_state_index(&events);
    let indexed_states = memory_ids
        .iter()
        .map(|memory_id| latest_memory_lifecycle_state_from_index(memory_id, &index))
        .collect::<Vec<_>>();
    let index_elapsed = index_start.elapsed();

    assert_eq!(indexed_states, naive_states);
    assert_eq!(indexed_states[0], "superseded");
    assert_eq!(
        latest_memory_lifecycle_state_from_index("memory_without_event", &index),
        "active"
    );
    let indexed_event_visits = events.len();
    assert!(
        naive_event_visits > indexed_event_visits * 500,
        "fixture must prove the old repeated scan does superlinear event work"
    );
    eprintln!(
        "lifecycle index: naive={naive_elapsed:?} indexed={index_elapsed:?} speedup={:.1}x visits={naive_event_visits}->{indexed_event_visits}",
        naive_elapsed.as_secs_f64() / index_elapsed.as_secs_f64()
    );
}
