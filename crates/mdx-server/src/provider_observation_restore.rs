// Durable provider standing: the turn-on ceremony's evidence files ARE
// the durable artifact (presence-only, human-approved, observed proof).
// A kernel restart should not orphan them - so serve boot re-registers
// each observed evidence file through the SAME acceptance gate the live
// rail uses. Nothing is trusted from disk that the slot contract would
// not accept fresh; a tampered or incomplete file is rejected exactly
// like a bad POST. The slot table stays the naming contract (evidence
// written by older script versions restores under the slot's adapter).
use mdx_core::{
    MdxKernel, StorageProvider, TwinModelGatewayProviderObservation,
    twin_model_gateway_provider_slots,
};

pub(crate) fn restore_provider_observations<S: StorageProvider>(kernel: &mut MdxKernel<S>) {
    let dir = std::path::Path::new(".mdx-local/provider-turn-on");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(evidence) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if evidence["observed"].as_bool() != Some(true) {
            continue;
        }
        let substrate = evidence["substrate"].as_str().unwrap_or("");
        let Some(slot) = twin_model_gateway_provider_slots()
            .iter()
            .find(|slot| slot.provider_id == substrate)
        else {
            continue;
        };
        let total_tokens = evidence["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize;
        let result = kernel.save_twin_model_gateway_provider_observation_local(
            TwinModelGatewayProviderObservation {
                tenant_id: "tenant_local",
                actor_id: "local_serve_boot",
                provider_id: slot.provider_id,
                adapter: slot.adapter,
                receipt_kind: slot.required_receipt_kind,
                approval_receipt_id: evidence["approval_receipt_id"].as_str().unwrap_or(""),
                evidence_file: &path.to_string_lossy(),
                model_id: evidence["model_id"].as_str().unwrap_or(""),
                response_id: evidence["response_id"].as_str().unwrap_or(""),
                response_status: "completed",
                observed: true,
                provider_call_attempted: true,
                network_call_attempted: true,
                credential_presence_only: true,
                credential_values_recorded: false,
                provider_secret_values_recorded: false,
                requested_secret_values_recorded: false,
                output_text_recorded: false,
                production_write_allowed: false,
                total_tokens,
            },
        );
        match result {
            Ok(report) if report.accepted => {
                println!(
                    "provider standing restored: {} ({}) from {}",
                    slot.provider_id,
                    evidence["model_id"].as_str().unwrap_or("?"),
                    path.display()
                );
            }
            Ok(report) => {
                eprintln!(
                    "provider evidence not restored ({}): {}",
                    slot.provider_id, report.denial_reason
                );
            }
            Err(error) => {
                eprintln!(
                    "provider evidence rejected ({}): {}",
                    slot.provider_id,
                    error.message()
                );
            }
        }
    }
}
