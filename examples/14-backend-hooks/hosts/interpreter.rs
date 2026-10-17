//! Execute syntax immediately using signed 32-bit integers and lexical scopes.
use std::collections::BTreeMap;
use your_language::{
    backend::{Backend, BackendContext, Capture, HookBinding, HookContract},
    diagnostic::Diagnostic,
};

use super::common::{backend_error, operation, signatures};

#[derive(Default)]
pub struct Interpreter {
    scopes: Vec<BTreeMap<String, i32>>,
    pub loop_iterations: usize,
}

impl Backend for Interpreter {
    type Value = i32;

    fn bindings(&self) -> Vec<HookBinding> {
        signatures()
    }

    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        arguments: &[Capture],
    ) -> Result<i32, Diagnostic> {
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

impl Interpreter {
    fn number(
        &self,
        context: &BackendContext<'_, '_, Self>,
        value: &Capture,
    ) -> Result<i32, Diagnostic> {
        context
            .program()
            .text(value.syntax()?)?
            .parse()
            .map_err(|_| backend_error("Integer literal is outside signed i32 range"))
    }

    fn reference(
        &self,
        context: &BackendContext<'_, '_, Self>,
        name: &Capture,
    ) -> Result<i32, Diagnostic> {
        let syntax = name.syntax()?;
        let name = context.program().text(syntax)?;
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .copied()
            .ok_or_else(|| {
                context.program().error(
                    "example.undefined",
                    format!("Unknown variable {name}"),
                    syntax,
                )
            })
    }

    fn binary(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        operator: &str,
        left: &Capture,
        right: &Capture,
    ) -> Result<i32, Diagnostic> {
        let left = context.dispatch(self, left.syntax()?)?;
        let right = context.dispatch(self, right.syntax()?)?;

        // Explicit wrapping keeps debug builds, JavaScript, and Wasm consistent.
        match operator {
            "add" => Ok(left.wrapping_add(right)),
            "multiply" => Ok(left.wrapping_mul(right)),
            "less" => Ok(i32::from(left < right)),
            _ => Err(backend_error("Unsupported binary operator")),
        }
    }

    fn declare(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        initializer: &Capture,
    ) -> Result<i32, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        // The initializer sees the enclosing binding when a declaration shadows it.
        let value = context.dispatch(self, initializer.syntax()?)?;
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| backend_error("No lexical scope"))?;
        if scope.contains_key(&name) {
            return Err(backend_error(format!("Duplicate variable {name}")));
        }
        scope.insert(name, value);
        Ok(0)
    }

    fn assign(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        expression: &Capture,
    ) -> Result<i32, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        let value = context.dispatch(self, expression.syntax()?)?;
        let slot = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(&name))
            .ok_or_else(|| backend_error(format!("Unknown variable {name}")))?;
        *slot = value;
        Ok(0)
    }

    fn scoped_statements(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        statements: &Capture,
        result_expression: Option<&Capture>,
    ) -> Result<i32, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let result = (|| {
            for statement in statements.list()? {
                context.dispatch(self, statement.syntax()?)?;
            }
            match result_expression {
                Some(expression) => context.dispatch(self, expression.syntax()?),
                None => Ok(0),
            }
        })();
        // Restore the scope even when a nested hook reports an error.
        self.scopes.pop();
        result
    }

    fn conditional(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        condition: &Capture,
        yes: &Capture,
        no: &Capture,
    ) -> Result<i32, Diagnostic> {
        let condition = context.dispatch(self, condition.syntax()?)?;
        let branch = if condition != 0 { yes } else { no };
        // Unlike a compiler, the interpreter never visits the unchosen branch.
        context.dispatch(self, branch.syntax()?)
    }

    fn repeat(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        condition: &Capture,
        body: &Capture,
    ) -> Result<i32, Diagnostic> {
        while context.dispatch(self, condition.syntax()?)? != 0 {
            self.loop_iterations += 1;
            if self.loop_iterations > 10_000 {
                return Err(backend_error("Example interpreter loop limit exceeded"));
            }
            context.dispatch(self, body.syntax()?)?;
        }
        Ok(0)
    }
}
