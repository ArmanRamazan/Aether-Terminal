//! Cranelift IR generation and JIT compilation from typed AST.
//!
//! Each rule compiles to a native function with signature:
//! `fn(*const WorldStateFFI, u32) -> (matched, action_type, target_pid, severity)`.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::immediates::Ieee32;
use cranelift_codegen::ir::{
    types, AbiParam, Function, InstBuilder, MemFlags, Signature, UserFuncName,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

use crate::ast::{Action, CompareOp, Expr, Field, Rule, Value};
use crate::error::ScriptError;

/// C-compatible process state passed to compiled rules.
#[repr(C)]
pub struct WorldStateFFI {
    pub pid: u32,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    pub mem_growth_percent: f32,
    pub state: u32,
    pub hp: f32,
    // 4 bytes padding (implicit, for pointer alignment)
    pub name_ptr: *const u8,
    pub name_len: u32,
    pub process_count: u32,
    pub processes_ptr: *const WorldStateFFI,
}

/// C-compatible result returned by compiled rules.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleResult {
    /// 1 if the rule matched, 0 otherwise.
    pub matched: u32,
    /// Action type: 0=none, 1=alert, 2=kill, 3=log.
    pub action_type: u32,
    /// PID of the target process.
    pub target_pid: u32,
    /// Severity: 0=info, 1=warning, 2=critical.
    pub severity: u32,
}

/// Generates Cranelift IR from typed AST rules.
pub struct CodeGenerator {
    builder_ctx: FunctionBuilderContext,
}

impl Default for CodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGenerator {
    /// Create a new code generator.
    pub fn new() -> Self {
        Self {
            builder_ctx: FunctionBuilderContext::new(),
        }
    }

    /// Compile a rule AST into Cranelift IR.
    pub fn generate_rule(&mut self, rule: &Rule) -> Result<Function, ScriptError> {
        let sig = build_rule_signature();
        let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);

        {
            let mut builder = FunctionBuilder::new(&mut func, &mut self.builder_ctx);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);

            let state_ptr = builder.block_params(entry)[0];
            let result_ptr = builder.block_params(entry)[1];
            let matched = emit_condition(&mut builder, state_ptr, &rule.when_clause)?;
            emit_return(
                &mut builder,
                state_ptr,
                result_ptr,
                matched,
                &rule.then_clause,
            );
            builder.finalize();
        }

        verify(&func)?;
        Ok(func)
    }
}

/// Type alias for the compiled rule function signature.
///
/// Arguments: `(*const WorldStateFFI, *mut RuleResult)`.
/// The function writes its results into the RuleResult pointer. No return value.
type RuleFn = unsafe extern "C" fn(*const WorldStateFFI, *mut RuleResult);

/// A rule compiled to native machine code.
pub struct CompiledRule {
    /// Rule name from the DSL source.
    pub name: String,
    /// Native function pointer. Valid as long as the owning `JitCompiler` is alive.
    func_ptr: *const u8,
}

impl CompiledRule {
    /// Call the compiled rule against a process state.
    ///
    /// # Safety
    /// - `state` must point to a valid `WorldStateFFI` for the duration of the call.
    /// - The `JitCompiler` that produced this rule must still be alive (owns the code memory).
    pub unsafe fn call(&self, state: *const WorldStateFFI) -> RuleResult {
        // SAFETY: func_ptr was produced by JITModule::get_finalized_function,
        // which returns a valid function pointer matching our RuleFn signature.
        // The caller guarantees state is a valid pointer and the JIT module is alive.
        let func: RuleFn = unsafe { std::mem::transmute(self.func_ptr) };
        let mut result = RuleResult {
            matched: 0,
            action_type: 0,
            target_pid: 0,
            severity: 0,
        };
        unsafe { func(state, &mut result) };
        result
    }
}

/// JIT compiler that materializes Cranelift IR into executable machine code.
///
/// Owns the JIT memory — compiled rules are valid only while this struct lives.
pub struct JitCompiler {
    module: JITModule,
}

