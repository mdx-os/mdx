use crate::RouteResponse;
use mdx_core::MdxKernel;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    _method: &str,
    _path: &str,
    _kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    None
}
