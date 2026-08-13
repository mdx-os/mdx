use super::{OwnedLocalMemory, recall_score};
use std::cmp::Ordering;

type ScoredMemory<'a> = (usize, usize, &'a OwnedLocalMemory);

/// Return the exact stable top-K ranking without sorting the discarded tail.
/// The input position is the final tie-breaker so this preserves `sort_by`
/// behavior even if two records carry the same score and memory ID.
pub(super) fn top_scored_memories<'a>(
    memories: impl IntoIterator<Item = &'a OwnedLocalMemory>,
    query: &str,
    limit: usize,
) -> Vec<(usize, &'a OwnedLocalMemory)> {
    let mut scored = memories
        .into_iter()
        .enumerate()
        .map(|(position, memory)| (recall_score(query, memory), position, memory))
        .collect::<Vec<_>>();
    truncate_and_sort_top_k_by(&mut scored, limit, compare_scored_memories);
    scored
        .into_iter()
        .map(|(score, _, memory)| (score, memory))
        .collect()
}

fn compare_scored_memories(left: &ScoredMemory<'_>, right: &ScoredMemory<'_>) -> Ordering {
    right
        .0
        .cmp(&left.0)
        .then_with(|| left.2.memory_id.cmp(&right.2.memory_id))
        .then_with(|| left.1.cmp(&right.1))
}

fn truncate_and_sort_top_k_by<T>(
    items: &mut Vec<T>,
    limit: usize,
    mut compare: impl FnMut(&T, &T) -> Ordering,
) {
    if limit == 0 {
        items.clear();
        return;
    }
    if items.len() > limit {
        items.select_nth_unstable_by(limit, &mut compare);
        items.truncate(limit);
    }
    items.sort_by(compare);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn memory(index: usize, memory_id: String) -> OwnedLocalMemory {
        OwnedLocalMemory {
            memory_id,
            tier: "working".to_string(),
            content: format!("memory content {index}"),
            source_receipt_id: format!("receipt_{index:05}"),
            importance: (index * 37) % 101,
            access_count: index % 7,
            usefulness_score_percent: 0,
            edge_weight_percent: 100,
            last_recalled_query: String::new(),
        }
    }

    #[test]
    fn bounded_selection_matches_full_stable_sort_with_less_comparison_work() {
        let record_count = 50_000usize;
        let limit = 10usize;
        let memories = (0..record_count)
            .map(|index| {
                memory(
                    index,
                    format!("memory_{:05}", (index * 7_919) % record_count),
                )
            })
            .collect::<Vec<_>>();
        let scored = memories
            .iter()
            .enumerate()
            .map(|(position, memory)| {
                (recall_score("memory content 123", memory), position, memory)
            })
            .collect::<Vec<_>>();

        let full_comparisons = Cell::new(0usize);
        let full_started = std::time::Instant::now();
        let mut full = scored.clone();
        full.sort_by(|left, right| {
            full_comparisons.set(full_comparisons.get() + 1);
            compare_scored_memories(left, right)
        });
        full.truncate(limit);
        let full_elapsed = full_started.elapsed();

        let bounded_comparisons = Cell::new(0usize);
        let bounded_started = std::time::Instant::now();
        let mut bounded = scored;
        truncate_and_sort_top_k_by(&mut bounded, limit, |left, right| {
            bounded_comparisons.set(bounded_comparisons.get() + 1);
            compare_scored_memories(left, right)
        });
        let bounded_elapsed = bounded_started.elapsed();

        let full_keys = full
            .iter()
            .map(|(score, position, memory)| (*score, *position, memory.memory_id.as_str()))
            .collect::<Vec<_>>();
        let bounded_keys = bounded
            .iter()
            .map(|(score, position, memory)| (*score, *position, memory.memory_id.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(bounded_keys, full_keys);
        assert!(
            bounded_comparisons.get() * 4 < full_comparisons.get(),
            "bounded selection should use at least 4x fewer comparisons"
        );
        eprintln!(
            "ctx top-k: full={full_elapsed:?}/{} comparisons bounded={bounded_elapsed:?}/{} comparisons",
            full_comparisons.get(),
            bounded_comparisons.get()
        );
    }

    #[test]
    fn top_k_preserves_stable_ties_and_limit_edges() {
        let first = memory(0, "duplicate".to_string());
        let mut second = first.clone();
        second.source_receipt_id = "receipt_00001".to_string();
        let mut third = first.clone();
        third.source_receipt_id = "receipt_00002".to_string();
        let memories = [first, second, third];
        let ranked = top_scored_memories(memories.iter(), "no lexical match", 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].1.source_receipt_id, "receipt_00000");
        assert_eq!(ranked[1].1.source_receipt_id, "receipt_00001");
        assert!(top_scored_memories(memories.iter(), "query", 0).is_empty());
        assert_eq!(
            top_scored_memories(memories.iter(), "query", usize::MAX).len(),
            memories.len()
        );
    }
}
