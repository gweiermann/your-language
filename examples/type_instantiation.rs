//! Explicitly owned type templates through the experimental public host API.
use std::collections::BTreeMap;
use your_language::{
    diagnostic::{Diagnostic, Severity},
    diagnostic_render::print_diagnostics,
    semantics::{
        types, NativeOperation, OperationContext, OperationParameter, OperationSignature,
        ParameterKind, SemanticEngine, SemanticIdentity, SemanticKey, SemanticValue,
    },
};

struct Template;
impl NativeOperation for Template {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let Some(SemanticValue::Identity(local)) = context.argument("local") else {
            return Err(Diagnostic::error(
                "example.local",
                "Expected a local identity",
                context.operation_span.clone(),
            ));
        };
        let locals = vec![local.clone()];
        types::register_template(
            context,
            SemanticIdentity::Occurrence(context.occurrence.id),
            &locals,
        )
    }
}
struct Show;
impl NativeOperation for Show {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let value = context
            .argument("value")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        let name = match context.argument("name") {
            Some(SemanticValue::Text(name)) => name.clone(),
            _ => "instance".into(),
        };
        context.attach(&format!("example#{name}"), value);
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");
    let errors = args.iter().any(|arg| arg == "errors");
    let mut engine = SemanticEngine::default();
    for (name, parameters, contribution) in [
        ("template", vec!["local"], true),
        ("show", vec!["name", "value"], false),
    ] {
        engine.registry_mut().register(
            "host",
            name,
            OperationSignature {
                id: format!("host#{name}"),
                provides_context: false,
                finalization_only: true,
                contribution,
                parameters: parameters
                    .into_iter()
                    .map(|name| OperationParameter {
                        name: name.into(),
                        kind: if name == "name" {
                            ParameterKind::Text
                        } else {
                            ParameterKind::Value
                        },
                        default: None,
                    })
                    .collect(),
            },
            move || {
                if name == "template" {
                    Box::new(Template) as Box<dyn NativeOperation>
                } else {
                    Box::new(Show)
                }
            },
        )?;
    }
    let definition = include_str!("type_instantiation/definition/main.yl");
    let sources = BTreeMap::from([("main.yl".into(), definition.into())]);
    let language = engine
        .compile("main.yl", &sources)
        .map_err(|errors| format!("{errors:?}"))?;
    let language = engine
        .load(
            &language
                .to_bytes()
                .map_err(|errors| format!("{errors:?}"))?,
        )
        .map_err(|errors| format!("{errors:?}"))?;
    let source = if errors {
        include_str!("type_instantiation/programs/errors.txt")
    } else {
        include_str!("type_instantiation/programs/program.txt")
    };
    let result = engine.analyze(&language, "program", source);
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_diagnostics(&result.diagnostics, |span| {
            if span.file == "program" {
                Some(source.into())
            } else {
                sources.get(&span.file).cloned()
            }
        });
        for attachment in &result.graph.attachments {
            if !attachment.key.starts_with("example#") {
                continue;
            }
            let key = SemanticValue::Tuple(vec![
                attachment.value.clone(),
                SemanticValue::Symbol("main.yl#output".into()),
            ]);
            if let Some(row) = result.graph.records.iter().find(|row| {
                row.entries
                    .iter()
                    .any(|(k, v)| *k == SemanticKey::Symbol("std/types#key".into()) && *v == key)
            }) {
                if let Some((_, value)) = row
                    .entries
                    .iter()
                    .find(|(key, _)| *key == SemanticKey::Symbol("std/types#type".into()))
                {
                    println!(
                        "{}: {}",
                        attachment.key.trim_start_matches("example#"),
                        serde_json::to_string(value)?
                    );
                }
            }
        }
    }
    if result
        .diagnostics
        .iter()
        .any(|item| item.severity == Severity::Error)
    {
        std::process::exit(1);
    }
    Ok(())
}
