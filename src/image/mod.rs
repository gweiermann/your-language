//! Immutable, indexed executable language image; mutable state belongs to parse sessions.
pub mod parser;
pub mod query;

use crate::{
    diagnostic::{Diagnostic, Span},
    examples::Preview,
    ir::CompiledLanguage,
    semantics::MeaningDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT_VERSION: u32 = 10;
const CAPABILITIES: &[&str] = &[
    "indexed-parser",
    "indexed-checks",
    "indexed-queries",
    "semantic-scheduling",
    "positional-index",
    "recursive-query-functions",
    "value-match",
    "ide-highlighting",
    "host-backend-hooks",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageImage {
    pub version: u32,
    pub capabilities: Vec<String>,
    pub backend: crate::backend::BackendImage,
    pub parser: parser::ParserImage,
    pub queries: query::QueryImage,
    /// Compiled scheduling selectors and external native operation contracts.
    pub semantics: BTreeMap<String, MeaningDefinition>,
    /// Validated, bounded representative strings, prepared once by the compiler.
    pub previews: Vec<Preview>,
}
impl LanguageImage {
    pub(crate) fn compile(language: &CompiledLanguage) -> Result<Self, Diagnostic> {
        let image = Self {
            version: FORMAT_VERSION,
            backend: language.backend.clone(),
            capabilities: CAPABILITIES.iter().map(|s| (*s).to_owned()).collect(),
            parser: parser::ParserImage::compile(language)?,
            queries: query::QueryImage::compile(language)?,
            semantics: language.meanings.clone(),
            previews: language.example_previews(),
        };
        image.validate()?;
        Ok(image)
    }
    pub(crate) fn validate(&self) -> Result<(), Diagnostic> {
        if self.version != FORMAT_VERSION {
            return Err(invalid(
                "Unsupported executable image version; recompile the YL definition",
            ));
        }
        if self.capabilities.len() != CAPABILITIES.len()
            || CAPABILITIES
                .iter()
                .any(|required| !self.capabilities.iter().any(|v| v == required))
        {
            return Err(invalid(
                "Unsupported or missing executable image capability",
            ));
        }
        for (definition, category) in &self.parser.highlights {
            if definition.is_empty() || !crate::syntax::HIGHLIGHT_TYPES.contains(&category.as_str())
            {
                return Err(invalid("Invalid IDE highlighting metadata"));
            }
        }
        self.parser.validate()?;
        self.queries.validate()?;
        let captures = self.parser.capture_sets();
        self.backend.validate(&captures)?;
        self.queries.validate_owner_captures(&captures)?;
        for (owner, definition) in &self.semantics {
            let available = captures.get(owner).ok_or_else(|| {
                invalid("Semantic metadata has an unknown parser occurrence owner")
            })?;
            crate::compiler::validate_meaning_captures(definition, available)?;
        }
        for instruction in &self.parser.instructions {
            if let parser::Instruction::Meaning { definition, .. } = instruction {
                if !self.semantics.contains_key(definition) {
                    return Err(invalid("Parser occurrence has no semantic metadata"));
                }
            }
        }
        let mut previews = std::collections::BTreeSet::new();
        for preview in &self.previews {
            if !previews.insert(&preview.definition)
                || !self.parser.preview_roots.contains_key(&preview.definition)
            {
                return Err(invalid("Invalid example preview metadata"));
            }
        }
        if previews.len() != self.parser.preview_roots.len() {
            return Err(invalid("Missing example preview metadata"));
        }
        for instruction in &self.parser.instructions {
            if let parser::Instruction::Example { definition, .. } = instruction {
                if !self.parser.preview_roots.contains_key(definition)
                    && !self.parser.highlights.contains_key(definition)
                {
                    return Err(invalid("Unknown example metadata reference"));
                }
            }
        }
        crate::compiler::validate_meaning_definitions(&self.semantics, |id| {
            self.parser.rule_id(id).is_some()
        })?;
        for (owner, groups) in &self.queries.groups {
            let definition = self
                .semantics
                .get(owner)
                .ok_or_else(|| invalid("Query groups have no scheduling owner"))?;
            let mut seen = std::collections::BTreeSet::new();
            for group in groups {
                let matching: Vec<_> = definition
                    .groups
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| slot.name == group.name && slot.span == group.span)
                    .collect();
                if matching.len() != 1
                    || !seen.insert(matching[0].0)
                    || !matching[0].1.calls.is_empty()
                    || matching[0].1.matching.is_some()
                {
                    return Err(invalid(
                        "Query group has an invalid or ambiguous scheduling placeholder",
                    ));
                }
            }
        }
        Ok(())
    }
    /// Verify untrusted representative text against executable parser roots.
    pub(crate) fn validate_previews(&self) -> Result<(), Diagnostic> {
        for preview in &self.previews {
            if let Some(text) = &preview.text {
                let root = self.parser.preview_roots[&preview.definition];
                if !crate::runtime::accepts_preview(&self.parser, root, text) {
                    return Err(invalid(
                        "Example preview does not parse its compiled grammar",
                    ));
                }
            }
        }
        Ok(())
    }
}
pub(crate) fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error("yl.artifact", message, Span::default())
}
