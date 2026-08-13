use super::*;

fn receipt_by_id<'a>(kernel: &'a MdxKernel, receipt_id: &str) -> &'a Receipt {
    kernel.ledger().query().by_id(receipt_id).expect("receipt")
}

#[test]
fn harness_run_manifest_admission_requires_plan_only_closed_boundary() {
    let mut manifest = valid_harness_run_manifest();
    assert!(admit_harness_run_manifest(&manifest).is_ok());

    manifest.trust_boundary.network_allowed = true;
    assert_eq!(
        admit_harness_run_manifest(&manifest),
        Err(
            HarnessRunManifestRejection::PlanOnlyRequiresClosedExecutionBoundary("network_allowed")
        )
    );
}

#[test]
fn local_harness_plan_runner_emits_plan_and_refuses_writes() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalHarnessPlanRunner;
    let manifest = valid_harness_run_manifest();

    let report = runner
        .run_plan_only(&mut kernel, &manifest)
        .expect("plan-only harness run");

    assert_eq!(report.manifest_run_id, "harness_manifest_001");
    assert_eq!(report.status, "PLAN_ONLY_COMPLETED");
    assert_eq!(report.plan_items.len(), 1);
    assert!(!report.plan_items[0].write_allowed);
    assert_eq!(report.policy_decision_ids.len(), 3);
    assert_eq!(report.receipts.len(), 6);
    assert_eq!(report.transcript.replay_mode, "audit_replay");
    assert_eq!(report.transcript.items().len(), 3);
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::Admission
    );
    assert_eq!(
        report.transcript.items()[2].receipt_id,
        report.write_refusal_receipt_id
    );
    assert_eq!(kernel.loop_transitions().len(), 3);
    assert_eq!(kernel.memory_records().len(), 0);
    assert_eq!(kernel.outbox_events().len(), 0);
    assert!(kernel.ledger().verify().is_ok());
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&report.write_refusal_receipt_id)
            .map(|receipt| receipt.kind.as_str())
            == Some("harness.write.refused")
    );
    let admission = receipt_by_id(&kernel, &report.admission_receipt_id);
    assert_eq!(
        admission.payload.get("enterprise_pack").map(String::as_str),
        Some("none")
    );
}

#[test]
fn harness_transcript_audit_replay_does_not_claim_exact_reexecution() {
    let mut kernel = MdxKernel::boot_local();
    let report = LocalHarnessPlanRunner
        .run_plan_only(&mut kernel, &valid_harness_run_manifest())
        .expect("plan-only harness run");

    let replay = report.transcript.audit_replay().expect("audit replay");

    assert_eq!(replay.status, "AUDIT_REPLAYED");
    assert_eq!(replay.replay_mode, "audit_replay");
    assert!(!replay.exact_reexecution);
    assert_eq!(replay.item_count, 3);
    assert_eq!(replay.receipt_ids[0], report.admission_receipt_id);
    assert_eq!(replay.receipt_ids[2], report.write_refusal_receipt_id);
}

#[test]
fn harness_transcript_rejects_empty_replay() {
    let transcript = HarnessRunTranscript::new("manifest_empty", "harness_run_empty");

    assert_eq!(
        transcript.audit_replay(),
        Err(HarnessTranscriptReplayError::EmptyTranscript)
    );
}

