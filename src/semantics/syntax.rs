//! Original syntax-occurrence identities, independent of name/binding libraries.
use super::*;
struct Identity {
    capture: bool,
    list: bool,
    child: bool,
}
impl NativeOperation for Identity {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let identity = |id| SemanticValue::Identity(SemanticIdentity::Occurrence(id));
        let value = if self.child {
            context
                .occurrence
                .children
                .first()
                .map(|child| identity(child.id))
                .unwrap_or(SemanticValue::Absent)
        } else if !self.capture {
            identity(context.occurrence.id)
        } else {
            match context.argument("value") {
            Some(SemanticValue::Absent)=>SemanticValue::Absent,
            Some(SemanticValue::Source(source)) if matches!(source.value,AstValue::None)=>SemanticValue::Absent,
            Some(SemanticValue::Source(source)) if self.list=>SemanticValue::List(source.occurrences.iter().map(|id|identity(*id)).collect()),
            Some(SemanticValue::Source(source)) if source.occurrences.len()==1=>identity(source.occurrences[0]),
            Some(SemanticValue::Identity(SemanticIdentity::Occurrence(id)))=>identity(*id),
            Some(SemanticValue::Source(source))=>return Err(Diagnostic::error("syntax.identity","Expected one retained syntax occurrence; use identities(value) for a repeated capture",source.span.clone())),
            _=>return Err(Diagnostic::error("syntax.identity","Expected a syntax capture or occurrence identity",context.occurrence.span.clone())),
        }
        };
        Ok(Evaluation::Ready(value))
    }
}
pub fn register(registry: &mut OperationRegistry) -> Result<(), String> {
    for (name, capture, list, child) in [
        ("identity", false, false, false),
        ("identityOf", true, false, false),
        ("identities", true, true, false),
        ("childIdentity", false, false, true),
    ] {
        registry.register(
            "std/syntax",
            name,
            OperationSignature {
                id: format!("std/syntax#{name}"),
                provides_context: false,
                finalization_only: false,
                contribution: false,
                parameters: if capture {
                    vec![OperationParameter {
                        name: "value".into(),
                        kind: ParameterKind::Value,
                        default: None,
                    }]
                } else {
                    vec![]
                },
            },
            move || {
                Box::new(Identity {
                    capture,
                    list,
                    child,
                })
            },
        )?;
    }
    Ok(())
}
