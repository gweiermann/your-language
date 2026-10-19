//! Ordered compilation phases and executable image assembly.

use super::*;

impl Compiler {
    /// Run phases in diagnostic-stable order, then validate and freeze the executable image.
    /// No phase after image construction may mutate normalized rules or query definitions.
    pub(super) fn compile(&mut self) -> Result<CompiledLanguage> {
        self.resolve_imports()?;
        self.validate_declarations()?;
        self.validate_pipe_matches()?;
        self.find_query_operations()?;
        self.validate_semantic_wrappers()?;
        let mut entries = vec![];
        for (module, ast) in &self.modules {
            for d in &ast.declarations {
                if let DeclKind::Entry { node, trivia } = &d.kind {
                    let ExprKind::Ref(name) = &node.kind else {
                        return Err(Diagnostic::error(
                            "yl.entrypoint",
                            "Expected entry node",
                            node.span.clone(),
                        ));
                    };
                    let entry = self.resolve(module, "", name, &node.span)?;
                    let mut selected = vec![];
                    for matcher in trivia {
                        let ExprKind::Ref(name) = &matcher.kind else {
                            return Err(Diagnostic::error(
                                "yl.entry_trivia",
                                "Expected trivia reference",
                                matcher.span.clone(),
                            ));
                        };
                        let id = self.resolve(module, "", name, &matcher.span)?;
                        if !self.symbols.get(&id).is_some_and(|s| {
                            matches!(s.declaration.kind, DeclKind::Node { trivia: true, .. })
                        }) {
                            return Err(Diagnostic::error(
                                "yl.entry_trivia",
                                "Entry trivia selection must name a trivia declaration",
                                matcher.span.clone(),
                            ));
                        }
                        if !selected.contains(&id) {
                            selected.push(id);
                        }
                    }
                    entries.push((entry, selected));
                }
            }
        }
        if entries.len() != 1 {
            return Err(Diagnostic::error(
                "yl.entrypoint",
                format!("Expected exactly one entrypoint, found {}", entries.len()),
                self.modules
                    .values()
                    .next()
                    .map(|m| m.span.clone())
                    .unwrap_or_default(),
            ));
        }
        let (entry, trivia) = entries.remove(0);
        let mut language = CompiledLanguage {
            version: 1,
            backend: crate::backend::BackendImage::default(),
            entry,
            rules: BTreeMap::new(),
            trivia,
            meanings: BTreeMap::new(),
            query_meanings: BTreeMap::new(),
            query_functions: BTreeMap::new(),
            highlights: BTreeMap::new(),
            examples: BTreeMap::new(),
            diagnostics: vec![],
            image: None,
        };
        self.lower_backend(&mut language)?;
        self.lower_semantic_definitions(&mut language)?;
        self.lower_rules(&mut language)?;
        for (module, extension) in self.extensions.clone() {
            if let DeclKind::Extend { name, constraints } = extension.kind {
                let id = self.resolve(&module, "", &name, &extension.span)?;
                let checks = self.lower_checks(&module, "", &constraints, &BTreeMap::new(), 0)?;
                let rule = language.rules.get_mut(&id).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.extension_conflict",
                        "Extension target is not a node",
                        extension.span.clone(),
                    )
                })?;
                rule.constraints.extend(checks);
            }
        }
        if !language.rules.contains_key(&language.entry) {
            return Err(Diagnostic::error(
                "yl.entrypoint",
                "Entrypoint must be a node",
                Span::default(),
            ));
        }
        self.lower_tooling(&mut language)?;
        crate::examples::validate_explicit(&language)?;
        language.diagnostics = std::mem::take(&mut self.diagnostics);
        for (id, rule) in &language.rules {
            if let Some(meanings) = language.meanings.get(id) {
                let captures = rule_capture_sets(rule, &language, &mut BTreeSet::new()).0;
                validate_meaning_captures(meanings, &captures).map_err(|mut error| {
                    error.code = "yl.unknown_capture".into();
                    error
                })?;
            }
        }
        language.version = if !language.query_meanings.is_empty() {
            5
        } else if language.meanings.is_empty() {
            1
        } else {
            2
        };
        language.query_functions = self
            .query_functions
            .borrow()
            .iter()
            .filter_map(|(name, value)| value.clone().map(|value| (name.clone(), value)))
            .collect();
        validate_language(&language)?;
        language.image = Some(Box::new(crate::image::LanguageImage::compile(&language)?));
        Ok(language)
    }
}
