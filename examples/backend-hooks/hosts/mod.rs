#![allow(dead_code)]
use std::{collections::BTreeMap, path::Path};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
    Module, TypeSection, ValType,
};
use your_language::{
    backend::{Backend, BackendContext, Capture, HookBinding, HookContract},
    diagnostic::{Diagnostic, Span},
    CompiledLanguage,
};

pub fn load() -> Result<CompiledLanguage, String> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/backend-hooks/definition/language.yl");
    let language = your_language::compile_language(path).map_err(|e| format!("{e:?}"))?;
    your_language::load_compiled_language(&language.to_bytes().map_err(|e| format!("{e:?}"))?)
        .map_err(|e| format!("{e:?}"))
}
fn signatures() -> Vec<HookBinding> {
    [
        ("number", &["value"][..]),
        ("reference", &["name"][..]),
        ("add", &["left", "right"][..]),
        ("multiply", &["left", "right"][..]),
        ("less", &["left", "right"][..]),
        ("declare", &["name", "initializer"][..]),
        ("assign", &["name", "value"][..]),
        ("block", &["statements"][..]),
        ("conditional", &["condition", "yes", "no"][..]),
        ("repeat", &["condition", "body"][..]),
        ("program", &["statements", "result"][..]),
    ]
    .into_iter()
    .map(|(name, args)| HookBinding::new(format!("hooks.yl#{name}"), args))
    .collect()
}
fn unknown(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("example.backend", message, Span::default())
}
fn operation(hook: &HookContract) -> &str {
    hook.id.rsplit('#').next().unwrap_or("")
}

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
        cx: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        args: &[Capture],
    ) -> Result<i32, Diagnostic> {
        match operation(hook) {
            "number" => cx
                .program()
                .text(args[0].syntax()?)?
                .parse::<i32>()
                .map_err(|_| unknown("Integer literal is outside signed i32 range")),
            "reference" => {
                let syntax = args[0].syntax()?;
                let name = cx.program().text(syntax)?;
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|s| s.get(name))
                    .copied()
                    .ok_or_else(|| {
                        cx.program().error(
                            "example.undefined",
                            format!("Unknown variable {name}"),
                            syntax,
                        )
                    })
            }
            "add" | "multiply" | "less" => {
                let left = cx.dispatch(self, args[0].syntax()?)?;
                let right = cx.dispatch(self, args[1].syntax()?)?;
                Ok(match operation(hook) {
                    "add" => left.wrapping_add(right),
                    "multiply" => left.wrapping_mul(right),
                    _ => i32::from(left < right),
                })
            }
            "declare" | "assign" => {
                let name = cx.program().text(args[0].syntax()?)?.to_owned();
                let value = cx.dispatch(self, args[1].syntax()?)?;
                if operation(hook) == "declare" {
                    let scope = self
                        .scopes
                        .last_mut()
                        .ok_or_else(|| unknown("No lexical scope"))?;
                    if scope.contains_key(&name) {
                        return Err(unknown(format!("Duplicate variable {name}")));
                    }
                    scope.insert(name, value);
                } else {
                    let slot = self
                        .scopes
                        .iter_mut()
                        .rev()
                        .find_map(|s| s.get_mut(&name))
                        .ok_or_else(|| unknown(format!("Unknown variable {name}")))?;
                    *slot = value;
                }
                Ok(0)
            }
            "program" | "block" => {
                self.scopes.push(BTreeMap::new());
                let result = (|| {
                    for statement in args[0].list()? {
                        cx.dispatch(self, statement.syntax()?)?;
                    }
                    if operation(hook) == "program" {
                        cx.dispatch(self, args[1].syntax()?)
                    } else {
                        Ok(0)
                    }
                })();
                self.scopes.pop();
                result
            }
            "conditional" => {
                let condition = cx.dispatch(self, args[0].syntax()?)?;
                cx.dispatch(self, args[if condition != 0 { 1 } else { 2 }].syntax()?)
            }
            "repeat" => {
                while cx.dispatch(self, args[0].syntax()?)? != 0 {
                    self.loop_iterations += 1;
                    if self.loop_iterations > 10_000 {
                        return Err(unknown("Example interpreter loop limit exceeded"));
                    }
                    cx.dispatch(self, args[1].syntax()?)?;
                }
                Ok(0)
            }
            _ => Err(unknown("Unsupported hook")),
        }
    }
}

