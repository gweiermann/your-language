use crate::{
    compiler::validate_language,
    diagnostic::{CompileResult, Diagnostic, Span},
    CompiledLanguage,
};
use serde::{Deserialize, Serialize};

impl CompiledLanguage {
    /// Whether this artifact contains declarative attachment queries.
    pub fn has_declarative_meanings(&self) -> bool {
        !self.query_meanings.is_empty()
    }
    /// Non-fatal definition diagnostics produced while compiling this language.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Deterministic versioned JSON artifact. It contains no YL source or imports.
    pub fn to_bytes(&self) -> CompileResult<Vec<u8>> {
        let mut bytes = vec![];
        let mut serializer = serde_json::Serializer::pretty(&mut bytes);
        self.serialize(serde_stacker::Serializer::new(&mut serializer))
            .map_err(|e| {
                vec![Diagnostic::error(
                    "yl.artifact",
                    e.to_string(),
                    Span::default(),
                )]
            })?;
        Ok(bytes)
    }
}
pub fn load_compiled_language(bytes: &[u8]) -> CompileResult<CompiledLanguage> {
    // Bound the wire representation before any recursive Serde allocation.
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 512 {
                        return Err(vec![Diagnostic::error(
                            "yl.artifact",
                            "Artifact JSON exceeds the nesting limit",
                            Span::default(),
                        )]);
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let language =
        CompiledLanguage::deserialize(serde_stacker::Deserializer::new(&mut deserializer))
            .map_err(|e| {
                vec![Diagnostic::error(
                    "yl.artifact",
                    e.to_string(),
                    Span::default(),
                )]
            })?;
    deserializer.end().map_err(|error| {
        vec![Diagnostic::error(
            "yl.artifact",
            error.to_string(),
            Span::default(),
        )]
    })?;
    validate_language(&language).map_err(|e| vec![e])?;
    Ok(language)
}
