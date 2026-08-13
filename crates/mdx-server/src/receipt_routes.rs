use mdx_core::{MdxKernel, Receipt, json_string_literal};

pub(crate) fn render_receipts_json(kernel: &MdxKernel) -> String {
    let query = kernel.ledger().query();
    let receipt_ids = query
        .receipt_ids()
        .iter()
        .map(|receipt_id| json_string_literal(receipt_id))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"{{
  "count": {},
  "receipt_ids": [{}]
}}"#,
        query.count(),
        receipt_ids
    )
}

pub(crate) fn render_receipt_json(receipt: &Receipt) -> String {
    format!(
        r#"{{
  "receipt_id": {},
  "tenant_id": {},
  "trace_id": {},
  "actor_id": {},
  "loop_id": {},
  "workflow_id": {},
  "kind": {},
  "receipt_timestamp": {},
  "hash_version": {},
  "hash": {}
}}"#,
        json_string_literal(&receipt.receipt_id),
        json_string_literal(receipt.tenant_id.as_str()),
        json_string_literal(receipt.trace_id.as_str()),
        json_string_literal(receipt.actor_id.as_str()),
        json_string_literal(receipt.loop_id.as_str()),
        json_string_literal(receipt.workflow_id.as_str()),
        json_string_literal(&receipt.kind),
        json_string_literal(&receipt.receipt_timestamp),
        receipt.hash_version,
        json_string_literal(&receipt.hash)
    )
}
