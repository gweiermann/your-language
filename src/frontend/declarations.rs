//! Node, pattern, module, enum, operation, and hook declarations.

use super::*;

impl Parser {
    pub(super) fn declaration(&mut self) -> Result<Declaration> {
        let start = self.token().span.clone();
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "YL nesting exceeds implementation resource limit",
            ));
        }
        let export = self.take("export");
        let kind = if self.take("import") {
            let namespace = if self.take("*") {
                self.expect("as")?;
                Some(self.word()?)
            } else {
                None
            };
            let mut names = vec![];
            if namespace.is_none() {
                self.expect("{")?;
                while !self.is("}") {
                    let name = self.path()?;
                    let alias = if self.take("as") {
                        self.word()?
                    } else {
                        name.rsplit("::").next().unwrap_or(&name).into()
                    };
                    names.push((name, alias));
                    if !self.take(",") {
                        break;
                    }
                }
                self.expect("}")?;
            }
            self.expect("from")?;
            let t = self.bump();
            let TokenKind::String(path) = t.kind else {
                return Err(Diagnostic::error(
                    "yl.unexpected_token",
                    "Expected import path string",
                    t.span,
                ));
            };
            if let Some(name) = namespace {
                DeclKind::NamespaceImport { name, path }
            } else {
                DeclKind::Import { names, path }
            }
        } else if self.take("hook") {
            let name = self.word()?;
            self.expect("(")?;
            let mut parameters = vec![];
            let mut names = BTreeSet::new();
            while !self.is(")") {
                let span = self.token().span.clone();
                let name = self.word()?;
                if !names.insert(name.clone()) {
                    return Err(self.error("yl.hook_parameter", "Duplicate hook parameter"));
                }
                parameters.push(Parameter {
                    name,
                    ty: None,
                    default: None,
                    span: self.span_from(&span),
                });
                if self.is(":") || self.is("=") {
                    return Err(self.error(
                        "yl.hook_parameter",
                        "Hook parameters are names only; types and defaults are not supported",
                    ));
                }
                if !self.take(",") {
                    break;
                }
            }
            self.expect(")")?;
            DeclKind::Hook { name, parameters }
        } else if self.take("operation") {
            let name = self.word()?;
            let parameters = self.semantic_parameters()?;
            let body = if self.take("=") {
                OperationBody::Query(self.semantic_value()?)
            } else {
                self.expect("{")?;
                let actions = self.meaning_block()?;
                if actions
                    .iter()
                    .all(|action| matches!(action, MeaningSyntaxAction::Call(_)))
                {
                    OperationBody::Actions(
                        actions
                            .into_iter()
                            .map(|action| match action {
                                MeaningSyntaxAction::Call(call) => call,
                                _ => unreachable!(),
                            })
                            .collect(),
                    )
                } else {
                    OperationBody::Conditional(actions)
                }
            };
            DeclKind::Operation {
                name,
                parameters,
                body,
            }
        } else if self.take("symbol") {
            DeclKind::Symbol { name: self.word()? }
        } else if self.take("entry") {
            let node = self.selector()?;
            let mut trivia = vec![];
            if self.take("{") {
                if !self.is("}") {
                    self.expect("trivia")?;
                    trivia.push(self.selector()?);
                    while self.take(",") {
                        trivia.push(self.selector()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Entry { node, trivia }
        } else if self.take("extend") {
            self.expect("node")?;
            let name = self.path()?;
            self.expect("{")?;
            let mut constraints = vec![];
            while !self.is("}") {
                if !self.take("checks") {
                    return Err(self.error("yl.extension_conflict","An extension may only add checks in syntax-v0; metadata is deferred beyond syntax-v0"));
                }
                constraints.extend(self.constraint_block()?);
            }
            self.expect("}")?;
            DeclKind::Extend { name, constraints }
        } else if self.is("node") || self.is("trivia") {
            let trivia = self.take("trivia");
            if !trivia {
                self.expect("node")?;
            }
            let name = self.path()?;
            let mut grammar = None;
            let mut members = vec![];
            let mut precedence = vec![];
            let mut constraints = vec![];
            let mut meanings = None;
            let mut example = None;
            let mut ide = None;
            let mut backend = None;
            if self.take("=") {
                grammar = Some(self.expression()?);
            }
            if self.take("{") {
                while !self.is("}") {
                    if self.is("backend") {
                        if backend.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate backend section")
                            );
                        }
                        backend = Some(self.backend()?);
                    } else if self.take("ide") {
                        if ide.is_some() {
                            return Err(self.error("yl.duplicate_member", "Duplicate ide section"));
                        }
                        let (metadata, explicit) = self.ide()?;
                        ide = Some(metadata);
                        example = explicit;
                    } else if self.is("example") {
                        return Err(self.error(
                            "yl.ide_migration",
                            "Move example metadata into ide { example \"...\" }",
                        ));
                    } else if self.take("semantics") {
                        if meanings.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate semantics section")
                            );
                        }
                        meanings = Some(self.meanings()?);
                    } else if self.take("checks") {
                        constraints.extend(self.constraint_block()?);
                    } else if self.take("precedence") {
                        if !precedence.is_empty() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate precedence section")
                            );
                        }
                        self.expect("{")?;
                        loop {
                            let span = self.token().span.clone();
                            let associativity = if self.take("right") {
                                Associativity::Right
                            } else if self.take("nonassoc") {
                                Associativity::Nonassoc
                            } else {
                                Associativity::Left
                            };
                            let mut level = vec![self.path()?];
                            while self.take(",") {
                                level.push(self.path()?);
                            }
                            precedence.push(Level {
                                members: level,
                                associativity,
                                span: self.span_from(&span),
                            });
                            if !self.take(">") {
                                break;
                            }
                        }
                        self.expect("}")?;
                    } else {
                        members.push(self.declaration()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Node {
                name,
                trivia,
                grammar,
                members,
                precedence,
                constraints,
                meanings,
                example,
                ide,
                backend,
            }
        } else if self.take("pattern") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            self.expect("=")?;
            let grammar = self.expression()?;
            let mut meanings = None;
            let mut example = None;
            let mut ide = None;
            let mut backend = None;
            if self.take("{") {
                while !self.is("}") {
                    if self.is("backend") {
                        if backend.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate backend section")
                            );
                        }
                        backend = Some(self.backend()?);
                    } else if self.take("ide") {
                        if ide.is_some() {
                            return Err(self.error("yl.duplicate_member", "Duplicate ide section"));
                        }
                        let (metadata, explicit) = self.ide()?;
                        ide = Some(metadata);
                        example = explicit;
                    } else if self.is("example") {
                        return Err(self.error(
                            "yl.ide_migration",
                            "Move example metadata into ide { example \"...\" }",
                        ));
                    } else {
                        self.expect("semantics")?;
                        if meanings.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate semantics section")
                            );
                        }
                        meanings = Some(self.meanings()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Pattern {
                name,
                parameters,
                grammar,
                meanings,
                example,
                ide,
                backend,
            }
        } else if self.take("pipe") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            self.expect("{")?;
            let mut arms = vec![];
            while !self.is("}") {
                let span = self.token().span.clone();
                self.expect("rewrite")?;
                let binding = self.word()?;
                let quantifier = if self.take("*") {
                    Some(Quantifier::Star)
                } else if self.take("?") {
                    Some(Quantifier::Optional)
                } else if self.take("+") {
                    Some(Quantifier::Plus)
                } else {
                    None
                };
                let ty = if self.take(":") {
                    Some(self.path()?)
                } else {
                    None
                };
                let selectors = if self.take("match") {
                    let tuple = self.take("(");
                    let mut selectors = vec![self.selector()?];
                    if tuple {
                        while self.take(",") {
                            selectors.push(self.selector()?);
                        }
                        self.expect(")")?;
                    }
                    Some(selectors)
                } else {
                    None
                };
                let body = if let Some(selectors) = selectors {
                    RewriteBody::Match {
                        selectors,
                        cases: self.rewrite_cases()?,
                    }
                } else if self.take("=>") {
                    RewriteBody::Direct(self.expression()?)
                } else {
                    let cases = self.rewrite_cases()?;
                    if cases.iter().all(|c| {
                        c.patterns.len() == 1
                            && c.patterns[0].is_some()
                            && c.replacement.is_some()
                            && c.diagnostics.is_empty()
                    }) {
                        RewriteBody::Cases(
                            cases
                                .into_iter()
                                .filter_map(|c| {
                                    c.patterns.into_iter().next().flatten().zip(c.replacement)
                                })
                                .collect(),
                        )
                    } else {
                        RewriteBody::Match {
                            selectors: vec![],
                            cases,
                        }
                    }
                };
                arms.push(Rewrite {
                    binding,
                    quantifier,
                    ty,
                    body,
                    span: self.span_from(&span),
                });
            }
            self.expect("}")?;
            DeclKind::Pipe {
                name,
                parameters,
                arms,
            }
        } else if self.take("enum") {
            let name = self.word()?;
            self.expect("{")?;
            let mut variants = vec![];
            while !self.is("}") {
                variants.push(self.word()?);
                self.take(",");
            }
            self.expect("}")?;
            DeclKind::Enum { name, variants }
        } else if self.take("constraint") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            let body = self.constraint_block()?;
            DeclKind::Constraint {
                name,
                parameters,
                body,
            }
        } else {
            return Err(self.error(
                "yl.incomplete_declaration",
                "Expected declaration (when belongs inside checks, semantics, or an operation)",
            ));
        };
        self.depth -= 1;
        Ok(Declaration {
            documentation: None,
            export,
            kind,
            span: self.span_from(&start),
        })
    }
}
