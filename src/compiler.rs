//! Compile module graphs into normalized rules and an executable image.
//!
//! Graph loading precedes resolution; expansion preserves source spans and stable
//! symbol identities. Surface grammar and wrappers never reach the runtime.

mod arguments;
mod backend;
mod capture_shapes;
mod capture_validation;
mod checks;
mod condition_validation;
mod declarations;
mod grammar;
mod imports;
mod loading;
mod native_semantics;
mod pipeline;
mod query_actions;
mod query_arguments;
mod query_expressions;
mod query_functions;
mod resolution;
mod rewrite_diagnostics;
mod rewrites;
mod rules;
mod semantic_definitions;
mod semantic_groups;
mod substitution;
mod term_edges;
mod tooling;
mod validation;
mod wrappers;

use capture_shapes::{
    capture_sets, capture_terms, captures, find_marker, marker_value_cardinality, node_term,
    rule_capture_sets, string_term, valid_cardinality, validate_projections, value_shape,
};
pub(crate) use capture_validation::validate_meaning_captures;
use capture_validation::{
    meaning_value_matches, semantic_has_context_query, semantic_has_query, symbol_has_capture,
};
use condition_validation::{validate_condition, ConditionType};
use rewrite_diagnostics::validate_case_emission;
use substitution::{bind_constraint, substitute, ungroup};
use term_edges::edge_ref;
pub(crate) use validation::{
    reject_retired_standard_operation, validate_language, validate_meaning_definitions,
};

use crate::{
    diagnostic::{CompileResult, Diagnostic, Severity, Span},
    frontend::parse_yl,
    ir::*,
    semantics::{
        MeaningAction, MeaningCall, MeaningCase, MeaningChain, MeaningDefinition, MeaningGroup,
        MeaningMatch, MeaningMessagePart, MeaningSelector, MeaningValue, OperationRegistry,
        ParameterKind,
    },
    syntax::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Diagnostic>;
/// Bound grammar parameters after argument normalization; never semantic runtime values.
type GrammarBindings = BTreeMap<String, GrammarArgument>;
/// A grammar argument is either an expanded term or a qualified contextual enum variant.
#[derive(Clone)]
enum GrammarArgument {
    Grammar(Term),
    Enum(String, String, Option<Span>),
}
/// Source-owned declaration indexed by its canonical module/member identity.
/// `scope` is the containing declaration path; `name` includes the member path.
#[derive(Clone)]
struct DeclarationSymbol {
    module: String,
    name: String,
    scope: String,
    declaration: Declaration,
}
/// Per-compilation working state, discarded once the executable image is built.
/// Maps use ordered keys to keep diagnostics and serialized output deterministic.
#[derive(Default)]
struct Compiler {
    registry: OperationRegistry,
    /// Pattern occurrence levels retained for semantic scheduling and backend captures.
    selected_patterns: BTreeSet<String>,
    modules: BTreeMap<String, Module>,
    /// Resolved module graph edges: (importing module, source spelling) -> canonical ID.
    dependencies: BTreeMap<(String, String), String>,
    imports: BTreeMap<(String, String), String>,
    namespaces: BTreeMap<(String, String), String>,
    symbols: BTreeMap<String, DeclarationSymbol>,
    exports: BTreeMap<(String, String), String>,
    members: BTreeMap<(String, String), String>,
    extensions: Vec<(String, Declaration)>,
    /// Active grammar expansion stack for cycle diagnostics.
    expanding: Vec<String>,
    next_marker: u32,
    /// Call-site identity path and copied-node budget for hygienic wrapper expansion.
    invocation_path: std::cell::RefCell<Vec<Span>>,
    expansion_nodes: std::cell::Cell<usize>,
    query_operations: BTreeSet<String>,
    /// None reserves an in-progress recursive function; Some owns its normalized body.
    query_functions:
        std::cell::RefCell<BTreeMap<String, Option<crate::semantics::queries::QueryFunction>>>,
    diagnostics: Vec<Diagnostic>,
}

/// Compile an entry file and its transitive imports.
///
/// `entry_file` selects the graph root; diagnostic spans retain canonical module IDs
/// relative to its directory. Authored standard modules are loaded from bundled sources.
pub fn compile_language(entry_file: impl AsRef<Path>) -> CompileResult<CompiledLanguage> {
    let absolute = std::fs::canonicalize(entry_file.as_ref()).map_err(|e| {
        vec![Diagnostic::error(
            "yl.io",
            e.to_string(),
            Span::new(&entry_file.as_ref().to_string_lossy(), 0, 0),
        )]
    })?;
    let root = absolute.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut compiler = Compiler::default();
    register_query_primitives(&mut compiler.registry)?;
    compiler
        .read_module(&absolute, &root)
        .map_err(|e| vec![e])?;
    compiler.compile().map_err(|e| vec![e])
}
/// In-memory graph API, also useful to hosts which do not use a filesystem.
/// Relative imports resolve against module IDs; omitted extensions become `.yl`.
/// `entry` names a key in `sources`; each value is complete YL source text.
/// Unreferenced source entries are not loaded into the compilation graph.
pub fn compile_sources(
    entry: &str,
    sources: &BTreeMap<String, String>,
) -> CompileResult<CompiledLanguage> {
    compile_sources_with_registry(entry, sources, &OperationRegistry::new())
}
/// Compile a module graph with explicitly registered native operation libraries.
///
/// `registry` is cloned, so registration and compilation cannot mutate the host.
/// Bundled authored standard modules cannot be replaced by native registrations.
pub fn compile_sources_with_registry(
    entry: &str,
    sources: &BTreeMap<String, String>,
    registry: &OperationRegistry,
) -> CompileResult<CompiledLanguage> {
    for module in [
        "std/references",
        "std/variables",
        "std/types",
        "std/types/structural",
        "std/types/arithmetic",
        "std/backend/integer",
    ] {
        if registry.contains_module(module) {
            return Err(vec![Diagnostic::error(
                "yl.reserved_module",
                format!("{module} is an authored standard module and cannot be replaced by native operations"),
                Span::new(entry, 0, 0),
            )]);
        }
    }
    let mut registry = registry.clone();
    if !registry.contains_module("core/query") {
        register_query_primitives(&mut registry)?;
    }
    let mut compiler = Compiler {
        registry,
        ..Compiler::default()
    };
    compiler.read_sources(entry, sources).map_err(|e| vec![e])?;
    compiler.compile().map_err(|e| vec![e])
}
/// Keep core registration diagnostics identical for filesystem and in-memory hosts.
fn register_query_primitives(registry: &mut OperationRegistry) -> CompileResult<()> {
    crate::semantics::queries::register(registry).map_err(|error| {
        vec![Diagnostic::error(
            "yl.query_registry",
            error,
            Span::default(),
        )]
    })
}

#[cfg(test)]
mod authored_standard_module_tests;
#[cfg(test)]
mod declarative_query_frontend_tests;
#[cfg(test)]
mod semantic_artifact_tests;
