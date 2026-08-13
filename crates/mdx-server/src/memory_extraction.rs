// Sensitivity-locked fact extraction for the memory write path.
//
// Surface writes used to store verbatim strings; this stage turns raw
// surface content into 1-3 short declarative fact atoms BEFORE deterministic
// adjudication in the kernel. The residency ruling is enforced here: Twin,
// Message, and Pages content is the most sensitive class, so extraction for
// those surfaces resolves ONLY local/sovereign endpoints (loopback or a
// provably-configured sovereign slot). If no such model exists the stage
// SKIPS - the content is stored verbatim and the consolidation receipt says
// "unadjudicated_verbatim" - it never falls through to a frontier provider.
// Forge build-request content (repo intents) may use the standard path.
//
// The model stage runs on a spawned thread with NO kernel lock held
// (record_run_outcome_signal's read/model/write shape), so Message send and
// Pages publish never block or fail on this stage.
use crate::forge_turn_client::TurnClient;
use mdx_core::MdxKernel;
use std::sync::{Arc, RwLock};

const MAX_FACT_ATOMS: usize = 3;

pub(crate) struct SurfaceMemoryIntake {
    pub(crate) tenant_id: String,
    pub(crate) actor_id: String,
    pub(crate) source_surface: &'static str,
    pub(crate) memory_scope: &'static str,
    pub(crate) source_receipt_id: String,
    pub(crate) content: String,
}

/// Consolidate one surface event into memory. When an extraction model is
/// admissible and configured, the extract-adjudicate-write chain runs on its
/// own thread; otherwise the verbatim fallback records synchronously, which
/// is also the deterministic path every test and model-less deployment gets.
pub(crate) fn consolidate_surface_event(
    kernel: &Arc<RwLock<MdxKernel>>,
    intake: SurfaceMemoryIntake,
) {
    // Unit tests always take the deterministic verbatim path: a test must
    // never depend on a live provider a dev machine happens to have
    // configured, and the spawned extraction thread would race the test's
    // own assertions. Resolution logic has its own direct tests below.
    let client = if cfg!(test) {
        None
    } else {
        extraction_client(kernel, &intake)
    };
    match client {
        None => record_verbatim(kernel, &intake),
        Some(client) => {
            let kernel = Arc::clone(kernel);
            std::thread::spawn(move || {
                extract_and_record(&kernel, &client, &intake);
                crate::kernel_snapshot::snapshot_at_boundary(&kernel);
            });
        }
    }
}

/// The model allowed to see this surface's content. Twin, Message, and
/// Pages memory text may ONLY meet a boundary-locked (local or sovereign)
/// endpoint; None here means skip extraction, never a frontier fallback.
fn extraction_client(
    kernel: &Arc<RwLock<MdxKernel>>,
    intake: &SurfaceMemoryIntake,
) -> Option<TurnClient> {
    let guard = kernel.read().ok()?;
    match intake.source_surface {
        "twin" | "messages" | "pages" => {
            TurnClient::boundary_locked_for_role(&guard, &intake.tenant_id, "memory_extractor")
        }
        _ => TurnClient::for_role(&guard, &intake.tenant_id, "memory_extractor")
            .or_else(|| TurnClient::for_role(&guard, &intake.tenant_id, "reviewer")),
    }
}

fn extract_and_record(
    kernel: &Arc<RwLock<MdxKernel>>,
    client: &TurnClient,
    intake: &SurfaceMemoryIntake,
) {
    let atoms = extract_fact_atoms(kernel, client, intake);
    match atoms {
        Some(atoms) if !atoms.is_empty() => {
            // Embed each atom with the boundary-locked local embedder BEFORE
            // taking the write lock (the embedder call is loopback HTTP). When
            // no embedder is configured the vector is None and the record
            // stays lexical - fail-closed to today's behavior.
            let embedder = {
                let Ok(guard) = kernel.read() else {
                    return;
                };
                crate::memory_embedding::resolve_embedder(&guard, &intake.tenant_id)
            };
            let embedded: Vec<(String, Option<Vec<f32>>)> = atoms
                .into_iter()
                .map(|atom| {
                    let vector = embedder
                        .as_ref()
                        .and_then(|client| crate::memory_embedding::embed_with(client, &atom));
                    (atom, vector)
                })
                .collect();
            let Ok(mut guard) = kernel.write() else {
                return;
            };
            for (atom, embedding) in embedded {
                match guard.consolidate_surface_memory(
                    &intake.tenant_id,
                    &intake.actor_id,
                    intake.source_surface,
                    intake.memory_scope,
                    &intake.source_receipt_id,
                    &atom,
                    "adjudicated_fact_atom",
                ) {
                    Ok(outcome) => {
                        if let Some(vector) = embedding
                            && !outcome.memory_id.is_empty()
                        {
                            guard.set_memory_embedding(&outcome.memory_id, vector);
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "memory extraction write for {} fell back: {error}",
                            intake.source_surface
                        );
                    }
                }
            }
        }
        // Extraction unavailable or refused by the guard: verbatim, honestly
        // stamped, nothing lost.
        _ => record_verbatim(kernel, intake),
    }
}

