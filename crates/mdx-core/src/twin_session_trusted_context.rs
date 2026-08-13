use crate::TrustedContextSourceSummary;

pub(crate) fn local_created_at(sequence: usize) -> String {
    format!("2026-01-01T00:00:{:02}Z", sequence % 60)
}

pub(crate) fn trusted_context_sources_joined(
    sources: &[TrustedContextSourceSummary],
    field: impl Fn(&TrustedContextSourceSummary) -> &String,
) -> String {
    sources
        .iter()
        .map(field)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
