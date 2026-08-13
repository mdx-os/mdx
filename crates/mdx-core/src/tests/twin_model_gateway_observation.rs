use super::*;

fn openai_observation() -> TwinModelGatewayProviderObservation<'static> {
    TwinModelGatewayProviderObservation {
        tenant_id: "tenant_local",
        actor_id: "local_operator",
        provider_id: "openai",
        adapter: "OpenAIResponsesModelGateway",
        receipt_kind: "openai.response.observed",
        approval_receipt_id: "human_approval_receipt_000001",
        evidence_file: ".mdx-local/provider-turn-on/openai-response-observed.json",
        model_id: "gpt-5-mini",
        response_id: "resp_000001",
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
        total_tokens: 12,
    }
}

#[test]
fn twin_model_gateway_provider_observation_accepts_receipt_safe_openai_metadata() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .save_twin_model_gateway_provider_observation_local(openai_observation())
        .expect("provider observation");

    assert_eq!(report.status, "TWIN_MODEL_GATEWAY_PROVIDER_OBSERVED");
    assert!(report.provider_connected_local_allowed);
    assert!(report.provider_call_allowed_now);
    assert!(report.live_network_allowed_now);
    assert!(!report.credential_values_recorded);
    assert!(!report.provider_secret_values_recorded);
    assert!(!report.output_text_recorded);
    assert!(!report.production_write_allowed);
    let receipt = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.observation_receipt_id)
        .expect("provider observation receipt");
    assert_eq!(receipt.kind, "twin.model_gateway.provider.observed");
    assert_eq!(receipt.payload["provider_id"], "openai");
    assert_eq!(receipt.payload["adapter"], "OpenAIResponsesModelGateway");
    assert_eq!(receipt.payload["provider_connected_local_allowed"], "true");
    assert_eq!(receipt.payload["credential_values_recorded"], "false");
}

#[test]
fn twin_model_gateway_provider_observation_rejects_secret_or_output_recording() {
    let mut kernel = MdxKernel::boot_local();
    let mut request = openai_observation();
    request.approval_receipt_id = "human_approval_receipt_000002";
    request.response_id = "resp_000002";
    request.credential_values_recorded = true;
    request.output_text_recorded = true;

    let report = kernel
        .save_twin_model_gateway_provider_observation_local(request)
        .expect("provider observation rejection retained");

    assert_eq!(
        report.status,
        "TWIN_MODEL_GATEWAY_PROVIDER_OBSERVATION_REJECTED"
    );
    assert!(!report.provider_connected_local_allowed);
    assert!(!report.provider_call_allowed_now);
    assert!(!report.live_network_allowed_now);
    assert_eq!(report.denial_reason, "secret_recording_rejected");
    let receipt = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.observation_receipt_id)
        .expect("provider observation receipt");
    assert_eq!(
        receipt.payload["terminal_state"],
        "TWIN_MODEL_GATEWAY_PROVIDER_OBSERVATION_REJECTED"
    );
    assert_eq!(receipt.payload["provider_connected_local_allowed"], "false");
    assert_eq!(receipt.payload["credential_values_recorded"], "false");
    assert_eq!(receipt.payload["output_text_recorded"], "false");
}
