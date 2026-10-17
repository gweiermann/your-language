//! Transpile syntax to JavaScript while preserving this language's i32 semantics.
use std::collections::BTreeMap;
use your_language::{
    backend::{Backend, BackendContext, Capture, HookBinding, HookContract},
    diagnostic::Diagnostic,
};

use super::common::{backend_error, operation, signatures};

#[derive(Default)]
pub struct JavaScript {
    scopes: Vec<BTreeMap<String, String>>,
    next_binding: usize,
}

impl Backend for JavaScript {
    type Value = String;

    fn bindings(&self) -> Vec<HookBinding> {
        signatures()
    }

    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        arguments: &[Capture],
    ) -> Result<String, Diagnostic> {
        match operation(hook) {
            "number" => self.number(context, &arguments[0]),
            "reference" => self.reference(context, &arguments[0]),
            "add" | "multiply" | "less" => {
                self.binary(context, operation(hook), &arguments[0], &arguments[1])
            }
            "declare" => self.declare(context, &arguments[0], &arguments[1]),
            "assign" => self.assign(context, &arguments[0], &arguments[1]),
            "block" => self.scoped_statements(context, &arguments[0], None),
            "program" => self.scoped_statements(context, &arguments[0], Some(&arguments[1])),
            "conditional" => self.conditional(context, &arguments[0], &arguments[1], &arguments[2]),
            "repeat" => self.repeat(context, &arguments[0], &arguments[1]),
            _ => Err(backend_error("Unsupported hook")),
        }
    }
}

impl JavaScript {
    fn binding(&self, name: &str) -> Result<String, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .ok_or_else(|| backend_error(format!("Unknown variable {name}")))
    }

    fn number(
        &self,
        context: &BackendContext<'_, '_, Self>,
        value: &Capture,
    ) -> Result<String, Diagnostic> {
        let number = context
            .program()
            .text(value.syntax()?)?
            .parse::<i32>()
            .map_err(|_| backend_error("Integer literal is outside signed i32 range"))?;
        Ok(number.to_string())
    }

    fn reference(
        &self,
        context: &BackendContext<'_, '_, Self>,
        name: &Capture,
    ) -> Result<String, Diagnostic> {
        self.binding(context.program().text(name.syntax()?)?)
    }

    fn binary(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        operator: &str,
        left: &Capture,
        right: &Capture,
    ) -> Result<String, Diagnostic> {
        let left = context.dispatch(self, left.syntax()?)?;
        let right = context.dispatch(self, right.syntax()?)?;
        // JavaScript uses floating-point numbers. These forms preserve i32 results
        // and represent comparisons as 0 or 1, matching the other two hosts.
        match operator {
            "add" => Ok(format!("(({left} + {right}) | 0)")),
            "multiply" => Ok(format!("Math.imul({left}, {right})")),
            "less" => Ok(format!("(({left} < {right}) | 0)")),
            _ => Err(backend_error("Unsupported binary operator")),
        }
    }

    fn declare(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        initializer: &Capture,
    ) -> Result<String, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        let value = context.dispatch(self, initializer.syntax()?)?;
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| backend_error("No lexical scope"))?;
        if scope.contains_key(&name) {
            return Err(backend_error(format!("Duplicate variable {name}")));
        }

        // Give every declaration a distinct JavaScript name. In this language,
        // `let x = x + 1` may read an outer x. Emitting the same name would hit
        // JavaScript's temporal dead zone instead, even inside a nested block.
        let binding = format!("v_{name}_{}", self.next_binding);
        self.next_binding += 1;
        scope.insert(name, binding.clone());
        Ok(format!("let {binding} = {value};"))
    }

    fn assign(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        expression: &Capture,
    ) -> Result<String, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        let value = context.dispatch(self, expression.syntax()?)?;
        let binding = self.binding(&name)?;
        Ok(format!("{binding} = {value};"))
    }

    fn scoped_statements(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        statements: &Capture,
        result_expression: Option<&Capture>,
    ) -> Result<String, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let result = (|| {
            let mut generated_statements = Vec::new();
            for statement in statements.list()? {
                generated_statements.push(context.dispatch(self, statement.syntax()?)?);
            }
            let statements = indent(&generated_statements.join("\n"));
            match result_expression {
                Some(expression) => {
                    let value = context.dispatch(self, expression.syntax()?)?;
                    Ok(format!(
                        "function run() {{\n{statements}\n    return {value};\n}}\nconsole.log(run());\n"
                    ))
                }
                None => Ok(format!("{{\n{statements}\n}}")),
            }
        })();
        self.scopes.pop();
        result
    }

    fn conditional(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        condition: &Capture,
        yes: &Capture,
        no: &Capture,
    ) -> Result<String, Diagnostic> {
        let condition = context.dispatch(self, condition.syntax()?)?;
        // Emit both branches: JavaScript will choose the branch at runtime.
        let yes = context.dispatch(self, yes.syntax()?)?;
        let no = context.dispatch(self, no.syntax()?)?;
        Ok(format!("if ({condition}) {yes} else {no}"))
    }

    fn repeat(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        condition: &Capture,
        body: &Capture,
    ) -> Result<String, Diagnostic> {
        let condition = context.dispatch(self, condition.syntax()?)?;
        let body = context.dispatch(self, body.syntax()?)?;
        Ok(format!("while ({condition}) {body}"))
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
