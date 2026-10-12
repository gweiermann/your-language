//! The consumer selects native standard libraries; typing rules are authored in YL.
use your_language::semantics::{SemanticEngine, SemanticKey, SemanticRecord, SemanticValue};
pub fn engine() -> SemanticEngine {
    SemanticEngine::default()
}
/// Presentation for this example's authored Function/parameters/returnType schema.
/// This is not a typing rule or part of the generic type library.
pub fn display_type(record: &SemanticRecord) -> Option<String> {
    let text = |name: &str| {
        record
            .entries
            .iter()
            .find_map(|(key, value)| match (key, value) {
                (SemanticKey::Symbol(key), SemanticValue::Text(value)) if key == name => {
                    Some(value.clone())
                }
                _ => None,
            })
    };
    let value = text("std/types#inferredType")?;
    if value == "Function" {
        let parameters = text("definition/types.yl#parameters").unwrap_or_else(|| "[]".into());
        let result = text("definition/types.yl#returnType").unwrap_or_else(|| "unknown".into());
        let alternatives: Vec<_> = parameters
            .split(" | ")
            .map(|value| value.trim_start_matches('[').trim_end_matches(']'))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let parameters = if alternatives.iter().all(|value| !value.contains(',')) {
            alternatives.join(" | ")
        } else if alternatives.len() == 1 {
            alternatives[0].into()
        } else {
            alternatives
                .iter()
                .map(|value| format!("({value})"))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        Some(format!("({parameters}) => {result}"))
    } else {
        Some(value)
    }
}
