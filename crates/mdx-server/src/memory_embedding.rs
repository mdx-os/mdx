// Sensitivity-locked local embedding for the memory read and write paths.
//
// Memory content is the most sensitive class, so its embeddings MUST come
// from a boundary-locked model: a loopback local server (Ollama's
// /v1/embeddings) or a provably-sovereign slot. If no such endpoint is
// configured this module returns None and the caller stores or ranks without
// an embedding - the recall ranker then falls back to the lexical and
// checksum components, exactly today's behavior. It NEVER falls through to a
// frontier provider, and a vector never leaves the box.
//
// Embedding happens here in mdx-server (where the model gateways live), never
// in the deterministic kernel. The kernel only stores the serialized vector.

use crate::forge_turn_client::TurnClient;
use mdx_core::{MdxKernel, StorageProvider};

pub(crate) const EMBEDDER_ROLE: &str = "memory_embedder";

/// The model allowed to embed this tenant's memory content: loopback or
/// provably-sovereign only. None means no qualifying local embedder exists
/// and the caller must skip embedding rather than reach for a frontier API.
pub(crate) fn resolve_embedder<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
) -> Option<TurnClient> {
    TurnClient::boundary_locked_for_role(kernel, tenant_id, EMBEDDER_ROLE)
}

/// Embed one piece of text with the resolved boundary-locked embedder, or
/// None when none is configured or the call fails. Fail-closed by design:
/// a provider error is not an error to the caller, just "no embedding".
pub(crate) fn embed_memory_text<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    text: &str,
) -> Option<Vec<f32>> {
    let client = resolve_embedder(kernel, tenant_id)?;
    embed_with(&client, text)
}

/// Embed with an already-resolved client (the write path resolves once, then
/// embeds several atoms without re-resolving).
pub(crate) fn embed_with(client: &TurnClient, text: &str) -> Option<Vec<f32>> {
    match client.embed(text) {
        Ok(vector) if !vector.is_empty() => Some(vector),
        Ok(_) => None,
        Err(error) => {
            eprintln!("memory embedder returned no vector, falling back to lexical: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_local_embedder_resolves_none_and_never_reaches_frontier() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // A fully configured FRONTIER builder and reviewer must not satisfy
        // the boundary-locked embedder resolution.
        let saved: Vec<(String, Option<String>)> = std::env::vars()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("MDX_FLEET_") || key == "MDX_DEFAULT_MODEL_PROVIDER")
            .map(|key| (key.clone(), std::env::var(&key).ok()))
            .collect();
        for (key, _) in &saved {
            unsafe { std::env::remove_var(key) };
        }
        unsafe {
            std::env::set_var("MDX_FLEET_BUILDER_BASE_URL", "https://api.x.ai/v1");
            std::env::set_var("MDX_FLEET_BUILDER_API_KEY", "test-key");
            std::env::set_var("MDX_FLEET_BUILDER_MODEL", "grok-4.3");
        }
        let kernel = MdxKernel::boot_local();
        assert!(resolve_embedder(&kernel, "local_tenant").is_none());
        assert!(embed_memory_text(&kernel, "local_tenant", "sensitive memory content").is_none());

        // A loopback embedder slot qualifies.
        unsafe {
            std::env::set_var(
                "MDX_FLEET_MEMORY_EMBEDDER_BASE_URL",
                "http://127.0.0.1:11434/v1",
            );
            std::env::set_var("MDX_FLEET_MEMORY_EMBEDDER_API_KEY", "local");
            std::env::set_var("MDX_FLEET_MEMORY_EMBEDDER_MODEL", "nomic-embed-text");
        }
        assert!(
            resolve_embedder(&kernel, "local_tenant").is_some(),
            "a loopback embedder must resolve"
        );
        unsafe {
            std::env::remove_var("MDX_FLEET_MEMORY_EMBEDDER_BASE_URL");
            std::env::remove_var("MDX_FLEET_MEMORY_EMBEDDER_API_KEY");
            std::env::remove_var("MDX_FLEET_MEMORY_EMBEDDER_MODEL");
            std::env::remove_var("MDX_FLEET_BUILDER_BASE_URL");
            std::env::remove_var("MDX_FLEET_BUILDER_API_KEY");
            std::env::remove_var("MDX_FLEET_BUILDER_MODEL");
        }
        for (key, value) in saved {
            match value {
                Some(value) => unsafe { std::env::set_var(&key, value) },
                None => unsafe { std::env::remove_var(&key) },
            }
        }
    }
}
