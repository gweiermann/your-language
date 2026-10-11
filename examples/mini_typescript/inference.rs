//! Throwaway composable monomorphic constraints; no grammar names or arithmetic policy.
use std::{any::Any, collections::BTreeMap};
use your_language::{
    diagnostic::{Diagnostic, Span},
    semantics::{
        AnalysisService, Evaluation, NativeOperation, OperationContext, OperationParameter,
        OperationSignature, ParameterKind, SemanticEngine, SemanticKey, SemanticValue,
        SourceCapture,
    },
};
const MODULE: &str = "std/types";
fn symbol(name: &str) -> String {
    format!("{MODULE}#{name}")
}
fn key(name: &str) -> SemanticKey {
    SemanticKey::Symbol(symbol(name))
}
#[derive(Clone, Debug, PartialEq)]
enum Type {
    Var(usize),
    Unknown,
    Primitive(String),
    Function(Vec<Type>, Box<Type>),
}
impl Type {
    fn display(&self) -> String {
        match self {
            Self::Var(_) | Self::Unknown => "unknown".into(),
            Self::Primitive(name) => name.rsplit('#').next().unwrap_or(name).into(),
            Self::Function(params, result) => format!(
                "({}) => {}",
                params
                    .iter()
                    .map(Self::display)
                    .collect::<Vec<_>>()
                    .join(", "),
                result.display()
            ),
        }
    }
}
#[derive(Default)]
struct Constraints {
    next: usize,
    expressions: BTreeMap<usize, Type>,
    bindings: BTreeMap<usize, Type>,
    equalities: Vec<(Type, Type, Span)>,
    substitutions: BTreeMap<usize, Type>,
    settled: bool,
    unknown_symbol: Option<String>,
    function_symbol: Option<String>,
}
impl Constraints {
    fn fresh(&mut self) -> Type {
        let id = self.next;
        self.next += 1;
        Type::Var(id)
    }
    fn expression(&mut self, id: usize) -> Type {
        if let Some(ty) = self.expressions.get(&id) {
            return ty.clone();
        }
        let ty = self.fresh();
        self.expressions.insert(id, ty.clone());
        ty
    }
    fn binding(&mut self, id: usize) -> Type {
        if let Some(ty) = self.bindings.get(&id) {
            return ty.clone();
        }
        let ty = self.fresh();
        self.bindings.insert(id, ty.clone());
        ty
    }
    fn equal(&mut self, a: Type, b: Type, span: &Span) {
        self.equalities.push((a, b, span.clone()));
    }
    fn resolve(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(id) => self
                .substitutions
                .get(id)
                .map(|ty| self.resolve(ty))
                .unwrap_or_else(|| ty.clone()),
            Type::Function(params, result) => Type::Function(
                params.iter().map(|ty| self.resolve(ty)).collect(),
                Box::new(self.resolve(result)),
            ),
            _ => ty.clone(),
        }
    }
    fn occurs(id: usize, ty: &Type) -> bool {
        match ty {
            Type::Var(other) => id == *other,
            Type::Function(params, result) => {
                params.iter().any(|ty| Self::occurs(id, ty)) || Self::occurs(id, result)
            }
            _ => false,
        }
    }
    fn unify(&mut self, a: Type, b: Type) -> Result<(), String> {
        let (a, b) = (self.resolve(&a), self.resolve(&b));
        if a == b {
            return Ok(());
        }
        match (a, b) {
            (Type::Var(id), ty) | (ty, Type::Var(id)) => {
                if Self::occurs(id, &ty) {
                    return Err("Recursive constraint requires an infinite type".into());
                }
                self.substitutions.insert(id, ty);
                Ok(())
            }
            // Settled unknown carries no positive information and is never a type error.
            (Type::Unknown, _) | (_, Type::Unknown) => Ok(()),
            (Type::Function(a, ar), Type::Function(b, br)) if a.len() == b.len() => {
                for (a, b) in a.into_iter().zip(b) {
                    self.unify(a, b)?;
                }
                self.unify(*ar, *br)
            }
            (a, b) => Err(format!(
                "Cannot equate {} with {}",
                a.display(),
                b.display()
            )),
        }
    }
    fn captures(&mut self, capture: &SourceCapture) -> Vec<Type> {
        capture
            .occurrences
            .iter()
            .map(|id| self.expression(*id))
            .collect()
    }
    fn capture(&mut self, capture: &SourceCapture) -> Option<Type> {
        capture.occurrences.first().map(|id| self.expression(*id))
    }
}
impl AnalysisService for Constraints {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn progress(&mut self, ctx: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
        let equations = std::mem::take(&mut self.equalities);
        let changed = !equations.is_empty();
        for (a, b, span) in equations {
            if let Err(message) = self.unify(a, b) {
                ctx.emit(Diagnostic::error("prototype.type_mismatch", message, span));
            }
        }
        Ok(changed)
    }
    fn settle(&mut self, _ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.settled = true;
        // All existing unconstrained equivalence classes become settled unknown.
        for id in 0..self.next {
            if matches!(self.resolve(&Type::Var(id)), Type::Var(_)) {
                self.substitutions.insert(id, Type::Unknown);
            }
        }
        Ok(())
    }
    fn finish(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        for (id, ty) in &self.bindings {
            ctx.set(
                *id,
                key("inferredType"),
                SemanticValue::Text(self.resolve(ty).display()),
            )?;
        }
        let report = ctx.record();
        for (id, ty) in &self.expressions {
            ctx.set(
                report,
                SemanticKey::Text(id.to_string()),
                SemanticValue::Text(self.resolve(ty).display()),
            )?;
        }
        ctx.attach(&symbol("expressionTypes"), SemanticValue::Record(report));
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Configure,
    Type,
    TypeOf,
    Alias,
    Inherit,
    Declaration,
    Binding,
    Reference,
    Function,
    Call,
}
struct TypeOperation(Kind);
fn capture(ctx: &OperationContext<'_>, name: &str) -> Result<SourceCapture, Diagnostic> {
    match ctx.argument(name) {
        Some(SemanticValue::Source(value)) => Ok(value.clone()),
        _ => Err(Diagnostic::error(
            "prototype.argument",
            format!("Expected capture {name}"),
            ctx.occurrence.span.clone(),
        )),
    }
}
fn identity(ctx: &OperationContext<'_>, role: &str) -> Option<usize> {
    match ctx.attachment(ctx.occurrence.id, &format!("std/variables#{role}")) {
        Some(SemanticValue::Record(id)) => Some(*id),
        _ => None,
    }
}
impl NativeOperation for TypeOperation {
    fn before(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let kind = self.0;
        ctx.with_service::<Constraints, _>(MODULE, |state, ctx| {
            if !matches!(kind, Kind::Configure) && state.unknown_symbol.is_none() {
                return Err(Diagnostic::error("types.configuration", "Configure the language's unknown and function symbols with types(unknown, function) before type meanings", ctx.occurrence.span.clone()));
            }
            let own = state.expression(ctx.occurrence.id);
            let span = &ctx.occurrence.span;
            match kind {
                Kind::Configure => {
                    if let (
                        Some(SemanticValue::Symbol(unknown)),
                        Some(SemanticValue::Symbol(function)),
                    ) = (ctx.argument("unknown"), ctx.argument("function"))
                    {
                        state.unknown_symbol = Some(unknown.clone());
                        state.function_symbol = Some(function.clone());
                    }
                }
                Kind::Type => {
                    if let Some(SemanticValue::Symbol(ty)) = ctx.argument("value") {
                        state.equal(
                            own,
                            if state.unknown_symbol.as_ref() == Some(ty) {
                                Type::Unknown
                            } else {
                                Type::Primitive(ty.clone())
                            },
                            span,
                        );
                    }
                }
                Kind::Alias => {
                    let source = capture(ctx, "value")?;
                    if let Some(value) = state.capture(&source) {
                        state.equal(own, value, &source.span);
                    }
                }
                Kind::Inherit => {
                    if let Some(child) = ctx.occurrence.children.first() {
                        let value = state.expression(child.id);
                        state.equal(own, value, span);
                    }
                }
                Kind::Declaration => {
                    if let Some(id) = identity(ctx, "declaration") {
                        let binding = state.binding(id);
                        state.equal(own, binding, span);
                    }
                }
                Kind::Binding => {
                    if let Some(id) = identity(ctx, "declaration") {
                        let binding = state.binding(id);
                        state.equal(own.clone(), binding, span);
                    }
                    let source = capture(ctx, "value")?;
                    if let Some(value) = state.capture(&source) {
                        state.equal(own, value, &source.span);
                    }
                }
                Kind::Reference => {
                    if let Some(id) = identity(ctx, "reference") {
                        let binding = state.binding(id);
                        state.equal(own, binding, span);
                    }
                }
                Kind::Function => {
                    let params = state.captures(&capture(ctx, "parameters")?);
                    let body = capture(ctx, "body")?;
                    let result = state.capture(&body).unwrap_or_else(|| state.fresh());
                    let annotation = capture(ctx, "annotation")?;
                    if let Some(annotation_type) = state.capture(&annotation) {
                        state.equal(result.clone(), annotation_type, &annotation.span);
                    }
                    state.equal(own, Type::Function(params, Box::new(result)), span);
                }
                Kind::Call => {
                    let callee = capture(ctx, "callee")?;
                    if let Some(callee_type) = state.capture(&callee) {
                        let args = state.captures(&capture(ctx, "arguments")?);
                        state.equal(callee_type, Type::Function(args, Box::new(own)), span);
                    }
                }
                Kind::TypeOf => {}
            }
            Ok(())
        })
        .unwrap_or_else(|| {
            Err(Diagnostic::error(
                "prototype.service",
                "Missing type analysis service",
                ctx.occurrence.span.clone(),
            ))
        })
    }
    fn evaluate(&mut self, ctx: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        if !matches!(self.0, Kind::TypeOf) {
            return Ok(Evaluation::Ready(SemanticValue::Absent));
        }
        let source = capture(ctx, "value")?;
        let configured = ctx
            .with_service::<Constraints, _>(MODULE, |state, _| {
                state.unknown_symbol.is_some() && state.function_symbol.is_some()
            })
            .unwrap_or(false);
        if !configured {
            return Err(Diagnostic::error(
                "types.configuration",
                "Configure type symbols before typeof",
                ctx.occurrence.span.clone(),
            ));
        }
        Ok(ctx
            .with_service::<Constraints, _>(MODULE, |state, _ctx| {
                let Some(ty) = state.capture(&source) else {
                    return Evaluation::Ready(SemanticValue::Symbol(
                        state.unknown_symbol.clone().unwrap_or_default(),
                    ));
                };
                match state.resolve(&ty) {
                    Type::Var(_) if !state.settled => Evaluation::Pending,
                    Type::Var(_) | Type::Unknown => Evaluation::Ready(SemanticValue::Symbol(
                        state.unknown_symbol.clone().unwrap_or_default(),
                    )),
                    Type::Primitive(name) => Evaluation::Ready(SemanticValue::Symbol(name)),
                    Type::Function(_, _) => Evaluation::Ready(SemanticValue::Symbol(
                        state.function_symbol.clone().unwrap_or_default(),
                    )),
                }
            })
            .unwrap_or(Evaluation::Pending))
    }
}
pub fn engine() -> SemanticEngine {
    let mut engine = SemanticEngine::default();
    engine
        .registry_mut()
        .register_service(MODULE, || Box::<Constraints>::default())
        .unwrap();
    let operations = [
        (
            "types",
            Kind::Configure,
            vec![
                ("unknown", ParameterKind::Symbol),
                ("function", ParameterKind::Symbol),
            ],
        ),
        ("type", Kind::Type, vec![("value", ParameterKind::Symbol)]),
        (
            "typeof",
            Kind::TypeOf,
            vec![("value", ParameterKind::Capture)],
        ),
        (
            "sameType",
            Kind::Alias,
            vec![("value", ParameterKind::Capture)],
        ),
        ("inheritType", Kind::Inherit, vec![]),
        ("declarationType", Kind::Declaration, vec![]),
        (
            "bindingType",
            Kind::Binding,
            vec![("value", ParameterKind::Capture)],
        ),
        ("referenceType", Kind::Reference, vec![]),
        (
            "functionType",
            Kind::Function,
            vec![
                ("parameters", ParameterKind::Capture),
                ("body", ParameterKind::Capture),
                ("annotation", ParameterKind::Capture),
            ],
        ),
        (
            "callType",
            Kind::Call,
            vec![
                ("callee", ParameterKind::Capture),
                ("arguments", ParameterKind::Capture),
            ],
        ),
    ];
    for (name, kind, params) in operations {
        engine
            .registry_mut()
            .register(
                MODULE,
                name,
                OperationSignature {
                    id: symbol(name),
                    provides_context: matches!(kind, Kind::Configure),
                    parameters: params
                        .into_iter()
                        .map(|(name, kind)| OperationParameter {
                            name: name.into(),
                            kind,
                            default: None,
                        })
                        .collect(),
                },
                move || Box::new(TypeOperation(kind)),
            )
            .unwrap();
    }
    engine
}
