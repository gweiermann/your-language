//! Compile syntax to stack instructions, then assemble an executable Wasm module.
use std::collections::BTreeMap;
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
    Module, TypeSection, ValType,
};
use your_language::{
    backend::{Backend, BackendContext, Capture, HookBinding, HookContract},
    diagnostic::Diagnostic,
};

use super::common::{backend_error, operation, signatures};

type Instructions = Vec<Instruction<'static>>;

#[derive(Default)]
pub struct Wasm {
    scopes: Vec<BTreeMap<String, u32>>,
    local_count: u32,
}

impl Backend for Wasm {
    type Value = Instructions;

    fn bindings(&self) -> Vec<HookBinding> {
        signatures()
    }

    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        arguments: &[Capture],
    ) -> Result<Instructions, Diagnostic> {
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

impl Wasm {
    /// Export one `run() -> i32` function containing the compiled program.
    pub fn finish(&self, instructions: &[Instruction<'static>]) -> Vec<u8> {
        let mut module = Module::new();

        let mut types = TypeSection::new();
        types.ty().function([], [ValType::I32]);
        module.section(&types);

        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);

        let mut exports = ExportSection::new();
        exports.export("run", ExportKind::Func, 0);
        module.section(&exports);

        // Lexical scopes share one Wasm function, so allocate all locals here.
        // Shadowed names have distinct indices even after their scope ends.
        let mut function = Function::new([(self.local_count, ValType::I32)]);
        for instruction in instructions {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);

        let mut code = CodeSection::new();
        code.function(&function);
        module.section(&code);
        module.finish()
    }

    fn local(&self, name: &str) -> Result<u32, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .copied()
            .ok_or_else(|| backend_error(format!("Unknown variable {name}")))
    }

    fn number(
        &self,
        context: &BackendContext<'_, '_, Self>,
        value: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        let number = context
            .program()
            .text(value.syntax()?)?
            .parse::<i32>()
            .map_err(|_| backend_error("Integer literal is outside signed i32 range"))?;
        Ok(vec![Instruction::I32Const(number)])
    }

    fn reference(
        &self,
        context: &BackendContext<'_, '_, Self>,
        name: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        let local = self.local(context.program().text(name.syntax()?)?)?;
        Ok(vec![Instruction::LocalGet(local)])
    }

    fn binary(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        operator: &str,
        left: &Capture,
        right: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        let mut code = context.dispatch(self, left.syntax()?)?;
        code.extend(context.dispatch(self, right.syntax()?)?);
        // Both operands are now on the Wasm stack. Integer arithmetic wraps
        // naturally, and signed comparison produces i32 0 or 1.
        code.push(match operator {
            "add" => Instruction::I32Add,
            "multiply" => Instruction::I32Mul,
            "less" => Instruction::I32LtS,
            _ => return Err(backend_error("Unsupported binary operator")),
        });
        Ok(code)
    }

    fn declare(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        initializer: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        // Compile before registering the declaration, so a shadowing initializer
        // can still refer to the enclosing binding.
        let mut code = context.dispatch(self, initializer.syntax()?)?;
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| backend_error("No lexical scope"))?;
        if scope.contains_key(&name) {
            return Err(backend_error(format!("Duplicate variable {name}")));
        }
        let local = self.local_count;
        self.local_count += 1;
        scope.insert(name, local);
        code.push(Instruction::LocalSet(local));
        Ok(code)
    }

    fn assign(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        name: &Capture,
        expression: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        let name = context.program().text(name.syntax()?)?.to_owned();
        let mut code = context.dispatch(self, expression.syntax()?)?;
        code.push(Instruction::LocalSet(self.local(&name)?));
        Ok(code)
    }

    fn scoped_statements(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        statements: &Capture,
        result_expression: Option<&Capture>,
    ) -> Result<Instructions, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let result = (|| {
            let mut code = Vec::new();
            for statement in statements.list()? {
                code.extend(context.dispatch(self, statement.syntax()?)?);
            }
            if let Some(expression) = result_expression {
                code.extend(context.dispatch(self, expression.syntax()?)?);
            }
            Ok(code)
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
    ) -> Result<Instructions, Diagnostic> {
        let mut code = context.dispatch(self, condition.syntax()?)?;
        code.push(Instruction::If(BlockType::Empty));
        code.extend(context.dispatch(self, yes.syntax()?)?);
        code.push(Instruction::Else);
        code.extend(context.dispatch(self, no.syntax()?)?);
        code.push(Instruction::End);
        Ok(code)
    }

    fn repeat(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        condition: &Capture,
        body: &Capture,
    ) -> Result<Instructions, Diagnostic> {
        // Branch depths are relative to the innermost active label:
        // br_if 1 exits the outer block; br 0 repeats the inner loop.
        let mut code = vec![
            Instruction::Block(BlockType::Empty),
            Instruction::Loop(BlockType::Empty),
        ];
        code.extend(context.dispatch(self, condition.syntax()?)?);
        code.push(Instruction::I32Eqz);
        code.push(Instruction::BrIf(1));
        code.extend(context.dispatch(self, body.syntax()?)?);
        code.push(Instruction::Br(0));
        code.push(Instruction::End);
        code.push(Instruction::End);
        Ok(code)
    }
}
