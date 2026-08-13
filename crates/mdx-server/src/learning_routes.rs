use crate::RouteResponse;
use mdx_core::MdxKernel;
use std::sync::{Arc, RwLock};

#[path = "learning_adaptation_route.rs"]
pub(crate) mod adaptation;
#[path = "learning_candidate_rejection_route.rs"]
pub(crate) mod candidate_rejection;
#[path = "learning_evidence_request_route.rs"]
mod evidence_request;
#[path = "fixture_graduation_route.rs"]
mod fixture_graduation;
#[path = "learning_forge_outcome_candidates_route.rs"]
mod forge_outcome_candidates;
#[path = "learning_implicit_signal_route.rs"]
pub(crate) mod implicit_signal;
#[path = "learning_judgment_decision_route.rs"]
mod judgment_decision;
#[path = "learning_memory_activation_route.rs"]
mod memory_activation;
#[path = "learning_memory_promotion_route.rs"]
mod memory_promotion;
#[path = "learning_memory_supersede_route.rs"]
pub(crate) mod memory_supersede;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    evidence_request::route_response(method, path, body, kernel)
        .or_else(|| forge_outcome_candidates::route_response(method, path, kernel))
        .or_else(|| candidate_rejection::route_response(method, path, body, kernel))
        .or_else(|| implicit_signal::route_response(method, path, body, kernel))
        .or_else(|| judgment_decision::route_response(method, path, body, kernel))
        .or_else(|| memory_promotion::route_response(method, path, body, kernel))
        .or_else(|| memory_activation::route_response(method, path, body, kernel))
        .or_else(|| memory_supersede::route_response(method, path, body, kernel))
        .or_else(|| adaptation::route_response(method, path, body, kernel))
        .or_else(|| fixture_graduation::route_response(method, path, body, kernel))
}