fn record_verbatim(kernel: &Arc<RwLock<MdxKernel>>, intake: &SurfaceMemoryIntake) {
    let embedding = {
        let Ok(guard) = kernel.read() else {
            return;
        };
        crate::memory_embedding::embed_memory_text(&guard, &intake.tenant_id, &intake.content)
    };
    let Ok(mut guard) = kernel.write() else {
        return;
    };
    match guard.consolidate_surface_memory(
        &intake.tenant_id,
        &intake.actor_id,
        intake.source_surface,
        intake.memory_scope,
        &intake.source_receipt_id,
        &intake.content,
        "unadjudicated_verbatim",
    ) {
        Ok(outcome) => {
            if let Some(vector) = embedding
                && !outcome.memory_id.is_empty()
            {
                guard.set_memory_embedding(&outcome.memory_id, vector);
            }
        }
        Err(error) => {
            eprintln!(
                "surface memory write for {} dropped: {error}",
                intake.source_surface
            );
        }
    }
}

/// The model stage, run with no kernel lock held. Returns None on any
/// provider error so the caller stores verbatim.
fn extract_fact_atoms(
    kernel: &Arc<RwLock<MdxKernel>>,
    client: &TurnClient,
    intake: &SurfaceMemoryIntake,
) -> Option<Vec<String>> {
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You extract durable facts from one workspace event so future work can recall them. Output 1 to 3 short declarative sentences, one per line, each a single fact worth remembering. State only what the content shows; never invent names, ids, or outcomes. No preamble, no bullets, no em dashes or en dashes."
        }),
        serde_json::json!({
            "role": "user",
            "content": format!(
                "Surface: {}\nSource receipt: {}\nContent:\n{}",
                intake.source_surface, intake.source_receipt_id, intake.content
            )
        }),
    ];
    let turn = client.complete(&messages).ok()?;
    let atoms: Vec<String> = turn
        .text
        .lines()
        .map(|line| line.trim().trim_start_matches(['-', '*', ' ']).to_string())
        .filter(|line| !line.is_empty())
        .take(MAX_FACT_ATOMS)
        .collect();
    let guard = kernel.read().ok()?;
    atoms
        .iter()
        .all(|atom| fact_atom_admissible(&guard, atom, &intake.content))
        .then_some(atoms)
}