#[test]
fn local_harness_read_tool_caps_output_and_records_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.budget_policy.max_tool_calls = 1;
    let request = HarnessReadToolRequest {
        path: "docs/HARNESS-RUNWAY.md",
        contents: "abcdef",
        allowed_read_roots: &["docs/"],
        max_output_bytes: 4,
    };

    let report = LocalHarnessReadToolPlane
        .read_virtual_artifact(&mut kernel, &manifest, &request)
        .expect("safe read tool report");

    assert_eq!(report.status, "READ_ALLOWED");
    assert_eq!(report.output, "abcd");
    assert!(report.output_truncated);
    assert_eq!(report.output_bytes, 4);
    assert_eq!(report.denied_reason, None);
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ToolRead
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.read.allowed")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_read_tool_denies_blocked_paths_with_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.budget_policy.max_tool_calls = 1;
    let request = HarnessReadToolRequest {
        path: ".env",
        contents: "SECRET=value",
        allowed_read_roots: &[".env"],
        max_output_bytes: 1024,
    };

    let report = LocalHarnessReadToolPlane
        .read_virtual_artifact(&mut kernel, &manifest, &request)
        .expect("denied read tool report");

    assert_eq!(report.status, "READ_DENIED");
    assert_eq!(report.output, "");
    assert_eq!(report.denied_reason.as_deref(), Some("blocked_path"));
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ToolDenied
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.read.denied")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_search_and_list_tools_are_capped_and_receipted() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.budget_policy.max_tool_calls = 4;

    let search = LocalHarnessReadToolPlane
        .search_virtual_artifact(
            &mut kernel,
            &manifest,
            &HarnessSearchToolRequest {
                path: "docs/HARNESS-SAFE-TOOL-PLANE.md",
                contents: "alpha\nsafe tool plane\nsafe tool plane extended\nomega",
                query: "safe",
                allowed_read_roots: &["docs/"],
                max_matches: 1,
                max_output_bytes: 64,
            },
        )
        .expect("safe search report");
    assert_eq!(search.status, "SEARCH_ALLOWED");
    assert_eq!(search.matches.len(), 1);
    assert!(search.output_truncated);
    assert_eq!(
        search.transcript.items()[0].kind,
        HarnessRunItemKind::ToolSearch
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&search.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.search.allowed")
    );

    let list = LocalHarnessReadToolPlane
        .list_virtual_artifacts(
            &mut kernel,
            &manifest,
            &HarnessListToolRequest {
                path: "docs/",
                entries: &[
                    "HARNESS-RUNWAY.md",
                    "HARNESS-SAFE-TOOL-PLANE.md",
                    "MDX-HARNESS-SPEC.md",
                ],
                allowed_read_roots: &["docs/"],
                max_entries: 2,
                max_output_bytes: 128,
            },
        )
        .expect("safe list report");
    assert_eq!(list.status, "LIST_ALLOWED");
    assert_eq!(list.entries.len(), 2);
    assert!(list.output_truncated);
    assert_eq!(
        list.transcript.items()[0].kind,
        HarnessRunItemKind::ToolList
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&list.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.list.allowed")
    );
}

#[test]
fn local_harness_search_and_list_tools_deny_blocked_paths() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.budget_policy.max_tool_calls = 4;

    let search = LocalHarnessReadToolPlane
        .search_virtual_artifact(
            &mut kernel,
            &manifest,
            &HarnessSearchToolRequest {
                path: ".env",
                contents: "SECRET=value",
                query: "SECRET",
                allowed_read_roots: &[".env"],
                max_matches: 1,
                max_output_bytes: 64,
            },
        )
        .expect("denied safe search");
    assert_eq!(search.status, "SEARCH_DENIED");
    assert_eq!(search.denied_reason.as_deref(), Some("blocked_path"));
    assert!(search.matches.is_empty());
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&search.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.search.denied")
    );

    let list = LocalHarnessReadToolPlane
        .list_virtual_artifacts(
            &mut kernel,
            &manifest,
            &HarnessListToolRequest {
                path: ".mdx-local/",
                entries: &["evidence-index.json"],
                allowed_read_roots: &[".mdx-local/"],
                max_entries: 1,
                max_output_bytes: 64,
            },
        )
        .expect("denied safe list");
    assert_eq!(list.status, "LIST_DENIED");
    assert_eq!(list.denied_reason.as_deref(), Some("blocked_path"));
    assert!(list.entries.is_empty());
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&list.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.list.denied")
    );
}

#[test]
fn local_harness_patch_tool_mediates_bounded_patch_without_applying() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_patch_manifest();
    let request = HarnessPatchToolRequest {
        path: "crates/mdx-core/src/lib.rs",
        patch: "diff --git a/crates/mdx-core/src/lib.rs b/crates/mdx-core/src/lib.rs\n@@\n+safe patch proposal\n",
        max_patch_bytes: 512,
        max_output_bytes: 40,
    };

    let report = LocalHarnessReadToolPlane
        .mediate_virtual_patch(&mut kernel, &manifest, &request)
        .expect("safe patch report");

    assert_eq!(report.status, "PATCH_ALLOWED");
    assert_eq!(report.patch_bytes, request.patch.len());
    assert!(report.output_truncated);
    assert!(report.patch_preview.len() <= request.max_output_bytes);
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ToolPatch
    );
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("patch receipt");
    assert_eq!(receipt.kind, "harness.tool.patch.allowed");
    assert_eq!(
        receipt.payload.get("applied").map(String::as_str),
        Some("false")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_patch_tool_denies_blocked_paths_with_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_patch_manifest();
    let request = HarnessPatchToolRequest {
        path: ".env",
        patch: "diff --git a/.env b/.env\n@@\n+SECRET=value\n",
        max_patch_bytes: 512,
        max_output_bytes: 128,
    };

    let report = LocalHarnessReadToolPlane
        .mediate_virtual_patch(&mut kernel, &manifest, &request)
        .expect("denied patch report");

    assert_eq!(report.status, "PATCH_DENIED");
    assert_eq!(report.patch_preview, "");
    assert_eq!(report.denied_reason.as_deref(), Some("blocked_path"));
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ToolDenied
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .is_some_and(|receipt| receipt.kind == "harness.tool.patch.denied")
    );
    assert!(kernel.ledger().verify().is_ok());
}
