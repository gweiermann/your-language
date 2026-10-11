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
#[derive(Clone, Default)]
struct Constraints {
    next: usize,
    parents: Vec<usize>,
    projections: BTreeMap<(usize, String), usize>,
    variables: BTreeMap<TypeKey, Type>,
    substitutions: BTreeMap<usize, Type>,
    equalities: Vec<(Type, Type, Span)>,
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
    fn unify(&mut self, left: Type, right: Type) -> Result<(), String> {
        if let (Type::Variable(left), Type::Variable(right)) = (&left, &right) {
            let (left, right) = (self.root(*left), self.root(*right));
            if left == right {
                return Ok(());
            }
            let a = self.substitutions.remove(&left);
            let b = self.substitutions.remove(&right);
            self.parents[right] = left;
            if let Some(a) = a.clone().filter(|value| !matches!(value, Type::Unknown) || b.is_none()) {
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
                    self.unify(Type::Variable(existing), Type::Variable(value))?;
                } else {
                    self.projections.insert((left, slot), value);
                }
            }
            if let (Some(a), Some(b)) = (a, b) {
                self.unify(a, b)?;
            }
            if self.cyclic(left, &mut std::collections::BTreeSet::new()) {
                return Err("Recursive constraint requires an infinite type".into());
            }
            return Ok(());
        }
        for (variable, requirement) in [(&left, &right), (&right, &left)] {
            if let Type::Variable(id) = variable {
                if matches!(self.head(variable), Type::Unknown) && !matches!(self.head(requirement), Type::Unknown) {
                    let id = self.root(*id);
                    self.substitutions.insert(id, self.head(requirement));
                    if self.cyclic(id, &mut std::collections::BTreeSet::new()) { return Err("Recursive constraint requires an infinite type".into()); }
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
                    return Err("Recursive constraint requires an infinite type".into());
                }
                self.substitutions.insert(id, value);
                if self.cyclic(id, &mut std::collections::BTreeSet::new()) {
                    return Err("Recursive constraint requires an infinite type".into());
                }
                Ok(())
            }
            (Type::Unknown, _) | (_, Type::Unknown) => Ok(()),
            (Type::List(left), Type::List(right)) if left.len() == right.len() => {
                for (left, right) in left.into_iter().zip(right) {
                    self.unify(left, right)?;
                }
                Ok(())
            }
            (left, right) => Err(format!(
                "Type {} does not match {}",
                Self::display(&left),
                Self::display(&right)
            )),
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
        for (left, right, span) in equations {
            let snapshot = self.clone();
            if let Err(message) = self.unify(left, right) {
                *self = snapshot;
                context.emit(Diagnostic::error("types.mismatch", message, span));
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
        let report = context.record();
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
        if matches!(requirement, SemanticValue::Absent) {
            return Ok(());
        }
        let origin = context.identity_span(&requirement);
        context
            .with_service::<Constraints, _>(MODULE, |state, context| {
                let target = state.read(&target, context.operation_span)?;
                let requirement = state.value(&requirement, context.operation_span)?;
                state.equalities.push((target, requirement, origin));
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
                parameters: parameters
                    .into_iter()
                    .map(|name| OperationParameter {
                        name: name.into(),
                        kind: ParameterKind::Value,
                        default: None,
                    })
                    .collect(),
            },
            move || factory(),
        )?;
    }
    Ok(())
}