/// Guard on extracted atoms: an atom longer than the original content is
/// not a distillation, and a cited receipt id must exist in the ledger.
fn fact_atom_admissible(kernel: &MdxKernel, atom: &str, original: &str) -> bool {
    if atom.is_empty() || atom.chars().count() > original.chars().count() {
        return false;
    }
    if atom.contains('\u{2014}') || atom.contains('\u{2013}') {
        return false;
    }
    atom.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| token.contains("receipt_"))
        .all(|token| kernel.ledger().query().by_id(token).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_with_source() -> (Arc<RwLock<MdxKernel>>, String) {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("loop run");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();
        (Arc::new(RwLock::new(kernel)), source_receipt_id)
    }

    fn clean_extraction_env() -> Vec<(String, Option<String>)> {
        // Remove every var that could resolve an extraction endpoint so the
        // test is hermetic on machines that run local models.
        let mut saved = Vec::new();
        let vars: Vec<String> = std::env::vars().map(|(key, _)| key).collect();
        for key in vars {
            if key.starts_with("MDX_FLEET_") || key == "MDX_DEFAULT_MODEL_PROVIDER" {
                saved.push((key.clone(), std::env::var(&key).ok()));
                unsafe { std::env::remove_var(&key) };
            }
        }
        saved
    }

    fn restore_env(saved: Vec<(String, Option<String>)>) {
        for (key, value) in saved {
            match value {
                Some(value) => unsafe { std::env::set_var(&key, value) },
                None => unsafe { std::env::remove_var(&key) },
            }
        }
    }

    #[test]
    fn no_local_model_stores_verbatim_stamped_unadjudicated_and_never_fails() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = clean_extraction_env();
        let (kernel, source_receipt_id) = kernel_with_source();
        consolidate_surface_event(
            &kernel,
            SurfaceMemoryIntake {
                tenant_id: "local_tenant".to_string(),
                actor_id: "human:local_user".to_string(),
                source_surface: "messages",
                memory_scope: "team_memory",
                source_receipt_id,
                content: "We agreed to ship the beta gate before load tests".to_string(),
            },
        );
        restore_env(saved);
        let guard = kernel.read().expect("kernel");
        let record = guard
            .memory_records()
            .iter()
            .find(|record| record.content.contains("beta gate before load tests"))
            .expect("verbatim record stored synchronously when no model qualifies");
        assert_eq!(record.consolidation_decision.as_str(), "RETAIN");
        let gate = guard
            .ledger()
            .query()
            .by_id(&record.provenance.gate_receipt_id)
            .expect("gate receipt");
        assert_eq!(
            gate.payload.get("adjudication_state").map(String::as_str),
            Some("unadjudicated_verbatim")
        );
    }

    #[test]
    fn sensitive_surfaces_never_resolve_a_frontier_extraction_client() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = clean_extraction_env();
        // A fully configured FRONTIER default builder and reviewer role.
        unsafe {
            std::env::set_var("MDX_FLEET_BUILDER_BASE_URL", "https://api.x.ai/v1");
            std::env::set_var("MDX_FLEET_BUILDER_API_KEY", "test-key");
            std::env::set_var("MDX_FLEET_BUILDER_MODEL", "grok-4.3");
            std::env::set_var("MDX_FLEET_REVIEWER_BASE_URL", "https://api.openai.com/v1");
            std::env::set_var("MDX_FLEET_REVIEWER_API_KEY", "test-key");
            std::env::set_var("MDX_FLEET_REVIEWER_MODEL", "gpt-5");
        }
        let (kernel, source_receipt_id) = kernel_with_source();
        for surface in ["twin", "messages", "pages"] {
            let intake = SurfaceMemoryIntake {
                tenant_id: "local_tenant".to_string(),
                actor_id: "human:local_user".to_string(),
                source_surface: surface,
                memory_scope: "team_memory",
                source_receipt_id: source_receipt_id.clone(),
                content: "sensitive content".to_string(),
            };
            assert!(
                extraction_client(&kernel, &intake).is_none(),
                "{surface} must not extract through a frontier provider"
            );
        }
        // A loopback endpoint for the extractor role qualifies.
        unsafe {
            std::env::set_var(
                "MDX_FLEET_MEMORY_EXTRACTOR_BASE_URL",
                "http://127.0.0.1:11434/v1",
            );
            std::env::set_var("MDX_FLEET_MEMORY_EXTRACTOR_API_KEY", "local");
            std::env::set_var("MDX_FLEET_MEMORY_EXTRACTOR_MODEL", "qwen3:8b");
        }
        let intake = SurfaceMemoryIntake {
            tenant_id: "local_tenant".to_string(),
            actor_id: "human:local_user".to_string(),
            source_surface: "messages",
            memory_scope: "team_memory",
            source_receipt_id: source_receipt_id.clone(),
            content: "sensitive content".to_string(),
        };
        let client = extraction_client(&kernel, &intake).expect("loopback extractor resolves");
        assert_eq!(client.provider_family(), "ollama");
        unsafe {
            std::env::remove_var("MDX_FLEET_MEMORY_EXTRACTOR_BASE_URL");
            std::env::remove_var("MDX_FLEET_MEMORY_EXTRACTOR_API_KEY");
            std::env::remove_var("MDX_FLEET_MEMORY_EXTRACTOR_MODEL");
        }
        restore_env(saved);
    }

    #[test]
    fn atom_guard_rejects_expansions_and_fabricated_receipts() {
        let (kernel, source_receipt_id) = kernel_with_source();
        let guard = kernel.read().expect("kernel");
        assert!(fact_atom_admissible(
            &guard,
            "User prefers short answers",
            "User said: I prefer short answers in chat please"
        ));
        assert!(
            !fact_atom_admissible(
                &guard,
                "An atom that is somehow much longer than the original content it distills",
                "short original"
            ),
            "an atom longer than its source is not a distillation"
        );
        assert!(
            !fact_atom_admissible(
                &guard,
                "Proven by receipt_999999_fabricated",
                "Proven by receipt_999999_fabricated plus padding for length"
            ),
            "a fabricated receipt id must fail the guard"
        );
        assert!(fact_atom_admissible(
            &guard,
            &format!("Recorded under {source_receipt_id}"),
            &format!("Recorded under {source_receipt_id} with extra context words")
        ));
    }
}
