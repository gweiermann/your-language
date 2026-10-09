use crate::{
    compiler::validate_language,
    diagnostic::{CompileResult, Diagnostic, Span},
    CompiledLanguage,
};

impl CompiledLanguage {
    /// Deterministic versioned JSON artifact. It contains no YL source or imports.
    pub fn to_bytes(&self) -> CompileResult<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| {
            vec![Diagnostic::error(
                "yl.artifact",
                e.to_string(),
                Span::default(),
            )]
        })
    }
}
pub fn load_compiled_language(bytes: &[u8]) -> CompileResult<CompiledLanguage> {
    let language: CompiledLanguage = serde_json::from_slice(bytes).map_err(|e| {
        vec![Diagnostic::error(
            "yl.artifact",
            e.to_string(),
            Span::default(),
        )]
    })?;
    validate_language(&language).map_err(|e| vec![e])?;
    Ok(language)
}
