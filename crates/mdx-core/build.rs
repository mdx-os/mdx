use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct LoopBudget {
    const_name: String,
    loop_id: String,
    max_runtime_ms: usize,
    max_receipts: usize,
    max_policy_decisions: usize,
    required_transitions: usize,
    max_outbox_events: usize,
    max_credentials: usize,
    max_memory_records: usize,
    max_charter_records: usize,
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    let loops_dir = repo_root.join("loops");
    let mut budgets = Vec::new();

    println!("cargo:rerun-if-changed={}", loops_dir.display());
    for entry in fs::read_dir(&loops_dir).expect("loops directory") {
        let entry = entry.expect("loop directory entry");
        if !entry
            .file_type()
            .expect("loop directory file type")
            .is_dir()
        {
            continue;
        }
        let path = entry.path().join("loop.yaml");
        println!("cargo:rerun-if-changed={}", path.display());
        budgets.push(parse_loop_budget(&path));
    }

    budgets.sort_by(|left, right| left.loop_id.cmp(&right.loop_id));

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    fs::write(
        out_dir.join("loop_budgets.rs"),
        render_loop_budgets(&budgets),
    )
    .expect("write loop budgets");
}

fn parse_loop_budget(path: &Path) -> LoopBudget {
    let content = fs::read_to_string(path).expect("read loop declaration");
    if !content.lines().any(|line| line.trim() == "budget:") {
        panic!("missing budget block in {}", path.display());
    }
    let loop_id = parse_scalar(&content, "id").expect("loop id");
    let const_name = format!(
        "{}_LOCAL_BUDGET",
        loop_id.trim_end_matches("_agent").to_ascii_uppercase()
    );
    LoopBudget {
        const_name,
        loop_id,
        max_runtime_ms: parse_budget_usize(&content, "max_runtime_ms"),
        max_receipts: parse_budget_usize(&content, "max_receipts"),
        max_policy_decisions: parse_budget_usize(&content, "max_policy_decisions"),
        required_transitions: parse_budget_usize(&content, "required_transitions"),
        max_outbox_events: parse_budget_usize(&content, "max_outbox_events"),
        max_credentials: parse_budget_usize(&content, "max_credentials"),
        max_memory_records: parse_budget_usize(&content, "max_memory_records"),
        max_charter_records: parse_budget_usize(&content, "max_charter_records"),
    }
}

fn parse_scalar(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}: ");
    content.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

fn parse_budget_usize(content: &str, key: &str) -> usize {
    let prefix = format!("  {key}: ");
    content
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("missing budget field {key}"))
        .trim()
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("invalid budget field {key}"))
}

fn render_loop_budgets(budgets: &[LoopBudget]) -> String {
    let constants = budgets
        .iter()
        .map(|budget| {
            format!(
                r#"pub const {const_name}: LocalLoopBudget = LocalLoopBudget {{
    loop_id: "{loop_id}",
    max_runtime_ms: {max_runtime_ms},
    max_receipts: {max_receipts},
    max_policy_decisions: {max_policy_decisions},
    required_transitions: {required_transitions},
    max_outbox_events: {max_outbox_events},
    max_credentials: {max_credentials},
    max_memory_records: {max_memory_records},
    max_charter_records: {max_charter_records},
}};"#,
                const_name = budget.const_name,
                loop_id = budget.loop_id,
                max_runtime_ms = budget.max_runtime_ms,
                max_receipts = budget.max_receipts,
                max_policy_decisions = budget.max_policy_decisions,
                required_transitions = budget.required_transitions,
                max_outbox_events = budget.max_outbox_events,
                max_credentials = budget.max_credentials,
                max_memory_records = budget.max_memory_records,
                max_charter_records = budget.max_charter_records
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let names = budgets
        .iter()
        .map(|budget| budget.const_name.as_str())
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        r#"{constants}

const LOCAL_LOOP_BUDGETS: [LocalLoopBudget; {count}] = [
    {names},
];

pub fn local_loop_budgets() -> &'static [LocalLoopBudget] {{
    &LOCAL_LOOP_BUDGETS
}}
"#,
        count = budgets.len()
    )
}