impl JitCompiler {
    /// Create a new JIT compiler for the host platform.
    pub fn new() -> Result<Self, ScriptError> {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| ScriptError::Compile(e.to_string()))?;
        let module = JITModule::new(builder);
        Ok(Self { module })
    }

    /// Compile a Cranelift IR function to native code, returning the function pointer.
    pub fn compile(&mut self, func: Function) -> Result<*const u8, ScriptError> {
        let func_id = self
            .module
            .declare_anonymous_function(&func.signature)
            .map_err(|e| ScriptError::Compile(e.to_string()))?;

        let mut ctx = Context::for_function(func);
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| ScriptError::Compile(e.to_string()))?;

        self.module
            .finalize_definitions()
            .map_err(|e| ScriptError::Compile(format!("JIT finalization failed: {e}")))?;

        let code_ptr = self.module.get_finalized_function(func_id);
        Ok(code_ptr)
    }

    /// Compile a rule AST end-to-end: generate IR, then JIT compile to native code.
    pub fn compile_rule(
        &mut self,
        codegen: &mut CodeGenerator,
        rule: &Rule,
    ) -> Result<CompiledRule, ScriptError> {
        let func = codegen.generate_rule(rule)?;
        let func_ptr = self.compile(func)?;
        Ok(CompiledRule {
            name: rule.name.clone(),
            func_ptr,
        })
    }
}

/// Build the function signature: `fn(*const WorldStateFFI, *mut RuleResult)`.
///
/// Uses struct-return via output pointer to avoid multi-value return limitations.
fn build_rule_signature() -> Signature {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I64)); // *const WorldStateFFI
    sig.params.push(AbiParam::new(types::I64)); // *mut RuleResult (output)
    sig
}

/// Emit the return sequence: write matched/action/pid/severity to result pointer.
///
/// RuleResult layout: `{ matched: u32, action_type: u32, target_pid: u32, severity: u32 }`.
fn emit_return(
    builder: &mut FunctionBuilder,
    state_ptr: cranelift_codegen::ir::Value,
    result_ptr: cranelift_codegen::ir::Value,
    matched: cranelift_codegen::ir::Value,
    action: &Action,
) {
    let (action_val, severity_val) = action_constants(builder, action);
    let pid = builder
        .ins()
        .load(types::I32, MemFlags::new(), state_ptr, 0i32);
    let matched_i32 = builder.ins().uextend(types::I32, matched);
    let zero = builder.ins().iconst(types::I32, 0);
    let final_action = builder.ins().select(matched, action_val, zero);
    let final_pid = builder.ins().select(matched, pid, zero);
    let final_sev = builder.ins().select(matched, severity_val, zero);

    let flags = MemFlags::new();
    builder.ins().store(flags, matched_i32, result_ptr, 0i32); // offset 0: matched
    builder.ins().store(flags, final_action, result_ptr, 4i32); // offset 4: action_type
    builder.ins().store(flags, final_pid, result_ptr, 8i32); // offset 8: target_pid
    builder.ins().store(flags, final_sev, result_ptr, 12i32); // offset 12: severity

    builder.ins().return_(&[]);
}

/// Emit integer constants for the action type and severity.
fn action_constants(
    builder: &mut FunctionBuilder,
    action: &Action,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match action {
        Action::Alert { severity } => {
            let a = builder.ins().iconst(types::I32, 1);
            let s = builder.ins().iconst(types::I32, severity.as_i64());
            (a, s)
        }
        Action::Kill => {
            let a = builder.ins().iconst(types::I32, 2);
            let s = builder.ins().iconst(types::I32, 0);
            (a, s)
        }
        Action::Log => {
            let a = builder.ins().iconst(types::I32, 3);
            let s = builder.ins().iconst(types::I32, 0);
            (a, s)
        }
    }
}

/// Recursively emit condition IR, returning an I8 boolean value.
fn emit_condition(
    builder: &mut FunctionBuilder,
    state_ptr: cranelift_codegen::ir::Value,
    expr: &Expr,
) -> Result<cranelift_codegen::ir::Value, ScriptError> {
    match expr {
        Expr::Comparison { field, op, value } => {
            emit_comparison(builder, state_ptr, *field, *op, *value)
        }
        Expr::And(left, right) => {
            let l = emit_condition(builder, state_ptr, left)?;
            let r = emit_condition(builder, state_ptr, right)?;
            Ok(builder.ins().band(l, r))
        }
        Expr::Or(left, right) => {
            let l = emit_condition(builder, state_ptr, left)?;
            let r = emit_condition(builder, state_ptr, right)?;
            Ok(builder.ins().bor(l, r))
        }
    }
}

