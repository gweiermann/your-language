//! A native library adapter. The scheduler has no knowledge of these names.
//! Policy: one namespace, same-scope duplicates are errors, inner scopes shadow.
use super::*;

const MODULE: &str = "std/semantics";
const CURRENT: &str = "std/semantics/currentLexicalScope";
fn symbol(name: &str) -> String {
    format!("{MODULE}#{name}")
}
fn key(name: &str) -> SemanticKey {
    SemanticKey::Symbol(symbol(name))
}
fn parameter(name: &str, kind: ParameterKind, default: Option<MeaningValue>) -> OperationParameter {
    OperationParameter {
        name: name.into(),
        kind,
        default,
    }
}
/// A fresh registry; factories create fresh per-occurrence state for each analysis.
pub fn registry() -> OperationRegistry {
    let mut registry = OperationRegistry::new();
    for name in [
        "variable",
        "function",
        "scope",
        "parentSymbol",
        "kindSymbol",
        "nameSymbol",
        "originSymbol",
        "reassignableSymbol",
        "declaration",
        "reference",
    ] {
        registry
            .symbols
            .insert((MODULE.into(), name.into()), symbol(name));
    }
    let mut add = |name: &str,
                   parameters: Vec<OperationParameter>,
                   provides_context: bool,
                   factory: Factory| {
        let id = symbol(name);
        registry
            .exports
            .insert((MODULE.into(), name.into()), id.clone());
        registry.operations.insert(
            id.clone(),
            (
                OperationSignature {
                    id,
                    parameters,
                    provides_context,
                },
                factory,
            ),
        );
    };
    add(
        "lexicalScope",
        vec![],
        true,
        Arc::new(|| Box::new(LexicalScope::default())),
    );
    let binding_parameters = || {
        vec![
            parameter("name", ParameterKind::Capture, None),
            parameter(
                "reassignable",
                ParameterKind::Bool,
                Some(MeaningValue::Bool(true)),
            ),
        ]
    };
    add(
        "declareVariable",
        binding_parameters(),
        false,
        Arc::new(|| {
            Box::new(Declare {
                kind: Some(symbol("variable")),
            })
        }),
    );
    add(
        "declareFunction",
        binding_parameters(),
        false,
        Arc::new(|| {
            Box::new(Declare {
                kind: Some(symbol("function")),
            })
        }),
    );
    let mut generic = vec![parameter("kind", ParameterKind::Symbol, None)];
    generic.extend(binding_parameters());
    add(
        "declare",
        generic,
        false,
        Arc::new(|| Box::new(Declare { kind: None })),
    );
    for name in ["use", "useVariable"] {
        add(
            name,
            vec![parameter("name", ParameterKind::Capture, None)],
            false,
            Arc::new(|| Box::new(Use)),
        );
    }
    registry
}

#[derive(Default)]
struct LexicalScope {
    scope: Option<usize>,
    previous: SemanticValue,
}
impl Default for SemanticValue {
    fn default() -> Self {
        Self::Absent
    }
}
impl NativeOperation for LexicalScope {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let parent = context.global(CURRENT);
        let scope = context.record();
        context.set(scope, key("parentSymbol"), parent)?;
        context.attach(&symbol("scope"), SemanticValue::Record(scope));
        self.scope = Some(scope);
        Ok(())
    }
    fn enter(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.previous = context.global(CURRENT);
        let Some(scope) = self.scope else {
            return Err(Diagnostic::error(
                "semantic.lifecycle",
                "Scope entered before setup",
                context.occurrence.span.clone(),
            ));
        };
        context.set_global(CURRENT, SemanticValue::Record(scope));
        Ok(())
    }
    fn leave(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        context.set_global(CURRENT, self.previous.clone());
        Ok(())
    }
}
fn name(context: &OperationContext<'_>) -> Result<SourceCapture, Diagnostic> {
    match context.argument("name") {
        Some(SemanticValue::Source(source)) => Ok(source.clone()),
        _ => Err(Diagnostic::error(
            "semantic.argument",
            "Expected a source capture for name",
            context.occurrence.span.clone(),
        )),
    }
}
fn current(context: &OperationContext<'_>) -> Result<usize, Diagnostic> {
    match context.global(CURRENT) {
        SemanticValue::Record(id) => Ok(id),
        _ => Err(Diagnostic::error(
            "semantic.no_scope",
            "No enclosing lexical scope",
            context.occurrence.span.clone(),
        )),
    }
}
struct Declare {
    kind: Option<String>,
}
impl NativeOperation for Declare {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let name = name(context)?;
        let scope = current(context)?;
        let entry = SemanticKey::Text(name.text.clone());
        if let Some(SemanticValue::Record(previous)) = context.get(scope, &entry) {
            let mut diagnostic = Diagnostic::error(
                "semantic.duplicate_declaration",
                format!("{} is already declared in this scope", name.text),
                name.span.clone(),
            );
            if let Some(SemanticValue::Source(origin)) =
                context.get(*previous, &key("originSymbol"))
            {
                diagnostic.secondary.push(origin.span.clone());
            }
            context.emit(diagnostic);
            return Ok(());
        }
        let kind = match (&self.kind, context.argument("kind")) {
            (Some(kind), _) | (None, Some(SemanticValue::Symbol(kind))) => kind.clone(),
            _ => {
                return Err(Diagnostic::error(
                    "semantic.argument",
                    "Expected a declaration category symbol",
                    context.occurrence.span.clone(),
                ))
            }
        };
        let reassignable = context
            .argument("reassignable")
            .cloned()
            .unwrap_or(SemanticValue::Bool(true));
        let binding = context.record();
        context.set(binding, key("kindSymbol"), SemanticValue::Symbol(kind))?;
        context.set(
            binding,
            key("nameSymbol"),
            SemanticValue::Text(name.text.clone()),
        )?;
        context.set(binding, key("originSymbol"), SemanticValue::Source(name))?;
        context.set(binding, key("reassignableSymbol"), reassignable)?;
        context.set(scope, entry, SemanticValue::Record(binding))?;
        context.attach(&symbol("declaration"), SemanticValue::Record(binding));
        Ok(())
    }
}
struct Use;
impl NativeOperation for Use {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let name = name(context)?;
        let mut scope = Some(current(context)?);
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = scope {
            if !visited.insert(id) {
                return Err(Diagnostic::error(
                    "semantic.scope_cycle",
                    "Lexical scope parent links contain a cycle",
                    name.span,
                ));
            }
            if let Some(binding) = context
                .get(id, &SemanticKey::Text(name.text.clone()))
                .cloned()
            {
                context.attach(&symbol("reference"), binding.clone());
                return Ok(());
            }
            scope = match context.get(id, &key("parentSymbol")) {
                Some(SemanticValue::Record(parent)) => Some(*parent),
                _ => None,
            };
        }
        context.emit(Diagnostic::error(
            "semantic.unresolved_reference",
            format!("No declaration found for {}", name.text),
            name.span,
        ));
        Ok(())
    }
}
