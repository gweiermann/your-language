//! Lower concrete and abstract rules, including recursive operator precedence.

use super::*;

impl Compiler {
    /// Lower concrete and abstract rules, including recursive operator precedence.
    pub(super) fn lower_rules(&mut self, language: &mut CompiledLanguage) -> Result<()> {
        for (id, symbol) in self.symbols.clone() {
            if let DeclKind::Node {
                trivia,
                grammar,
                members,
                precedence,
                constraints,
                ..
            } = &symbol.declaration.kind
            {
                let body = if let Some(grammar) = grammar {
                    RuleBody::Concrete(self.lower(
                        &symbol.module,
                        &symbol.name,
                        grammar,
                        &GrammarBindings::new(),
                        0,
                    )?)
                } else {
                    if members.is_empty() {
                        return Err(Diagnostic::error(
                            "yl.incomplete_declaration",
                            "Top-level node needs a grammar or members",
                            symbol.declaration.span.clone(),
                        ));
                    }
                    let mut bases = vec![];
                    let mut visiting = BTreeSet::new();
                    self.leaves(&id, &mut visiting, &mut bases)?;
                    let all_leaves = bases;
                    let mut bases = members
                        .iter()
                        .filter_map(|m| {
                            if let DeclKind::Node { name, .. } = &m.kind {
                                Some(self.member_path(&id, name, &m.span, 0))
                            } else {
                                None
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let mut operators = vec![];
                    let mut listed = BTreeSet::new();
                    if precedence.is_empty() {
                        for member in &all_leaves {
                            let member_symbol = self.symbols[member].clone();
                            let DeclKind::Node {
                                grammar: Some(grammar),
                                ..
                            } = &member_symbol.declaration.kind
                            else {
                                continue;
                            };
                            // Candidate discovery does not emit a runtime term. Keep
                            // projection IDs and warnings owned by actual lowering.
                            let next_marker = self.next_marker;
                            let diagnostic_count = self.diagnostics.len();
                            let term = self.lower(
                                &member_symbol.module,
                                &member_symbol.name,
                                grammar,
                                &GrammarBindings::new(),
                                0,
                            );
                            self.next_marker = next_marker;
                            self.diagnostics.truncate(diagnostic_count);
                            let term = term?;
                            let left = edge_ref(&term, false) == Some(id.as_str());
                            let right = edge_ref(&term, true) == Some(id.as_str());
                            if left || right {
                                listed.insert(member.clone());
                                operators.push(Operator {
                                    rule: member.clone(),
                                    binding_power: 1,
                                    associativity: Associativity::Left,
                                    left,
                                    right,
                                });
                            }
                        }
                        if operators.len() > 1 {
                            let mut diagnostic = Diagnostic::error(
                                "yl.missing_precedence",
                                "Multiple recursive operators require explicit precedence",
                                symbol.declaration.span.clone(),
                            );
                            diagnostic.secondary = operators
                                .iter()
                                .map(|operator| {
                                    self.symbols[&operator.rule].declaration.span.clone()
                                })
                                .collect();
                            diagnostic.help = Some(
                                "Add a precedence block listing the recursive operators in binding order"
                                    .into(),
                            );
                            return Err(diagnostic);
                        }
                    }
                    for (level_index, level) in precedence.iter().enumerate() {
                        for name in &level.members {
                            let member = self.member_path(&id, name, &level.span, 0).map_err(
                                |mut error| {
                                    error.code = "yl.invalid_precedence_member".into();
                                    error
                                },
                            )?;
                            if !all_leaves.contains(&member) || !listed.insert(member.clone()) {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    format!("Invalid/duplicate concrete precedence member {name}"),
                                    level.span.clone(),
                                ));
                            }
                            let member_symbol =
                                self.symbols.get(&member).cloned().ok_or_else(|| {
                                    Diagnostic::error(
                                        "yl.invalid_precedence_member",
                                        "Unknown member",
                                        level.span.clone(),
                                    )
                                })?;
                            let DeclKind::Node {
                                grammar: Some(grammar),
                                ..
                            } = &member_symbol.declaration.kind
                            else {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    "Precedence member needs concrete grammar",
                                    level.span.clone(),
                                ));
                            };
                            let term = self.lower(
                                &member_symbol.module,
                                &member_symbol.name,
                                grammar,
                                &GrammarBindings::new(),
                                0,
                            )?;
                            let left = edge_ref(&term, false) == Some(id.as_str());
                            let right = edge_ref(&term, true) == Some(id.as_str());
                            if !left && !right {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    "Precedence member must recurse at an edge",
                                    level.span.clone(),
                                ));
                            }
                            operators.push(Operator {
                                rule: member,
                                binding_power: precedence.len() - level_index,
                                associativity: level.associativity,
                                left,
                                right,
                            });
                        }
                    }
                    bases.retain(|b| !listed.contains(b));
                    RuleBody::Abstract { bases, operators }
                };
                let checks = self.lower_checks(
                    &symbol.module,
                    &symbol.name,
                    constraints,
                    &BTreeMap::new(),
                    0,
                )?;
                language.rules.insert(
                    id.clone(),
                    Rule {
                        name: symbol.name.clone(),
                        trivia: *trivia,
                        body,
                        constraints: checks,
                        span: symbol.declaration.span.clone(),
                    },
                );
            }
        }
        Ok(())
    }
}
