//! Forge recipes: curated, recurring engineering jobs that keep a repo in
//! tip-top shape. A recipe is a named starting point - a careful task
//! prompt and the checks that prove it - so the common maintenance work
//! is one click, not a blank page.
//!
//! A recipe is ONLY a starting point. It prefills a run; the engineer
//! still picks the repo, reviews the diff, and records the ship decision.
//! No recipe bypasses a gate. The cadence is a hint (which jobs are meant
//! to run nightly or weekly) for the scheduling layer that comes later;
//! today every recipe launches on demand.

/// A maintenance recipe. The intent is the agent's task; default_commands
/// are the checks that prove it (Rust-shaped for MDx; an engineer edits
/// them per repo). The cadence signals how often the job is meant to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgeRecipe {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    /// "nightly" | "weekly" | "on_demand".
    pub cadence: &'static str,
    pub summary: &'static str,
    pub intent: &'static str,
    pub default_commands: &'static [&'static str],
}

/// The top ten jobs that keep a repo healthy. Ordered roughly by how often
/// a team runs them.
pub const FORGE_RECIPES: &[ForgeRecipe] = &[
    ForgeRecipe {
        id: "security_scan",
        title: "Security scan",
        category: "security",
        cadence: "nightly",
        summary: "Find vulnerable dependencies, leaked secrets, and unsafe patterns.",
        intent: "Run a security review of this repository. Look for: dependencies with known vulnerabilities, hardcoded secrets or credentials, unsafe deserialization, command or path injection, missing authorization on sensitive paths, and unsafe use of unsafe/eval. For each finding, fix the ones that are clearly safe to fix (a dependency bump, a secret moved to config, an obvious sanitization) and leave a clear note for anything that needs human judgment. Do not weaken any existing security check. Prove the build still passes.",
        default_commands: &["cargo build", "cargo test"],
    },
    ForgeRecipe {
        id: "dependency_upgrades",
        title: "Dependency upgrades",
        category: "dependencies",
        cadence: "weekly",
        summary: "Bump outdated dependencies that upgrade cleanly, and prove the build holds.",
        intent: "Find dependencies that are outdated and bump the ones that upgrade safely - patch and minor versions whose changelogs show no breaking changes for how this repo uses them. Update the lockfile. Leave anything with a major version or a risky changelog for human review with a one-line note. After each change, prove the build and tests still pass.",
        default_commands: &["cargo build", "cargo test"],
    },
    ForgeRecipe {
        id: "performance_pass",
        title: "Performance pass",
        category: "performance",
        cadence: "weekly",
        summary: "Find obvious hotspots and apply targeted, behavior-preserving speedups.",
        intent: "Find the clearest performance problems in this repository: allocations inside hot loops, repeated work that could be cached, unbounded reads, N-plus-one patterns, and needless clones. Apply targeted, behavior-preserving improvements - small and local, not a rewrite. Keep public surfaces identical. Prove the tests still pass after each change, and note what you changed and why it is faster.",
        default_commands: &["cargo build", "cargo test"],
    },
    ForgeRecipe {
        id: "quality_cleanup",
        title: "Lint and format cleanup",
        category: "quality",
        cadence: "nightly",
        summary: "Clear linter warnings, fix formatting, remove dead warnings.",
        intent: "Bring this repository to a clean lint and format state. Fix every linter warning the project's own linter reports, apply the formatter, and resolve dead-code and unused-import warnings. Make no behavioral change - only the changes the linter and formatter ask for, plus any trivially-correct follow-ups they imply. Prove the lint and build are clean.",
        default_commands: &[
            "cargo fmt --check",
            "cargo clippy --workspace -- -D warnings",
        ],
    },
    ForgeRecipe {
        id: "bug_hunt",
        title: "Bug hunt",
        category: "bugs",
        cadence: "weekly",
        summary: "Hunt likely bugs - panic paths, swallowed errors, edge cases - and fix the safe ones.",
        intent: "Hunt for likely bugs across this repository: unwrap or expect on fallible paths, swallowed or ignored errors, off-by-one and boundary mistakes, unhandled match arms, and incorrect error propagation. For each clear bug with a safe, local fix, fix it and add a test that would have caught it. For anything ambiguous, leave a precise note. Prove the tests pass.",
        default_commands: &["cargo test"],
    },
    ForgeRecipe {
        id: "test_coverage",
        title: "Test coverage fill",
        category: "tests",
        cadence: "weekly",
        summary: "Add tests for untested public functions and fail-closed cases.",
        intent: "Raise test coverage where it matters. Find public functions and critical paths with no tests, and add tests for their happy paths AND their boundary and fail-closed cases - assert the refusals as carefully as the successes. Do not change behavior to make testing easier; test the behavior as it is. Prove the new tests pass.",
        default_commands: &["cargo test"],
    },
    ForgeRecipe {
        id: "documentation_sweep",
        title: "Documentation sweep",
        category: "docs",
        cadence: "weekly",
        summary: "Add missing doc comments and fix stale ones on the public surface.",
        intent: "Improve the documentation of this repository's public surface. Add a clear doc comment to every public function, type, and module that lacks one, and fix doc comments that no longer match the code. Keep the existing voice and density - match the surrounding style exactly. Change no behavior. Prove the build still passes.",
        default_commands: &["cargo build"],
    },
    ForgeRecipe {
        id: "dead_code_removal",
        title: "Dead code removal",
        category: "quality",
        cadence: "weekly",
        summary: "Remove unused code, imports, and dependencies the project no longer needs.",
        intent: "Remove dead weight from this repository: unused functions, types, and modules; unused imports; and dependencies nothing references. Be conservative - only remove what is provably unused, never a public API that something outside this repo might use. Prove the build and tests still pass after the removals.",
        default_commands: &["cargo build", "cargo test"],
    },
    ForgeRecipe {
        id: "error_handling",
        title: "Error-handling hardening",
        category: "reliability",
        cadence: "weekly",
        summary: "Replace unwrap/expect/panic on fallible paths with proper handling.",
        intent: "Harden error handling across this repository. Find unwrap, expect, and panic on paths that can fail at runtime (I/O, parsing, network, locks) and replace them with proper error propagation or a graceful fallback, matching the project's existing error type and style. Do not touch genuinely-infallible call sites or test code. Prove the tests pass.",
        default_commands: &["cargo build", "cargo test"],
    },
    ForgeRecipe {
        id: "contract_tightening",
        title: "Contract tightening",
        category: "correctness",
        cadence: "weekly",
        summary: "Tighten loose types and add the input validation an invariant needs.",
        intent: "Tighten the contracts in this repository. Find loose or overly-permissive types, missing input validation, and invariants that are assumed but not enforced, and tighten them - a stricter type, a validation at the boundary, a refusal where a bad value is currently accepted. Keep the public behavior correct for valid inputs; only reject what was always invalid. Add a test for each tightened boundary. Prove the tests pass.",
        default_commands: &["cargo test"],
    },
];

