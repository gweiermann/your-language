//! Query matches, argument binding, and diagnostic recognition.

use super::*;

impl Compiler {
    pub(super) fn validate_value_match(
        &self,
        symbol: &DeclarationSymbol,
        arity: usize,
        cases: &[ValueMatchCase],
        span: &Span,
    ) -> Result<()> {
        use crate::semantics::queries::QueryExpr as Q;
        let bad =
            |message: &str| Diagnostic::error("yl.match_exhaustiveness", message, span.clone());
        if arity == 0 || cases.is_empty() {
            return Err(bad("A value match needs inputs and cases"));
        }
        let mut rows = vec![];
        let mut wildcard = false;
        for case in cases {
            for row in &case.alternatives {
                if wildcard {
                    return Err(bad("Wildcard case must be last"));
                }
                if row.len() != arity {
                    return Err(bad("Match tuple arity differs from its inputs"));
                }
                let row = row
                    .iter()
                    .map(|pattern| {
                        pattern
                            .as_ref()
                            .map(|pattern| self.query_expr_normalized(symbol, pattern))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>>>()?;
                if row.iter().flatten().any(|pattern| {
                    !matches!(pattern, Q::Bool(_) | Q::Symbol(_) | Q::Text(_) | Q::Absent)
                }) {
                    return Err(bad("Value match patterns must be constants or wildcards"));
                }
                wildcard = row.iter().all(Option::is_none);
                rows.push(row);
            }
        }
        if wildcard {
            return Ok(());
        }
        let mut combinations = vec![vec![]];
        for column in 0..arity {
            let patterns: Vec<_> = rows.iter().filter_map(|row| row[column].as_ref()).collect();
            let domain = if patterns.is_empty() {
                vec![Q::Absent]
            } else if patterns.iter().all(|pattern| matches!(pattern, Q::Bool(_))) {
                vec![Q::Bool(false), Q::Bool(true)]
            } else if let Q::Symbol(first) = patterns[0] {
                let parent = first.rsplit_once("::").map(|(parent, _)| parent);
                let variants =
                    parent
                        .and_then(|parent| self.symbols.get(parent))
                        .and_then(|symbol| match &symbol.declaration.kind {
                            DeclKind::Enum { variants, .. } => Some(variants),
                            _ => None,
                        });
                match (parent, variants) {
                    (Some(parent), Some(variants)) if patterns.iter().all(|pattern| matches!(pattern, Q::Symbol(name) if name.rsplit_once("::").is_some_and(|(owner,_)| owner == parent))) => variants.iter().map(|variant| Q::Symbol(format!("{parent}::{variant}"))).collect(),
                    _ => return Err(bad("A value match over open values requires a wildcard fallback")),
                }
            } else {
                return Err(bad(
                    "A value match over open values requires a wildcard fallback",
                ));
            };
            if domain.len().saturating_mul(combinations.len()) > 4096 {
                return Err(bad("Value match exhaustiveness exceeds the resource limit"));
            }
            combinations = combinations
                .into_iter()
                .flat_map(|prefix| {
                    domain.iter().map(move |value| {
                        let mut result = prefix.clone();
                        result.push(value.clone());
                        result
                    })
                })
                .collect();
        }
        if combinations.iter().any(|values| {
            !rows.iter().any(|row| {
                row.iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern.as_ref().is_none_or(|pattern| pattern == value))
            })
        }) {
            return Err(bad(
                "Value match does not cover every boolean or enum combination",
            ));
        }
        Ok(())
    }
    pub(super) fn query_arguments(
        &self,
        symbol: &DeclarationSymbol,
        id: &str,
        args: &[Argument],
        span: &Span,
    ) -> Result<Vec<crate::semantics::queries::QueryExpr>> {
        let contract = self.registry.signature(id).ok_or_else(|| {
            Diagnostic::error(
                "yl.unknown_operation",
                "Unknown query primitive",
                span.clone(),
            )
        })?;
        if !id.starts_with("core/query#") {
            return Err(Diagnostic::error(
                "yl.query_native_effect",
                "Declarative queries cannot call native lifecycle operations",
                span.clone(),
            ));
        }
        let mut values = BTreeMap::new();
        let mut position = 0;
        let mut named = false;
        for arg in args {
            let parameter = if let Some(name) = &arg.name {
                named = true;
                contract
                    .parameters
                    .iter()
                    .find(|parameter| &parameter.name == name)
            } else {
                if named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional argument follows named argument",
                        arg.span.clone(),
                    ));
                }
                let p = contract.parameters.get(position);
                position += 1;
                p
            }
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or excess query argument",
                    arg.span.clone(),
                )
            })?;
            if values
                .insert(
                    parameter.name.clone(),
                    self.query_expr_normalized(symbol, &arg.value)?,
                )
                .is_some()
            {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Duplicate query argument",
                    arg.span.clone(),
                ));
            }
        }
        contract
            .parameters
            .iter()
            .map(|parameter| {
                values.remove(&parameter.name).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        format!("Missing query argument {}", parameter.name),
                        span.clone(),
                    )
                })
            })
            .collect()
    }
    pub(super) fn diagnostic_severity(
        &self,
        module: &str,
        scope: &str,
        name: &str,
        span: &Span,
    ) -> Result<Option<Severity>> {
        let id = self.resolve(module, scope, name, span)?;
        Ok(match id.as_str() {
            "core/diagnostics#error" => Some(Severity::Error),
            "core/diagnostics#warn" => Some(Severity::Warning),
            "core/diagnostics#hint" => Some(Severity::Help),
            _ => None,
        })
    }
    pub(super) fn operation_actions(&self, body: &OperationBody) -> Vec<MeaningSyntaxAction> {
        match body {
            OperationBody::Actions(calls) => calls
                .iter()
                .cloned()
                .map(MeaningSyntaxAction::Call)
                .collect(),
            OperationBody::Conditional(actions) => actions.clone(),
            OperationBody::Query(_) => vec![],
        }
    }
}