/// Emit a single field comparison, returning an I8 boolean.
fn emit_comparison(
    builder: &mut FunctionBuilder,
    state_ptr: cranelift_codegen::ir::Value,
    field: Field,
    op: CompareOp,
    value: Value,
) -> Result<cranelift_codegen::ir::Value, ScriptError> {
    let (offset, ty) = field.offset_and_type();
    let loaded = builder.ins().load(ty, MemFlags::new(), state_ptr, offset);

    match (ty, value) {
        (t, Value::Int(n)) if t == types::I32 => {
            let rhs = builder.ins().iconst(types::I32, n);
            Ok(builder.ins().icmp(int_cc(op), loaded, rhs))
        }
        (t, Value::Int(n)) if t == types::I64 => {
            let rhs = builder.ins().iconst(types::I64, n);
            Ok(builder.ins().icmp(int_cc(op), loaded, rhs))
        }
        (t, Value::Float(f)) if t == types::F32 => {
            let rhs = builder.ins().f32const(Ieee32::with_float(f as f32));
            Ok(builder.ins().fcmp(float_cc(op), loaded, rhs))
        }
        _ => Err(ScriptError::Compile(format!(
            "type mismatch: field {field:?} cannot be compared with {value:?}"
        ))),
    }
}

/// Map comparison operator to Cranelift integer condition code.
fn int_cc(op: CompareOp) -> IntCC {
    match op {
        CompareOp::Gt => IntCC::SignedGreaterThan,
        CompareOp::Lt => IntCC::SignedLessThan,
        CompareOp::Gte => IntCC::SignedGreaterThanOrEqual,
        CompareOp::Lte => IntCC::SignedLessThanOrEqual,
        CompareOp::Eq => IntCC::Equal,
        CompareOp::Neq => IntCC::NotEqual,
    }
}

/// Map comparison operator to Cranelift float condition code.
fn float_cc(op: CompareOp) -> FloatCC {
    match op {
        CompareOp::Gt => FloatCC::GreaterThan,
        CompareOp::Lt => FloatCC::LessThan,
        CompareOp::Gte => FloatCC::GreaterThanOrEqual,
        CompareOp::Lte => FloatCC::LessThanOrEqual,
        CompareOp::Eq => FloatCC::Equal,
        CompareOp::Neq => FloatCC::NotEqual,
    }
}

/// Verify the generated IR against Cranelift's verifier.
fn verify(func: &Function) -> Result<(), ScriptError> {
    let flags = settings::Flags::new(settings::builder());
    verify_function(func, &flags).map_err(|e| ScriptError::Compile(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, CompareOp, Expr, Field, Rule, Severity, Value};

    fn simple_rule() -> Rule {
        Rule {
            name: "high_cpu".to_string(),
            when_clause: Expr::Comparison {
                field: Field::CpuPercent,
                op: CompareOp::Gt,
                value: Value::Float(90.0),
            },
            duration: None,
            then_clause: Action::Alert {
                severity: Severity::Warning,
            },
        }
    }

    #[test]
    fn test_generate_ir_simple_comparison() {
        let mut gen = CodeGenerator::new();
        let func = gen
            .generate_rule(&simple_rule())
            .expect("IR generation failed");

        let ir = func.to_string();
        assert!(ir.contains("f32const"), "should load f32 constant");
        assert!(ir.contains("fcmp"), "should have float comparison");
        assert!(
            ir.contains("store"),
            "should store results to output pointer"
        );
        assert!(ir.contains("return"), "should have return instruction");
    }

    #[test]
    fn test_generated_ir_passes_verifier() {
        let mut gen = CodeGenerator::new();
        let func = gen
            .generate_rule(&simple_rule())
            .expect("IR generation failed");

        let flags = settings::Flags::new(settings::builder());
        verify_function(&func, &flags).expect("IR verification failed");
    }

    #[test]
    fn test_generate_ir_compound_conditions() {
        let rule = Rule {
            name: "compound".to_string(),
            duration: None,
            when_clause: Expr::And(
                Box::new(Expr::Comparison {
                    field: Field::CpuPercent,
                    op: CompareOp::Gt,
                    value: Value::Float(80.0),
                }),
                Box::new(Expr::Or(
                    Box::new(Expr::Comparison {
                        field: Field::MemBytes,
                        op: CompareOp::Gt,
                        value: Value::Int(1_000_000_000),
                    }),
                    Box::new(Expr::Comparison {
                        field: Field::State,
                        op: CompareOp::Eq,
                        value: Value::Int(2), // Zombie
                    }),
                )),
            ),
            then_clause: Action::Kill,
        };

        let mut gen = CodeGenerator::new();
        let func = gen.generate_rule(&rule).expect("IR generation failed");

        let ir = func.to_string();
        assert!(ir.contains("fcmp"), "should have float comparison for cpu");
        assert!(
            ir.contains("icmp"),
            "should have int comparison for mem/state"
        );
        assert!(ir.contains("band"), "should have AND operation");
        assert!(ir.contains("bor"), "should have OR operation");

        let flags = settings::Flags::new(settings::builder());
        verify_function(&func, &flags).expect("compound IR verification failed");
    }
}