/// Look up a recipe by id.
pub fn forge_recipe(id: &str) -> Option<&'static ForgeRecipe> {
    FORGE_RECIPES.iter().find(|recipe| recipe.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_top_ten_well_formed_and_lookupable() {
        assert_eq!(FORGE_RECIPES.len(), 10);
        for recipe in FORGE_RECIPES {
            assert!(!recipe.id.is_empty());
            assert!(!recipe.title.is_empty());
            assert!(!recipe.intent.is_empty());
            assert!(!recipe.default_commands.is_empty());
            assert!(["nightly", "weekly", "on_demand"].contains(&recipe.cadence));
        }
        // Ids are unique and lookupable.
        let mut ids: Vec<&str> = FORGE_RECIPES.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
        assert!(forge_recipe("security_scan").is_some());
        assert!(forge_recipe("not_a_recipe").is_none());
    }

    #[test]
    fn forge_recipes_const_happy_path_exposes_the_top_ten_recipes() {
        assert_eq!(FORGE_RECIPES.len(), 10);
        assert!(FORGE_RECIPES.iter().any(|r| r.id == "performance_pass"));
    }

    #[test]
    fn forge_recipe_happy_path_returns_some_for_known_id() {
        let recipe = forge_recipe("quality_cleanup").expect("known id must resolve");
        assert_eq!(recipe.title, "Lint and format cleanup");
        assert_eq!(recipe.cadence, "nightly");
    }
}
