use crate::RouteResponse;
use mdx_core::MdxKernel;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    crate::message_thread_message::route_response(method, path, body, kernel)
        .or_else(|| crate::message_action::route_response(method, path, body, kernel))
        .or_else(|| crate::message_fanout_request::route_response(method, path, body, kernel))
        .or_else(|| crate::message_presence_request::route_response(method, path, body, kernel))
        .or_else(|| {
            crate::message_realtime_cutover_preflight::route_response(method, path, body, kernel)
        })
        .or_else(|| crate::message_relay_observation::route_response(method, path, body, kernel))
        .or_else(|| crate::message_realtime_readiness::route_response(method, path, kernel))
}
