//! Identity-keyed, language-independent equality constraints.
//! Primitive identities and named slots are supplied by the language definition.
use super::*;
const MODULE: &str = "std/types";
const UNKNOWN: &str = "std/types#unknown";
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TypeKey {
    identity: SemanticIdentity,
    slot: Option<String>,
}
fn key(value: &SemanticValue, span: &Span) -> Result<TypeKey, Diagnostic> {
    match value {
        SemanticValue::Identity(identity) => Ok(TypeKey {
            identity: identity.clone(),
            slot: None,
        }),
        SemanticValue::Tuple(values) if values.len() == 2 => match (&values[0], &values[1]) {
            (SemanticValue::Identity(identity), SemanticValue::Symbol(slot)) => Ok(TypeKey {
                identity: identity.clone(),
                slot: Some(slot.clone()),
            }),
            _ => Err(Diagnostic::error(
                "types.key",
                "A type slot is (identity, symbol)",
                span.clone(),
            )),
        },
        _ => Err(Diagnostic::error(
            "types.key",
            "Expected an identity or (identity, symbol) type key",
            span.clone(),
        )),
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Type {
    Variable(usize),
    Unknown,
    Atom(String),
    List(Vec<Type>),
}
/// Diagnostic provenance does not participate in type compatibility.
#[derive(Clone, Debug, Default)]
struct Origin {
    fact: Option<Span>,
    requirement: Option<Span>,
}
impl Origin {
    fn merge(&self, other: &Self) -> Self {
        Self {
            fact: self.fact.clone().or_else(|| other.fact.clone()),
            requirement: self
                .requirement
                .clone()
                .or_else(|| other.requirement.clone()),
        }
    }
}
struct Mismatch {
    message: String,
    left: Origin,
    right: Origin,
}
impl Mismatch {
    fn diagnostic(self, fallback: Span) -> Diagnostic {
        let (primary, related, requirement) = if self.right.requirement.is_some() {
            (self.left.fact, self.right.requirement, true)
        } else if self.left.requirement.is_some() {
            (self.right.fact, self.left.requirement, true)
        } else {
            (self.right.fact, self.left.fact, false)
        };
        let mut diagnostic =
            Diagnostic::error("types.mismatch", self.message, primary.unwrap_or(fallback));
        if let Some(related) = related.filter(|span| span != diagnostic.primary.as_ref()) {
            diagnostic.secondary.push(related);
            diagnostic.help = Some(if requirement {
                "The related location requires the incompatible type".into()
            } else {
                "The related location contributed the conflicting type".into()
            });
        }
        diagnostic
    }
}
#[derive(Clone, Default)]
struct Constraints {
    next: usize,
    parents: Vec<usize>,
    projections: BTreeMap<(usize, String), usize>,
    variables: BTreeMap<TypeKey, Type>,
    substitutions: BTreeMap<usize, Type>,
    equalities: Vec<(Type, Type, Origin, Span)>,
    origins: BTreeMap<usize, Origin>,
    settled: bool,
}
impl Constraints {
    fn fresh(&mut self) -> Type {
        let id = self.next;
        self.next += 1;
        self.parents.push(id);
        Type::Variable(id)
    }
    fn root(&self, mut id: usize) -> usize {
        while self.parents[id] != id {
            id = self.parents[id];
        }
        id
    }
    fn variable(&mut self, key: TypeKey) -> Type {
        if let Some(value) = self.variables.get(&key) {
            return value.clone();
        }
        let value = if let Some(slot) = &key.slot {
            let base = self.variable(TypeKey {
                identity: key.identity.clone(),
                slot: None,
            });
            let Type::Variable(base) = base else {
                unreachable!()
            };
            let root = self.root(base);
            if let Some(id) = self.projections.get(&(root, slot.clone())) {
                Type::Variable(*id)
            } else {
                let Type::Variable(id) = self.fresh() else {
                    unreachable!()
                };
                self.projections.insert((root, slot.clone()), id);
                Type::Variable(id)
            }
        } else {
            self.fresh()
        };
        self.variables.insert(key, value.clone());
        value
    }
    fn value(&mut self, value: &SemanticValue, span: &Span) -> Result<Type, Diagnostic> {
        match value {
            SemanticValue::Symbol(value) if value == UNKNOWN => Ok(Type::Unknown),
            SemanticValue::Symbol(value) => Ok(Type::Atom(value.clone())),
            SemanticValue::Deferred {
                service,
                key: value,
                ..
            } if service == MODULE => self.read(value, span),
            SemanticValue::List(values) => Ok(Type::List(
                values
                    .iter()
                    .map(|value| self.value(value, span))
                    .collect::<Result<_, _>>()?,
            )),
            SemanticValue::Absent => Ok(Type::Unknown),
            _ => Err(Diagnostic::error(
                "types.value",
                "Expected a type symbol, deferred type, or list of types",
                span.clone(),
            )),
        }
    }
    fn read(&mut self, value: &SemanticValue, span: &Span) -> Result<Type, Diagnostic> {
        match value {
            SemanticValue::List(values) => Ok(Type::List(
                values
                    .iter()
                    .map(|value| self.read(value, span))
                    .collect::<Result<_, _>>()?,
            )),
            SemanticValue::Absent => Ok(Type::Unknown),
            _ => {
                let key = key(value, span)?;
                Ok(self.variable(key))
            }
        }
    }
    fn resolved(&self, value: &Type) -> Type {
        match value {
            Type::Variable(id) => self
                .substitutions
                .get(&self.root(*id))
                .map(|value| self.resolved(value))
                .unwrap_or_else(|| value.clone()),
            Type::List(values) => {
                Type::List(values.iter().map(|value| self.resolved(value)).collect())
            }
            _ => value.clone(),
        }
    }
    fn occurs(id: usize, value: &Type) -> bool {
        match value {
            Type::Variable(other) => id == *other,
            Type::List(values) => values.iter().any(|value| Self::occurs(id, value)),
            _ => false,
        }
    }
    fn head(&self, value: &Type) -> Type {
        if let Type::Variable(id) = value {
            self.substitutions
                .get(&self.root(*id))
                .map(|value| self.head(value))
                .unwrap_or(Type::Variable(self.root(*id)))
        } else {
            value.clone()
        }
    }
    fn origin(&self, value: &Type, fallback: &Origin, as_value: bool) -> Origin {
        if let Type::Variable(id) = value {
            let inherited = self
                .origins
                .get(&self.root(*id))
                .cloned()
                .unwrap_or_default();
            // A check's explicit requirement must describe this use, not an
            // unrelated fact originally stored in the queried annotation node.
            if fallback.requirement.is_some() {
                Origin {
                    fact: None,
                    requirement: fallback.requirement.clone(),
                }
            } else if as_value && fallback.fact.is_some() && inherited.requirement.is_some() {
                Origin {
                    fact: inherited.fact.or_else(|| fallback.fact.clone()),
                    requirement: None,
                }
            } else {
                inherited.merge(fallback)
            }
        } else {
            fallback.clone()
        }
    }
    fn unify(
        &mut self,
        left: Type,
        right: Type,
        left_origin: Origin,
        right_origin: Origin,
    ) -> Result<(), Box<Mismatch>> {
        let left_origin = self.origin(&left, &left_origin, false);
        let right_origin = self.origin(&right, &right_origin, true);
        let fail = |message: String| {
            Box::new(Mismatch {
                message,
                left: left_origin.clone(),
                right: right_origin.clone(),
            })
        };
        if let (Type::Variable(left), Type::Variable(right)) = (&left, &right) {
            let (left, right) = (self.root(*left), self.root(*right));
            if left == right {
                return Ok(());
            }
            let a = self.substitutions.remove(&left);
            let b = self.substitutions.remove(&right);
            let stored_requirement = self
                .origins
                .get(&left)
                .and_then(|origin| origin.requirement.clone())
                .or_else(|| {
                    self.origins
                        .get(&right)
                        .and_then(|origin| origin.requirement.clone())
                });
            self.parents[right] = left;
            self.origins.remove(&right);
            let mut merged_origin = left_origin.merge(&right_origin);
            // When positive information replaces unknown, its source replaces
            // the unknown contribution's source as well.
            if matches!(a, Some(Type::Unknown)) && !matches!(b, None | Some(Type::Unknown)) {
                merged_origin.fact = right_origin.fact.clone();
            }
            merged_origin.requirement = merged_origin.requirement.or(stored_requirement);
            self.origins.insert(left, merged_origin);
            if let Some(a) = a
                .clone()
                .filter(|value| !matches!(value, Type::Unknown) || b.is_none())
            {
                self.substitutions.insert(left, a);
            } else if let Some(b) = b.clone() {
                self.substitutions.insert(left, b);
            }
            let moved: Vec<_> = self
                .projections
                .iter()
                .filter(|((owner, _), _)| *owner == right)
                .map(|((_, slot), value)| (slot.clone(), *value))
                .collect();
            for (slot, value) in moved {
                self.projections.remove(&(right, slot.clone()));
                if let Some(existing) = self.projections.get(&(left, slot.clone())).copied() {
                    self.unify(
                        Type::Variable(existing),
                        Type::Variable(value),
                        left_origin.clone(),
                        right_origin.clone(),
                    )?;
                } else {
                    self.projections.insert((left, slot), value);
                }
            }
            if let (Some(a), Some(b)) = (a, b) {
                self.unify(a, b, left_origin.clone(), right_origin.clone())?;
            }
            if self.cyclic(left, &mut std::collections::BTreeSet::new()) {
                return Err(fail(
                    "Recursive constraint requires an infinite type".into(),
                ));
            }
            return Ok(());
        }
        for (variable, requirement) in [(&left, &right), (&right, &left)] {
            if let Type::Variable(id) = variable {
                if matches!(self.head(variable), Type::Unknown)
                    && !matches!(self.head(requirement), Type::Unknown)
                {
                    let id = self.root(*id);
                    let mut origin = left_origin.merge(&right_origin);
                    origin.fact = if matches!(self.head(&left), Type::Unknown) {
                        right_origin.fact.clone()
                    } else {
                        left_origin.fact.clone()
                    };
                    self.substitutions.insert(id, self.head(requirement));
                    self.origins.insert(id, origin);
                    if self.cyclic(id, &mut std::collections::BTreeSet::new()) {
                        return Err(fail(
                            "Recursive constraint requires an infinite type".into(),
                        ));
                    }
                    return Ok(());
                }
            }
        }
        let (left, right) = (self.head(&left), self.head(&right));
        if left == right {
            return Ok(());
        }
        match (left, right) {
            (Type::Variable(id), value) | (value, Type::Variable(id)) => {
                let id = self.root(id);
                if Self::occurs(id, &value) {
                    return Err(fail(
                        "Recursive constraint requires an infinite type".into(),
                    ));
                }
                self.substitutions.insert(id, value);
                self.origins.insert(id, left_origin.merge(&right_origin));
                if self.cyclic(id, &mut std::collections::BTreeSet::new()) {
                    return Err(fail(
                        "Recursive constraint requires an infinite type".into(),
                    ));
                }
                Ok(())
            }
            (Type::Unknown, _) | (_, Type::Unknown) => Ok(()),
            (Type::List(left), Type::List(right)) if left.len() == right.len() => {
                for (left, right) in left.into_iter().zip(right) {
                    self.unify(left, right, left_origin.clone(), right_origin.clone())?;
                }
                Ok(())
            }
            (left, right) => Err(fail(format!(
                "Type {} does not match {}",
                Self::display(&left),
                Self::display(&right)
            ))),
        }
    }
    fn cyclic(&self, id: usize, path: &mut std::collections::BTreeSet<usize>) -> bool {
        let id = self.root(id);
        if !path.insert(id) {
            return true;
        }
        fn children(value: &Type, output: &mut Vec<usize>) {
            match value {
                Type::Variable(id) => output.push(*id),
                Type::List(values) => {
                    for value in values {
                        children(value, output)
                    }
                }
                _ => {}
            }
        }
        let mut descendants: Vec<_> = self
            .projections
            .iter()
            .filter(|((owner, _), _)| self.root(*owner) == id)
            .map(|(_, value)| *value)
            .collect();
        if let Some(value) = self.substitutions.get(&id) {
            children(value, &mut descendants);
        }
        let found = descendants
            .into_iter()
            .any(|child| self.cyclic(child, path));
        path.remove(&id);
        found
    }
    fn display(value: &Type) -> String {
        match value {
            Type::Variable(_) | Type::Unknown => "unknown".into(),
            Type::Atom(name) => name.rsplit('#').next().unwrap_or(name).into(),
            Type::List(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::display)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    fn evaluation(&self, value: &Type) -> Evaluation {
        match self.resolved(value) {
            Type::Variable(_) if !self.settled => Evaluation::Pending,
            Type::Variable(_) | Type::Unknown => {
                Evaluation::Ready(SemanticValue::Symbol(UNKNOWN.into()))
            }
            Type::Atom(value) => Evaluation::Ready(SemanticValue::Symbol(value)),
            Type::List(values) => {
                let mut result = Vec::new();
                for value in values {
                    let Evaluation::Ready(value) = self.evaluation(&value) else {
                        return Evaluation::Pending;
                    };
                    result.push(value);
                }
                Evaluation::Ready(SemanticValue::List(result))
            }
        }
    }
}
impl AnalysisService for Constraints {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn resolve_value(
        &mut self,
        value: &SemanticValue,
        context: &mut OperationContext<'_>,
    ) -> Result<Evaluation, Diagnostic> {
        let value = self.read(value, context.operation_span)?;
        Ok(self.evaluation(&value))
    }
    fn progress(&mut self, context: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
        let equations = std::mem::take(&mut self.equalities);
        let changed = !equations.is_empty();
        for (left, right, origin, span) in equations {
            let snapshot = self.clone();
            if let Err(mismatch) = self.unify(left, right, Origin::default(), origin) {
                *self = snapshot;
                context.emit(mismatch.diagnostic(span));
            }
        }
        Ok(changed)
    }
    fn settle(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.settled = true;
        for id in 0..self.next {
            if matches!(self.resolved(&Type::Variable(id)), Type::Variable(_)) {
                self.substitutions.insert(self.root(id), Type::Unknown);
            }
        }
        Ok(())
    }
    fn finish(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        if self.variables.is_empty() {
            return Ok(());
        }
        // Cleanup also runs after a failed native action, without normal settle.
        if !self.settled {
            self.settle(context)?;
        }
        let report = context.record();
        let typed_report = context.record();
        let mut reported = self.variables.clone();
        for (key, value) in &self.variables {
            if key.slot.is_none() {
                if let Type::Variable(id) = value {
                    for ((owner, slot), projected) in &self.projections {
                        if self.root(*owner) == self.root(*id) {
                            reported.insert(
                                TypeKey {
                                    identity: key.identity.clone(),
                                    slot: Some(slot.clone()),
                                },
                                Type::Variable(*projected),
                            );
                        }
                    }
                }
            }
        }
        for (key, value) in &reported {
            let typed = context.record();
            let identity = SemanticValue::Identity(key.identity.clone());
            let identity = if let Some(slot) = &key.slot {
                SemanticValue::Tuple(vec![identity, SemanticValue::Symbol(slot.clone())])
            } else {
                identity
            };
            context.set(typed, SemanticKey::Symbol("std/types#key".into()), identity)?;
            let Evaluation::Ready(ready) = self.evaluation(value) else {
                return Err(Diagnostic::error(
                    "types.unsettled",
                    "A type remained pending after finalization",
                    context.occurrence.span.clone(),
                ));
            };
            context.set(typed, SemanticKey::Symbol("std/types#type".into()), ready)?;
            context.set(
                typed_report,
                SemanticKey::Text(format!("{:?}/{:?}", key.identity, key.slot)),
                SemanticValue::Record(typed),
            )?;
            let value = SemanticValue::Text(Self::display(&self.resolved(value)));
            if let SemanticIdentity::Binding(binding) = key.identity {
                let field = key
                    .slot
                    .clone()
                    .unwrap_or_else(|| "std/types#inferredType".into());
                context.set(binding, SemanticKey::Symbol(field), value.clone())?;
            }
            context.set(
                report,
                SemanticKey::Text(format!("{:?}/{:?}", key.identity, key.slot)),
                value,
            )?;
        }
        context.attach("std/types#inferredTypes", SemanticValue::Record(report));
        context.attach("std/types#typeValues", SemanticValue::Record(typed_report));
        Ok(())
    }
}
struct Ensure;
impl NativeOperation for Ensure {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let target = context
            .argument("key")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        let requirement = context
            .argument("type")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        key(&target, context.operation_span)?;
        if matches!(requirement, SemanticValue::Absent) {
            return Ok(());
        }
        let span = context.identity_span(&requirement);
        let is_requirement = context.argument("requirement") == Some(&SemanticValue::Bool(true));
        let origin = if is_requirement {
            Origin {
                fact: None,
                requirement: Some(span.clone()),
            }
        } else {
            Origin {
                fact: Some(span.clone()),
                requirement: None,
            }
        };
        context
            .with_service::<Constraints, _>(MODULE, |state, context| {
                let target = state.read(&target, context.operation_span)?;
                let requirement = state.value(&requirement, context.operation_span)?;
                state.equalities.push((target, requirement, origin, span));
                Ok(())
            })
            .unwrap_or_else(|| {
                Err(Diagnostic::error(
                    "types.service",
                    "Type service is unavailable",
                    context.occurrence.span.clone(),
                ))
            })
    }
}
struct TypeOf;
impl NativeOperation for TypeOf {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let key = context
            .argument("key")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        if matches!(key, SemanticValue::Absent) {
            return Ok(Evaluation::Ready(SemanticValue::Absent));
        }
        // Validate and reserve the variables now. The opaque handle lets ensure
        // link pending variables immediately; a match resolves it observationally.
        context
            .with_service::<Constraints, _>(MODULE, |state, context| {
                state.read(&key, context.operation_span)
            })
            .ok_or_else(|| {
                Diagnostic::error(
                    "types.service",
                    "Type service is unavailable",
                    context.occurrence.span.clone(),
                )
            })??;
        Ok(Evaluation::Ready(SemanticValue::Deferred {
            service: MODULE.into(),
            span: context.identity_span(&key),
            key: Box::new(key),
        }))
    }
}
pub fn register(registry: &mut OperationRegistry) -> Result<(), String> {
    registry.export_symbol(MODULE, "unknown")?;
    registry.register_service(MODULE, || Box::<Constraints>::default())?;
    for (name, parameters, factory) in [
        (
            "ensure",
            vec!["key", "type"],
            Arc::new(|| Box::new(Ensure) as Box<dyn NativeOperation>) as Factory,
        ),
        (
            "typeof",
            vec!["key"],
            Arc::new(|| Box::new(TypeOf) as Box<dyn NativeOperation>) as Factory,
        ),
    ] {
        registry.register(
            MODULE,
            name,
            OperationSignature {
                id: format!("{MODULE}#{name}"),
                finalization_only: true,
                provides_context: false,
                parameters: {
                    let mut parameters: Vec<_> = parameters
                        .into_iter()
                        .map(|name| OperationParameter {
                            name: name.into(),
                            kind: ParameterKind::Value,
                            default: None,
                        })
                        .collect();
                    if name == "ensure" {
                        parameters.push(OperationParameter {
                            name: "requirement".into(),
                            kind: ParameterKind::Bool,
                            default: Some(MeaningValue::Bool(false)),
                        });
                    }
                    parameters
                },
            },
            move || factory(),
        )?;
    }
    Ok(())
}
