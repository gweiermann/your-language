//! Lower hook contracts and unevaluated argument plans separately from retryable queries.

use super::*;

impl Compiler {
    /// Lower hook contracts and unevaluated argument plans separately from retryable queries.
    pub(super) fn lower_backend(&self, language: &mut CompiledLanguage) -> Result<()> {
        // Hook contracts and normalized calls are distinct from retryable semantic queries.
        let mut hook_ids = BTreeMap::new();
        for (id, symbol) in &self.symbols {
            if let DeclKind::Hook { parameters, .. } = &symbol.declaration.kind {
                if !symbol.scope.is_empty() {
                    return Err(Diagnostic::error(
                        "yl.hook_scope",
                        "Hooks must be declared at module level",
                        symbol.declaration.span.clone(),
                    ));
                }
                let names: Vec<_> = parameters.iter().map(|p| p.name.clone()).collect();
                if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
                    return Err(Diagnostic::error(
                        "yl.duplicate_parameter",
                        "Duplicate hook parameter",
                        symbol.declaration.span.clone(),
                    ));
                }
                hook_ids.insert(id.clone(), language.backend.hooks.len() as u32);
                language.backend.hooks.push(crate::backend::HookContract {
                    id: id.clone(),
                    parameters: names,
                    span: symbol.declaration.span.clone(),
                });
            }
        }
        for (id, symbol) in &self.symbols {
            let backend = match &symbol.declaration.kind {
                DeclKind::Node {
                    backend, trivia, ..
                } => {
                    if *trivia && backend.is_some() {
                        return Err(Diagnostic::error(
                            "yl.backend_scope",
                            "Trivia cannot dispatch backend hooks",
                            symbol.declaration.span.clone(),
                        ));
                    }
                    backend
                }
                DeclKind::Pattern { backend, .. } => backend,
                _ => continue,
            };
            if let Some(backend) = backend {
                let hook =
                    self.resolve(&symbol.module, &symbol.name, &backend.hook, &backend.span)?;
                let index = *hook_ids.get(&hook).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.backend_hook",
                        "Backend call must name a declared hook",
                        backend.span.clone(),
                    )
                })?;
                let contract = &language.backend.hooks[index as usize];
                if contract.parameters.len() != backend.arguments.len() {
                    return Err(Diagnostic::error(
                        "yl.backend_arity",
                        format!(
                            "Hook {} expects {} arguments, found {}",
                            backend.hook,
                            contract.parameters.len(),
                            backend.arguments.len()
                        ),
                        backend.span.clone(),
                    ));
                }
                fn argument(expr: &Expr) -> Result<crate::backend::ArgumentPlan> {
                    use crate::backend::ArgumentPlan;
                    Ok(match &expr.kind {
                        ExprKind::Ref(name) if name=="self" => ArgumentPlan::SelfSyntax,
                        ExprKind::Ref(name) => ArgumentPlan::Capture(name.clone()),
                        ExprKind::Group(inner) => return argument(inner),
                        ExprKind::Tuple(items) => ArgumentPlan::Tuple(items.iter().map(argument).collect::<Result<_>>()?),
                        ExprKind::List(items) => ArgumentPlan::List(items.iter().map(argument).collect::<Result<_>>()?),
                        _ => return Err(Diagnostic::error("yl.backend_argument", "Backend arguments must be unevaluated captures, self, or tuple/list groups", expr.span.clone())),
                    })
                }
                language.backend.calls.insert(
                    id.clone(),
                    crate::backend::CallPlan {
                        hook: index,
                        arguments: backend
                            .arguments
                            .iter()
                            .map(argument)
                            .collect::<Result<_>>()?,
                        span: backend.span.clone(),
                    },
                );
            }
        }
        if !language.backend.calls.is_empty() {
            // Trace nodes and named patterns so syntax handles survive lowering and pipes.
            for (id, symbol) in &self.symbols {
                if matches!(
                    symbol.declaration.kind,
                    DeclKind::Node { trivia: false, .. } | DeclKind::Pattern { .. }
                ) {
                    language.meanings.entry(id.clone()).or_default();
                }
            }
        }
        Ok(())
    }
}
