// The Forge recipe catalog over HTTP: the curated maintenance jobs that
// keep a repo healthy. A read of the static catalog - no kernel state, no
// authority. The UI renders these so the common work is one click; a
// recipe only prefills a run, it bypasses no gate.
use crate::RouteResponse;
use mdx_core::{FORGE_RECIPES, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    _kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/recipes.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Some(Ok(response));
    }
    let recipes: Vec<String> = FORGE_RECIPES
        .iter()
        .map(|recipe| {
            let commands = recipe
                .default_commands
                .iter()
                .map(|command| json_string_literal(command))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"id":{},"title":{},"category":{},"cadence":{},"summary":{},"intent":{},"default_commands":[{}]}}"#,
                json_string_literal(recipe.id),
                json_string_literal(recipe.title),
                json_string_literal(recipe.category),
                json_string_literal(recipe.cadence),
                json_string_literal(recipe.summary),
                json_string_literal(recipe.intent),
                commands,
            )
        })
        .collect();
    Some(Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-recipes","recipe_count":{},"recipes":[{}],"production_write_allowed":false}}"#,
            recipes.len(),
            recipes.join(","),
        ),
    )))
}
