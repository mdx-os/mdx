use mdx_core::local_loop_budgets;

pub(crate) fn enabled_loop_ids() -> String {
    local_loop_budgets()
        .iter()
        .map(|budget| budget.loop_id)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn parse_usize_arg(value: Option<&String>, name: &str) -> Result<usize, String> {
    value
        .ok_or_else(|| format!("{name} is required"))?
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))
}
