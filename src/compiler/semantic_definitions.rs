//! Compile semantic groups and preserve named pattern occurrences needed by scheduling.

use super::*;

impl Compiler {
    /// Compile semantic groups and preserve named pattern occurrences needed by scheduling.
    pub(super) fn lower_semantic_definitions(
        &mut self,
        language: &mut CompiledLanguage,
    ) -> Result<()> {
        for (id, symbol) in &self.symbols {
            let meanings = match &symbol.declaration.kind {
                DeclKind::Node { meanings, .. } | DeclKind::Pattern { meanings, .. } => meanings,
                _ => continue,
            };
            if let Some(meanings) = meanings {
                let query_groups = self.lower_query_meanings(symbol, meanings)?;
                if !query_groups.is_empty() {
                    language.query_meanings.insert(id.clone(), query_groups);
                }
                language
                    .meanings
                    .insert(id.clone(), self.lower_meanings(symbol, meanings)?);
            }
        }
        // Every named pattern is a real occurrence level in semantic languages,
        // including wrappers not themselves mentioned by any selector. Syntax-only
        // artifacts retain their existing sugar-free representation.
        if !language.meanings.is_empty() {
            for (id, symbol) in &self.symbols {
                if matches!(symbol.declaration.kind, DeclKind::Pattern { .. }) {
                    self.selected_patterns.insert(id.clone());
                    language.meanings.entry(id.clone()).or_default();
                }
            }
        }
        Ok(())
    }
}
