//! Retain highlighting and example templates without introducing runtime grammar sugar.

use super::*;

impl Compiler {
    /// Retain highlighting and example templates without introducing runtime grammar sugar.
    pub(super) fn lower_tooling(&mut self, language: &mut CompiledLanguage) -> Result<()> {
        for (id, symbol) in &self.symbols {
            if let DeclKind::Node { ide: Some(ide), .. }
            | DeclKind::Pattern { ide: Some(ide), .. } = &symbol.declaration.kind
            {
                if let Some(category) = &ide.highlight_as {
                    language.highlights.insert(id.clone(), category.clone());
                }
            }
        }
        // Pattern surface sugar is retained only as tooling templates, never runtime rules.
        for (id, symbol) in self.symbols.clone() {
            let (term, example) = match &symbol.declaration.kind {
                DeclKind::Node { example, .. } => (Term::Ref(id.clone()), example.clone()),
                DeclKind::Pattern {
                    parameters,
                    grammar,
                    example,
                    ..
                } if parameters.is_empty() => (
                    self.lower(
                        &symbol.module,
                        &symbol.name,
                        grammar,
                        &GrammarBindings::new(),
                        0,
                    )?,
                    example.clone(),
                ),
                DeclKind::Pattern {
                    example: Some(example),
                    ..
                } => {
                    return Err(Diagnostic::error(
                        "yl.example_parameters",
                        "Experimental examples require a parameterless pattern",
                        example.span.clone(),
                    ))
                }
                _ => continue,
            };
            // Keep node-only artifacts unchanged unless metadata is supplied.
            if example.is_some() || matches!(symbol.declaration.kind, DeclKind::Pattern { .. }) {
                language.examples.insert(
                    id,
                    crate::examples::ExampleDefinition {
                        term,
                        explicit: example,
                        span: symbol.declaration.span.clone(),
                    },
                );
            }
        }
        Ok(())
    }
}
