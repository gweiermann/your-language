use crate::{
    diagnostic::{CompileResult, Diagnostic, Span},
    image::{LanguageImage, FORMAT_VERSION},
    CompiledLanguage,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

impl Serialize for CompiledLanguage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(image) = &self.image {
            image.serialize(serializer)
        } else {
            LanguageImage::compile(self)
                .map_err(|error| serde::ser::Error::custom(error.message))?
                .serialize(serializer)
        }
    }
}
impl CompiledLanguage {
    /// Whether this artifact contains declarative attachment queries.
    pub fn has_declarative_meanings(&self) -> bool {
        self.image.as_ref().map_or_else(
            || !self.query_meanings.is_empty(),
            |image| !image.queries.groups.is_empty(),
        )
    }
    /// Non-fatal definition diagnostics produced while compiling this language.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Immutable executable tables; source normalization IR is never serialized.
    pub fn executable_image(&self) -> &LanguageImage {
        self.image
            .as_deref()
            .expect("public compilation and loading always prepare an executable image")
    }
    /// Deterministic, versioned JSON encoding of the executable image.
    pub fn to_bytes(&self) -> CompileResult<Vec<u8>> {
        let mut bytes = vec![];
        let mut serializer = serde_json::Serializer::pretty(&mut bytes);
        self.serialize(serde_stacker::Serializer::new(&mut serializer))
            .map_err(|error| vec![invalid(error.to_string())])?;
        Ok(bytes)
    }
}
fn invalid(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("yl.artifact", message, Span::default())
}
pub fn load_compiled_language(bytes: &[u8]) -> CompileResult<CompiledLanguage> {
    // Bound allocation and recursion before handing untrusted bytes to Serde.
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(vec![invalid(
            "Executable image exceeds the 64 MiB size limit",
        )]);
    }
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
                        return Err(vec![invalid("Artifact JSON exceeds the nesting limit")]);
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    // Reject retired formats explicitly rather than mistaking their fields for a
    // partial executable image. No parser/query IR is rebuilt from old artifacts.
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let mut header_decoder = serde_json::Deserializer::from_slice(bytes);
    header_decoder.disable_recursion_limit();
    let header = Header::deserialize(serde_stacker::Deserializer::new(&mut header_decoder))
        .map_err(|error| vec![invalid(error.to_string())])?;
    if header.version != FORMAT_VERSION {
        return Err(vec![invalid(
            "Unsupported executable image version; recompile the YL definition",
        )]);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let mut image = LanguageImage::deserialize(serde_stacker::Deserializer::new(&mut deserializer))
        .map_err(|error| vec![invalid(error.to_string())])?;
    deserializer
        .end()
        .map_err(|error| vec![invalid(error.to_string())])?;
    image.validate().map_err(|error| vec![error])?;
    image.parser.prepare().map_err(|error| vec![error])?;
    image.validate_previews().map_err(|error| vec![error])?;
    Ok(CompiledLanguage {
        version: FORMAT_VERSION,
        entry: image.parser.rules[image.parser.entry as usize]
            .definition
            .clone(),
        rules: BTreeMap::new(),
        trivia: vec![],
        query_meanings: BTreeMap::new(),
        query_functions: BTreeMap::new(),
        examples: BTreeMap::new(),
        meanings: image.semantics.clone(),
        diagnostics: vec![],
        image: Some(Box::new(image)),
    })
}
