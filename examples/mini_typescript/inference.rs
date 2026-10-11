//! Throwaway monomorphic constraint inference. No TypeScript compatibility claim.
use std::collections::BTreeMap;
use your_language::{
    diagnostic::{Diagnostic, Span},
    semantics::{
        NativeOperation, Occurrence, OperationContext, OperationSignature, SemanticEngine,
        SemanticKey, SemanticValue,
    },
};

const MODULE: &str = "prototype/types";
fn key(name: &str) -> SemanticKey {
    SemanticKey::Symbol(format!("{MODULE}#{name}"))
}

pub fn engine() -> SemanticEngine {
    let mut engine = SemanticEngine::default();
    engine
        .registry_mut()
        .register(
            MODULE,
            "solve",
            OperationSignature {
                id: format!("{MODULE}#solve"),
                parameters: vec![],
                provides_context: false,
            },
            || Box::new(Solve),
        )
        .unwrap();
    engine
}
#[derive(Clone, Debug, PartialEq)]
enum Type {
    Var(usize),
    Primitive(&'static str),
    Function(Vec<Type>, Box<Type>),
}
impl Type {
    fn display(&self) -> String {
        match self {
            Self::Var(_) => "unknown".into(),
            Self::Primitive(name) => (*name).into(),
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
    fn collect(&mut self, node: &Occurrence, context: &OperationContext<'_>) {
        for child in &node.children {
            self.collect(child, context);
        }
        let ty = self.expression(node.id);
        let linked =
            |kind: &str| match context.attachment(node.id, &format!("std/semantics#{kind}")) {
                Some(SemanticValue::Record(id)) => Some(*id),
                _ => None,
            };
        match role(node) {
            "Number" | "String" | "Boolean" => {
                let primitive = match role(node) {
                    "Number" => "number",
                    "String" => "string",
                    _ => "boolean",
                };
                self.equal(ty, Type::Primitive(primitive), &node.span);
            }
            "Reference" => {
                if let Some(id) = linked("reference") {
                    let binding = self.binding(id);
                    self.equal(ty, binding, &node.span);
                }
            }
            "Parameter" | "LetDeclaration" => {
                if let Some(id) = linked("declaration") {
                    let binding = self.binding(id);
                    if let Some(initializer) = captured(node, "initializer") {
                        let init = self.expression(initializer.id);
                        self.equal(binding.clone(), init, &initializer.span);
                    }
                    self.annotation(node, binding);
                }
            }
            "FunctionDeclaration" => {
                if let (Some(id), Some(contents)) =
                    (linked("declaration"), descendant(node, "FunctionContents"))
                {
                    let params = contents
                        .children
                        .iter()
                        .filter(|child| role(child) == "Parameter")
                        .filter_map(|param| {
                            match context.attachment(param.id, "std/semantics#declaration") {
                                Some(SemanticValue::Record(id)) => Some(self.binding(*id)),
                                _ => None,
                            }
                        })
                        .collect();
                    let result = descendant(contents, "FunctionBody")
                        .and_then(|body| captured(body, "result"))
                        .map(|result| self.expression(result.id))
                        .unwrap_or_else(|| self.fresh());
                    self.annotation(contents, result.clone());
                    let binding = self.binding(id);
                    self.equal(
                        binding,
                        Type::Function(params, Box::new(result)),
                        &node.span,
                    );
                }
            }
            "Call" => {
                if let Some(callee) = captured(node, "callee") {
                    let args = node
                        .children
                        .iter()
                        .filter(|child| role(child) == "Expression")
                        .map(|arg| self.expression(arg.id))
                        .collect();
                    let callee = self.expression(callee.id);
                    self.equal(callee, Type::Function(args, Box::new(ty)), &node.span);
                }
            }
            "Expression" => {
                if let Some(child) = node.children.first() {
                    let child = self.expression(child.id);
                    self.equal(ty, child, &node.span);
                }
            }
            _ => {}
        }
    }
    fn annotation(&mut self, node: &Occurrence, ty: Type) {
        if let Some(annotation) = captured(node, "annotation") {
            let name = annotation
                .captures
                .get("name")
                .map(|capture| capture.text.as_str());
            let primitive = match name {
                Some("number") => "number",
                Some("string") => "string",
                Some("boolean") => "boolean",
                _ => return,
            };
            self.equal(ty, Type::Primitive(primitive), &annotation.span);
        }
    }
}
fn role(node: &Occurrence) -> &str {
    node.definition
        .rsplit('#')
        .next()
        .unwrap_or(&node.definition)
}
fn descendant<'a>(node: &'a Occurrence, name: &str) -> Option<&'a Occurrence> {
    node.children
        .iter()
        .find(|child| role(child) == name)
        .or_else(|| {
            node.children
                .iter()
                .find_map(|child| descendant(child, name))
        })
}
fn captured<'a>(node: &'a Occurrence, name: &str) -> Option<&'a Occurrence> {
    let span = &node.captures.get(name)?.span;
    fn find<'a>(node: &'a Occurrence, span: &Span) -> Option<&'a Occurrence> {
        node.children
            .iter()
            .find(|child| child.span == *span)
            .or_else(|| node.children.iter().find_map(|child| find(child, span)))
    }
    find(node, span)
}
struct Solve;
impl NativeOperation for Solve {
    fn after(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let mut constraints = Constraints::default();
        constraints.collect(context.occurrence, context);
        for (a, b, span) in std::mem::take(&mut constraints.equalities) {
            // A failed equation does not roll back successful components; this is best effort.
            if let Err(message) = constraints.unify(a, b) {
                context.emit(Diagnostic::error("prototype.type_mismatch", message, span));
            }
        }
        for (id, ty) in &constraints.bindings {
            context.set(
                *id,
                key("inferredType"),
                SemanticValue::Text(constraints.resolve(ty).display()),
            )?;
        }
        let report = context.record();
        for (id, ty) in &constraints.expressions {
            context.set(
                report,
                SemanticKey::Text(id.to_string()),
                SemanticValue::Text(constraints.resolve(ty).display()),
            )?;
        }
        context.attach(
            &format!("{MODULE}#expressionTypes"),
            SemanticValue::Record(report),
        );
        Ok(())
    }
}
