use mdx_core::{live_substrate_statuses, local_status_json, migration_report};

pub(crate) fn render_status_text() -> String {
    let report = migration_report();
    format!(
        "mdx-native local\nmigrations: {}\nmode: deterministic-local\n{}",
        report.migration_count,
        render_substrate_status()
    )
}

pub(crate) fn render_status_json() -> String {
    local_status_json(migration_report().migration_count)
}

fn render_substrate_status() -> String {
    let mut output = String::from("live_substrate:\n");
    for status in live_substrate_statuses() {
        output.push_str(&format!(
            "- {}: {} (local: {}, live: {}, turn_on: {})\n",
            status.substrate,
            status.status,
            status.local_adapter,
            status.live_adapter,
            status.turn_on_signal
        ));
    }
    output
}