#[derive(Default)]
pub struct JavaScript {
    scopes: Vec<BTreeMap<String, String>>,
    next_binding: usize,
}
impl JavaScript {
    fn binding(&self, name: &str) -> Result<String, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .ok_or_else(|| unknown(format!("Unknown variable {name}")))
    }
}
impl Backend for JavaScript {
    type Value = String;
    fn bindings(&self) -> Vec<HookBinding> {
        signatures()
    }
    fn invoke(
        &mut self,
        cx: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        args: &[Capture],
    ) -> Result<String, Diagnostic> {
        let kind = operation(hook);
        match kind {
            "number" => {
                let number = cx
                    .program()
                    .text(args[0].syntax()?)?
                    .parse::<i32>()
                    .map_err(|_| unknown("Integer literal is outside signed i32 range"))?;
                Ok(number.to_string())
            }
            "reference" => self.binding(cx.program().text(args[0].syntax()?)?),
            "add" | "multiply" | "less" => {
                let left = cx.dispatch(self, args[0].syntax()?)?;
                let right = cx.dispatch(self, args[1].syntax()?)?;
                Ok(match kind {
                    "add" => format!("(({left} + {right}) | 0)"),
                    "multiply" => format!("Math.imul({left}, {right})"),
                    _ => format!("(({left} < {right}) | 0)"),
                })
            }
            "declare" | "assign" => {
                let name = cx.program().text(args[0].syntax()?)?.to_owned();
                // Resolve the initializer before introducing the new lexical binding.
                let value = cx.dispatch(self, args[1].syntax()?)?;
                let binding = if kind == "declare" {
                    let scope = self
                        .scopes
                        .last_mut()
                        .ok_or_else(|| unknown("No lexical scope"))?;
                    if scope.contains_key(&name) {
                        return Err(unknown(format!("Duplicate variable {name}")));
                    }
                    let binding = format!("v_{name}_{}", self.next_binding);
                    self.next_binding += 1;
                    scope.insert(name, binding.clone());
                    binding
                } else {
                    self.binding(&name)?
                };
                Ok(format!(
                    "{}{binding} = {value};",
                    if kind == "declare" { "let " } else { "" }
                ))
            }
            "block" | "program" => {
                self.scopes.push(BTreeMap::new());
                let result = (|| {
                    let statements = args[0]
                        .list()?
                        .iter()
                        .map(|arg| cx.dispatch(self, arg.syntax()?))
                        .collect::<Result<Vec<_>, _>>()?
                        .join("\n");
                    if kind == "program" {
                        let result = cx.dispatch(self, args[1].syntax()?)?;
                        Ok(format!("function run() {{\n{}\n    return {result};\n}}\nconsole.log(run());\n", indent(&statements)))
                    } else {
                        Ok(format!("{{\n{}\n}}", indent(&statements)))
                    }
                })();
                self.scopes.pop();
                result
            }
            "conditional" => {
                let condition = cx.dispatch(self, args[0].syntax()?)?;
                let yes = cx.dispatch(self, args[1].syntax()?)?;
                let no = cx.dispatch(self, args[2].syntax()?)?;
                Ok(format!("if ({condition}) {yes} else {no}"))
            }
            "repeat" => {
                let condition = cx.dispatch(self, args[0].syntax()?)?;
                let body = cx.dispatch(self, args[1].syntax()?)?;
                Ok(format!("while ({condition}) {body}"))
            }
            _ => Err(unknown("Unsupported hook")),
        }
    }
}
fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Default)]
pub struct Wasm {
    scopes: Vec<BTreeMap<String, u32>>,
    locals: u32,
}
impl Wasm {
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
        let mut code = CodeSection::new();
        let mut function = Function::new([(self.locals, ValType::I32)]);
        for instruction in instructions {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);
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
            .ok_or_else(|| unknown(format!("Unknown variable {name}")))
    }
}
impl Backend for Wasm {
    type Value = Vec<Instruction<'static>>;
    fn bindings(&self) -> Vec<HookBinding> {
        signatures()
    }
    fn invoke(
        &mut self,
        cx: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        args: &[Capture],
    ) -> Result<Self::Value, Diagnostic> {
        let kind = operation(hook);
        let mut code = vec![];
        match kind {
            "number" => {
                let number = cx
                    .program()
                    .text(args[0].syntax()?)?
                    .parse::<i32>()
                    .map_err(|_| unknown("Integer literal is outside signed i32 range"))?;
                code.push(Instruction::I32Const(number));
            }
            "reference" => code.push(Instruction::LocalGet(
                self.local(cx.program().text(args[0].syntax()?)?)?,
            )),
            "add" | "multiply" | "less" => {
                code.extend(cx.dispatch(self, args[0].syntax()?)?);
                code.extend(cx.dispatch(self, args[1].syntax()?)?);
                code.push(match kind {
                    "add" => Instruction::I32Add,
                    "multiply" => Instruction::I32Mul,
                    _ => Instruction::I32LtS,
                });
            }
            "declare" | "assign" => {
                let name = cx.program().text(args[0].syntax()?)?.to_owned();
                code.extend(cx.dispatch(self, args[1].syntax()?)?);
                let local = if kind == "declare" {
                    let scope = self
                        .scopes
                        .last_mut()
                        .ok_or_else(|| unknown("No lexical scope"))?;
                    if scope.contains_key(&name) {
                        return Err(unknown(format!("Duplicate variable {name}")));
                    }
                    let local = self.locals;
                    self.locals += 1;
                    scope.insert(name, local);
                    local
                } else {
                    self.local(&name)?
                };
                code.push(Instruction::LocalSet(local));
            }
            "program" | "block" => {
                self.scopes.push(BTreeMap::new());
                let result = (|| {
                    for item in args[0].list()? {
                        code.extend(cx.dispatch(self, item.syntax()?)?);
                    }
                    if kind == "program" {
                        code.extend(cx.dispatch(self, args[1].syntax()?)?);
                    }
                    Ok(code)
                })();
                self.scopes.pop();
                return result;
            }
            "conditional" => {
                code.extend(cx.dispatch(self, args[0].syntax()?)?);
                code.push(Instruction::If(BlockType::Empty));
                code.extend(cx.dispatch(self, args[1].syntax()?)?);
                code.push(Instruction::Else);
                code.extend(cx.dispatch(self, args[2].syntax()?)?);
                code.push(Instruction::End);
            }
            "repeat" => {
                code.push(Instruction::Block(BlockType::Empty));
                code.push(Instruction::Loop(BlockType::Empty));
                code.extend(cx.dispatch(self, args[0].syntax()?)?);
                code.push(Instruction::I32Eqz);
                code.push(Instruction::BrIf(1));
                code.extend(cx.dispatch(self, args[1].syntax()?)?);
                code.push(Instruction::Br(0));
                code.push(Instruction::End);
                code.push(Instruction::End);
            }
            _ => return Err(unknown("Unsupported hook")),
        }
        Ok(code)
    }
}

pub fn source_argument() -> Result<(String, String), String> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/backend-hooks/programs/control-flow.txt".into());
    let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok((path, source))
}

pub fn render_errors(errors: &[Diagnostic], path: &str, source: &str) -> String {
    your_language::diagnostic_render::render_diagnostics(
        errors,
        |span| (span.file == path).then(|| source.to_owned()),
        your_language::diagnostic_render::stderr_color_enabled(),
    )
}
