// The prompt packs this binary actually composes from: embedded at compile
// time from the generated declaration, parsed once, and served back at
// /twin/prompt-packs.json so "what is Twin told" is one read away.
// Embedding makes the route's answer the RUNNING binary's truth - a newer
// repo file shows up only after a rebuild, which is exactly what an
// operator means by "active packs".
use crate::RouteResponse;
use std::sync::OnceLock;

const PACKS_JSON: &str = include_str!("../../../generated/twin/twin-prompt-packs.json");

fn packs() -> &'static serde_json::Value {
    static CELL: OnceLock<serde_json::Value> = OnceLock::new();
    CELL.get_or_init(|| serde_json::from_str(PACKS_JSON).expect("embedded prompt packs parse"))
}

fn pack(id: &str) -> Option<&'static serde_json::Value> {
    packs()["packs"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["id"] == id))
}

pub(crate) fn pack_text(id: &str) -> &'static str {
    pack(id)
        .and_then(|entry| entry["text"].as_str())
        .unwrap_or("")
}

// The disclosure tag for a composed layer: id plus the pack version, so
// provenance can say exactly which declaration shaped an answer.
pub(crate) fn pack_layer_tag(id: &str) -> String {
    let version = pack(id)
        .and_then(|entry| entry["version"].as_u64())
        .unwrap_or(0);
    format!("{id}@{version}")
}

pub(crate) fn route_response(method: &str, path: &str, _body: &str) -> Option<RouteResponse> {
    if path != "/twin/prompt-packs.json" {
        return None;
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        ));
    }
    Some(RouteResponse::json("200 OK", PACKS_JSON.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_packs_parse_and_core_layers_exist() {
        for id in [
            "foundation",
            "voice_and_style",
            "companion:twin_advisor",
            "companion:twin_architect",
            "companion:twin_coach",
            "companion:twin_compliance",
            "companion:twin_problem_solver",
            "companion_fallback",
            "stance_and_shape",
            "persona_frame",
            "model_identity",
        ] {
            assert!(!pack_text(id).is_empty(), "pack {id} missing or empty");
        }
    }

    #[test]
    fn no_pack_text_carries_an_em_dash() {
        // The voice layer bans em dashes; the declarations must obey their
        // own rule.
        assert!(!PACKS_JSON.contains('\u{2014}'));
    }
}
