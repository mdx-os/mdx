use mdx_core::{
    ActorId, CorrelationIds, LocalTraceExporter, LoopId, ObservabilityAdapterError,
    OpenTelemetryExporter, TenantId, TraceEvent, TraceExporter, TraceId, WorkflowId,
};
use serde_json::{Value, json};
use std::{env, process};

fn main() {
    let command = env::args()
        .nth(1)
        .unwrap_or_else(|| "local-trace-proof".to_string());
    match command.as_str() {
        "local-trace-proof" => println!("{}", local_trace_proof()),
        "--help" | "-h" => {
            println!("usage: mdx-observability local-trace-proof");
        }
        other => {
            eprintln!("mdx-observability: unknown command {other}");
            process::exit(2);
        }
    }
}

fn local_trace_proof() -> Value {
    let correlation = CorrelationIds {
        tenant_id: TenantId::new("local_tenant"),
        trace_id: TraceId::new("trace_observability_local"),
        actor_id: ActorId::new("local_observability_actor"),
        loop_id: LoopId::new("observability_trace_proof_loop"),
        workflow_id: WorkflowId::new("observability_trace_proof_workflow"),
    };
    let mut exporter = LocalTraceExporter::default();
    for name in [
        "request.admitted",
        "policy.decision.recorded",
        "receipt.chain.linked",
    ] {
        if let Err(error) = exporter.export(TraceEvent::from_correlation(&correlation, name)) {
            return json!({
                "name": "mdx-local-observability-trace-proof",
                "mode": "deterministic-local",
                "status": "LOCAL_TRACE_EXPORTER_FAILED",
                "command": "cargo run -q -p mdx-observability -- local-trace-proof",
                "read_only": true,
                "writes_allowed": false,
                "network_allowed": false,
                "provider_call_allowed": false,
                "live_trace_export_allowed": false,
                "production_write_allowed": false,
                "failure": {
                    "stage": "local_trace_export",
                    "event_name": name,
                    "error": error.to_string()
                },
                "redaction": {
                    "secret_values_recorded": false,
                    "otel_endpoint_value_recorded": false,
                    "prompt_or_output_recorded": false,
                    "payload_values_recorded": false,
                    "telemetry_redaction_required": true
                }
            });
        }
    }

    let otel_endpoint_present = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let otel_result = if otel_endpoint_present {
        OpenTelemetryExporter::connect(Some("redacted-local-endpoint"))
    } else {
        OpenTelemetryExporter::connect(None)
    };
    let (otel_connect_status, otel_connect_error) = match otel_result {
        Ok(_) => (
            "UNEXPECTED_CONNECTED",
            "unexpected live exporter connection".to_string(),
        ),
        Err(ObservabilityAdapterError::MissingOtelEndpoint) => (
            "MISSING_ENDPOINT",
            "OTEL_EXPORTER_OTLP_ENDPOINT is required".to_string(),
        ),
        Err(ObservabilityAdapterError::PendingLiveRun { adapter, reason }) => {
            ("PENDING_LIVE_RUN", format!("{adapter}: {reason}"))
        }
    };

    json!({
        "name": "mdx-local-observability-trace-proof",
        "mode": "deterministic-local",
        "status": "LOCAL_TRACE_EXPORTER_READY_OTEL_BLOCKED",
        "command": "cargo run -q -p mdx-observability -- local-trace-proof",
        "read_only": true,
        "writes_allowed": false,
        "network_allowed": false,
        "provider_call_allowed": false,
        "live_trace_export_allowed": false,
        "production_write_allowed": false,
        "local_trace": {
            "exporter": exporter.exporter_name(),
            "event_count": exporter.events().len(),
            "correlation_fields": ["tenant_id", "trace_id", "actor_id", "loop_id", "workflow_id"],
            "events": exporter.events().iter().map(trace_event_json).collect::<Vec<_>>()
        },
        "redaction": {
            "secret_values_recorded": false,
            "otel_endpoint_value_recorded": false,
            "prompt_or_output_recorded": false,
            "payload_values_recorded": false,
            "telemetry_redaction_required": true
        },
        "otel_boundary": {
            "local_adapter": "LocalTraceExporter",
            "live_adapter": OpenTelemetryExporter::adapter_name(),
            "required_env": "OTEL_EXPORTER_OTLP_ENDPOINT",
            "env_present": otel_endpoint_present,
            "connect_status": otel_connect_status,
            "connect_error": otel_connect_error,
            "required_receipt_kind": "opentelemetry.trace.exported",
            "turn_on_signal": "live trace export observed through OpenTelemetryExporter",
            "requires_provider_turn_on_receipt": true,
            "requires_human_approval": true
        },
        "blocked_authority": {
            "live_trace_export_allowed": false,
            "network_allowed": false,
            "otel_endpoint_value_recorded": false,
            "secret_material_allowed": false,
            "production_authority_allowed": false,
            "production_write_allowed": false
        },
        "source_files": [
            "docs/adr/0014-observability-exporter-boundary.md",
            "generated/live-substrates/provider-turn-on-evidence.json",
            "generated/status/mdx-local-status.json"
        ]
    })
}

fn trace_event_json(event: &TraceEvent) -> Value {
    json!({
        "tenant_id": event.tenant_id.as_str(),
        "trace_id": event.trace_id.as_str(),
        "actor_id": event.actor_id.as_str(),
        "loop_id": event.loop_id.as_str(),
        "workflow_id": event.workflow_id.as_str(),
        "name": event.name.as_str(),
        "correlation_complete": true
    })
}
