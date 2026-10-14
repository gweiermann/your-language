//! A native library adapter. The scheduler has no knowledge of these names.
//! Policy: one namespace, same-scope duplicates are errors, inner scopes shadow.
use super::*;

const MODULE: &str = "std/variables";
const CURRENT: &str = "std/variables/currentLexicalScope";
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
        "scope",
        "parentSymbol",
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
                    finalization_only: false,
                    contribution: false,
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
    add(
        "identity",
        vec![],
        false,
        Arc::new(|| Box::new(OwnIdentity)),
    );
    add(
        "identityOf",
        vec![parameter("value", ParameterKind::Value, None)],
        false,
        Arc::new(|| Box::new(Identity { list: false })),
    );
    add(
        "childIdentity",
        vec![],
        false,
        Arc::new(|| Box::new(ChildIdentity)),
    );
    add(
        "identities",
        vec![parameter("value", ParameterKind::Value, None)],
        false,
        Arc::new(|| Box::new(Identity { list: true })),
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
        "declare",
        binding_parameters(),
        false,
        Arc::new(|| Box::new(Declare)),
    );
    add(
        "use",
        vec![parameter("name", ParameterKind::Capture, None)],
        false,
        Arc::new(|| Box::new(Use)),
    );
    // Preserve legacy contracts while exposing the scope/binding library by its
    // responsibility. New identity queries never fall back to syntax identities.
    for ((module, name), id) in registry.exports.clone() {
        if module == MODULE && matches!(name.as_str(), "lexicalScope" | "declare" | "use") {
            registry.exports.insert(("std/references".into(), name), id);
        }
    }
    for ((module, name), id) in registry.symbols.clone() {
        if module == MODULE && name != "reference" {
            registry.symbols.insert(("std/references".into(), name), id);
        }
    }
    registry
        .register_service("std/references", || Box::<Bindings>::default())
        .expect("fresh service");
    for (name, own) in [("reference", true), ("referenceOf", false)] {
        registry
            .register(
                "std/references",
                name,
                OperationSignature {
                    id: format!("std/references#{name}"),
                    provides_context: false,
                    finalization_only: false,
                    contribution: false,
                    parameters: if own {
                        vec![]
                    } else {
                        vec![parameter("value", ParameterKind::Value, None)]
                    },
                },
                move || Box::new(ReferenceIdentity { own }),
            )
            .expect("fresh reference query");
    }
    registry
}

#[derive(Default)]
struct Bindings {
    captures: BTreeMap<(String, usize, usize, Vec<usize>), usize>,
}
impl AnalysisService for Bindings {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
fn capture_key(source: &SourceCapture) -> (String, usize, usize, Vec<usize>) {
    (
        source.span.file.clone(),
        source.span.start,
        source.span.end,
        source.occurrences.clone(),
    )
}
fn associate(context: &mut OperationContext<'_>, source: &SourceCapture, binding: usize) {
    context.with_service::<Bindings, _>("std/references", |service, _| {
        service.captures.insert(capture_key(source), binding);
    });
}
struct ReferenceIdentity {
    own: bool,
}
impl NativeOperation for ReferenceIdentity {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let value = if self.own {
            occurrence_identity(context, context.occurrence.id)
        } else {
            context
                .argument("value")
                .cloned()
                .unwrap_or(SemanticValue::Absent)
        };
        let result = match value {
            SemanticValue::Source(source) => {
                let binding = context
                    .with_service::<Bindings, _>("std/references", |service, _| {
                        service.captures.get(&capture_key(&source)).copied()
                    })
                    .flatten();
                if let Some(binding) = binding {
                    SemanticValue::Identity(SemanticIdentity::Binding(binding))
                } else if source.occurrences.len() == 1 {
                    occurrence_identity(context, source.occurrences[0])
                } else {
                    SemanticValue::Absent
                }
            }
            SemanticValue::Identity(SemanticIdentity::Occurrence(id)) => {
                occurrence_identity(context, id)
            }
            value => value,
        };
        Ok(Evaluation::Ready(match result {
            SemanticValue::Identity(SemanticIdentity::Binding(_)) | SemanticValue::Absent => result,
            _ => SemanticValue::Absent,
        }))
    }
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
struct Declare;
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
        let reassignable = context
            .argument("reassignable")
            .cloned()
            .unwrap_or(SemanticValue::Bool(true));
        let binding = context.record();
        context.set(
            binding,
            key("nameSymbol"),
            SemanticValue::Text(name.text.clone()),
        )?;
        associate(context, &name, binding);
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
                if let SemanticValue::Record(id) = binding {
                    associate(context, &name, id);
                }
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

/// One analysis-local identity, shared by declarations and resolved references.
fn occurrence_identity(context: &OperationContext<'_>, occurrence: usize) -> SemanticValue {
    for role in ["declaration", "reference"] {
        if let Some(SemanticValue::Record(binding)) = context.attachment(occurrence, &symbol(role))
        {
            return SemanticValue::Identity(SemanticIdentity::Binding(*binding));
        }
    }
    SemanticValue::Identity(SemanticIdentity::Occurrence(occurrence))
}
struct OwnIdentity;
impl NativeOperation for OwnIdentity {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(occurrence_identity(
            context,
            context.occurrence.id,
        )))
    }
}
struct Identity {
    list: bool,
}
impl NativeOperation for Identity {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let value = match context.argument("value") {
            Some(SemanticValue::Absent) => SemanticValue::Absent,
            Some(SemanticValue::Source(source)) => {
                if matches!(source.value, AstValue::None) {
                    SemanticValue::Absent
                } else if self.list {
                    SemanticValue::List(
                        source
                            .occurrences
                            .iter()
                            .map(|id| occurrence_identity(context, *id))
                            .collect(),
                    )
                } else if source.occurrences.len() > 1 {
                    return Err(Diagnostic::error(
                        "semantic.identity",
                        "This capture contains multiple identities; use identities(value)",
                        source.span.clone(),
                    ));
                } else if let Some(id) = source.occurrences.first() {
                    occurrence_identity(context, *id)
                } else {
                    return Err(Diagnostic::error(
                        "semantic.identity",
                        "This capture has no syntax occurrence identity",
                        source.span.clone(),
                    ));
                }
            }
            Some(SemanticValue::Identity(identity)) => SemanticValue::Identity(identity.clone()),
            _ => {
                return Err(Diagnostic::error(
                    "semantic.identity",
                    "Expected a syntax capture or identity",
                    context.occurrence.span.clone(),
                ))
            }
        };
        Ok(Evaluation::Ready(value))
    }
}
struct ChildIdentity;
impl NativeOperation for ChildIdentity {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(
            context
                .occurrence
                .children
                .first()
                .map(|child| occurrence_identity(context, child.id))
                .unwrap_or(SemanticValue::Absent),
        ))
    }
}
