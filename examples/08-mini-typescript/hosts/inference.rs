//! Presentation only: all reference and typing rules live in the YL modules.
use your_language::semantics::{queries::QueryKey, AnalysisResult, SemanticEngine, SemanticValue};

pub fn engine() -> SemanticEngine {
    SemanticEngine::default()
}

fn symbol_name(value: &str) -> String {
    value.rsplit('#').next().unwrap_or(value).into()
}

fn query(result: &AnalysisResult, key: &QueryKey) -> Vec<SemanticValue> {
    let mut values = Vec::new();
    for entry in &result.graph.query_entries {
        if &entry.key == key && !values.contains(&entry.value) {
            values.push(entry.value.clone());
        }
    }
    values
}

fn type_values(result: &AnalysisResult, key: QueryKey) -> Vec<SemanticValue> {
    let mut values = query(
        result,
        &QueryKey::Tuple(vec![key, QueryKey::Symbol("std/types#type".into())]),
    );
    if values.len() > 1 {
        values.retain(|value| *value != SemanticValue::Symbol("std/types#unknown".into()));
    }
    values
}

fn type_text(result: &AnalysisResult, values: &[SemanticValue], depth: usize) -> String {
    if depth > 12 {
        return "unknown".into();
    }
    let mut names: Vec<_> = values
        .iter()
        .map(|value| match value {
            SemanticValue::Symbol(symbol) => symbol_name(symbol),
            SemanticValue::Identity(identity) => {
                let binding = QueryKey::Identity(identity.clone());
                let parameters = type_values(
                    result,
                    QueryKey::Tuple(vec![
                        binding.clone(),
                        QueryKey::Symbol("definition/types.yl#parameters".into()),
                    ]),
                );
                let parameters = parameters
                    .first()
                    .and_then(|value| match value {
                        SemanticValue::List(values) => Some(values),
                        _ => None,
                    })
                    .map(|values| {
                        values
                            .iter()
                            .map(|pair| match pair {
                                SemanticValue::Tuple(pair) if pair.len() == 2 => {
                                    type_text(result, &pair[1..], depth + 1)
                                }
                                _ => "unknown".into(),
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let output = type_values(
                    result,
                    QueryKey::Tuple(vec![
                        binding,
                        QueryKey::Symbol("definition/types.yl#returnType".into()),
                    ]),
                );
                format!(
                    "({parameters}) => {}",
                    type_text(result, &output, depth + 1)
                )
            }
            SemanticValue::Alternatives { values, .. } => type_text(result, values, depth + 1),
            _ => "unknown".into(),
        })
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "unknown".into()
    } else {
        names.join(" | ")
    }
}

/// Inferred declaration types, in source order. This performs no inference.
pub fn bindings(result: &AnalysisResult) -> Vec<(String, String)> {
    let mut declarations = result
        .graph
        .query_entries
        .iter()
        .filter_map(|entry| match (&entry.key, &entry.value) {
            (QueryKey::Tuple(parts), SemanticValue::Identity(identity))
                if parts.len() == 3
                    && parts[1] == QueryKey::Symbol("std/references#declarations".into()) =>
            {
                match &parts[2] {
                    QueryKey::Text(name) => {
                        Some((entry.span.start, name.clone(), identity.clone()))
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    declarations.sort_by_key(|(start, _, _)| *start);
    declarations
        .into_iter()
        .map(|(_, name, identity)| {
            let values = type_values(result, QueryKey::Identity(identity));
            (name, type_text(result, &values, 0))
        })
        .collect()
}

#[allow(dead_code)]
pub fn binding_types(result: &AnalysisResult, name: &str) -> Vec<String> {
    bindings(result)
        .into_iter()
        .filter_map(|(binding, ty)| (binding == name).then_some(ty))
        .collect()
}
