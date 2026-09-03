use crate::compiler::Compiler;
use crate::errors::{Severity, SourceError};
use crate::parser::{AstNode, NodeId};
use nu_protocol::ast::{
    Bits, Boolean, CellPath, Comparison, Math, Operator, PathMember, RangeInclusion,
};
use nu_protocol::casing::Casing;
use nu_protocol::ir::{DataSlice, Instruction, IrBlock, Literal, RedirectMode};
use nu_protocol::{
    BlockId as NuBlockId, DeclId as NuDeclId, RegId, Span, VarId as NuVarId, ENV_VARIABLE_ID,
    IN_VARIABLE_ID, NU_VARIABLE_ID,
};

const PLACEHOLDER_INDEX: usize = usize::MAX;

#[derive(Debug)]
struct LoopContext {
    continue_target: usize,
    break_jumps: Vec<usize>,
}

#[derive(Debug)]
struct OperatorPlan {
    operator: Operator,
    negate: bool,
}

enum CompiledArg {
    Positional(RegId, NodeId),
    Spread(RegId, NodeId),
    LongFlag(Vec<u8>, NodeId),
    ShortFlag(Vec<u8>, NodeId),
    ShortGroup(Vec<Vec<u8>>, NodeId),
    LongNamed(Vec<u8>, RegId, NodeId),
    ShortNamed(Vec<u8>, RegId, NodeId),
}

/// Generates IR (Intermediate Representation) from nu AST.
pub struct IrGenerator<'a> {
    // Immutable reference to a compiler after the typechecker pass
    compiler: &'a Compiler,
    var_map: Option<&'a [Option<NuVarId>]>,
    decl_map: Option<&'a [Option<NuDeclId>]>,
    block_map: Option<&'a [Option<NuBlockId>]>,
    run_external_decl: Option<NuDeclId>,
    errors: Vec<SourceError>,
    block: IrBlock,
    data: Vec<u8>,
    loop_stack: Vec<LoopContext>,
}

impl<'a> IrGenerator<'a> {
    pub fn new(compiler: &'a Compiler) -> Self {
        Self {
            compiler,
            var_map: None,
            decl_map: None,
            block_map: None,
            run_external_decl: None,
            errors: Default::default(),
            block: IrBlock {
                instructions: Default::default(),
                spans: Default::default(),
                data: Default::default(),
                ast: Default::default(),
                comments: Default::default(),
                register_count: 0,
                file_count: 0,
                scope_regions: Default::default(),
            },
            data: Vec::new(),
            loop_stack: Vec::new(),
        }
    }

    pub fn with_id_maps(
        compiler: &'a Compiler,
        var_map: &'a [Option<NuVarId>],
        decl_map: &'a [Option<NuDeclId>],
        block_map: &'a [Option<NuBlockId>],
    ) -> Self {
        let mut generator = Self::new(compiler);
        generator.var_map = Some(var_map);
        generator.decl_map = Some(decl_map);
        generator.block_map = Some(block_map);
        generator
    }

    pub fn with_run_external_decl(mut self, decl_id: Option<NuDeclId>) -> Self {
        self.run_external_decl = decl_id;
        self
    }

    /// Generates the IR from the given state of the compiler.
    /// After this is called, use `block` and `errors` to get the result.
    pub fn generate(&mut self) {
        if self.compiler.ast_nodes.is_empty() {
            return;
        }

        let node_id = NodeId(self.compiler.ast_nodes.len() - 1);
        self.generate_for_node(node_id);
    }

    /// Generates IR for a specific AST node.
    ///
    /// This is used by the runtime bridge to lower nested blocks before installing
    /// them into Nushell's EngineState.
    pub fn generate_for_node(&mut self, node_id: NodeId) {
        let reg = self.generate_node(node_id);
        self.add_instruction(node_id, Instruction::Return { src: reg });
        self.block.data = self.data.clone().into();
    }

    /// Returns generated IR block.
    ///
    /// Call `generate` before using this method and ensure there are no errors.
    pub fn block(self) -> IrBlock {
        self.block
    }

    /// Returns errors encountered during IR generation step.
    ///
    /// Call `generate` before using this method.
    pub fn errors(&self) -> &Vec<SourceError> {
        &self.errors
    }

    /// Prints the internal state to standard output.
    pub fn print(&self) {
        let output = self.display_state();
        print!("{output}");
    }

    /// Displays the state of the IR generator.
    /// The output can be used for human debugging and for snapshot tests.
    pub fn display_state(&self) -> String {
        let mut result = String::new();
        result.push_str("==== IR ====\n");
        result.push_str(&format!("register_count: {}\n", self.block.register_count));
        result.push_str(&format!("file_count: {}\n", self.block.file_count));

        for (idx, instruction) in self.block.instructions.iter().enumerate() {
            result.push_str(&format!("{}: {:?}\n", idx, instruction));
        }

        if !self.errors.is_empty() {
            result.push_str("==== IR ERRORS ====\n");
            for error in &self.errors {
                result.push_str(&format!(
                    "{:?} (NodeId {}): {}\n",
                    error.severity, error.node_id.0, error.message
                ));
            }
        }
        result
    }

    // Returns unused register.
    fn next_register(&mut self) -> RegId {
        let r = RegId::new(self.block.register_count);
        self.block.register_count += 1;
        r
    }

    fn generate_node(&mut self, node_id: NodeId) -> RegId {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Int => self.generate_int(node_id),
            AstNode::Float => self.generate_float(node_id),
            AstNode::String => self.load_string(node_id),
            AstNode::Name => self.generate_name(node_id),
            AstNode::Variable => self.load_variable(node_id),
            AstNode::True => self.load_literal(node_id, Literal::Bool(true)),
            AstNode::False => self.load_literal(node_id, Literal::Bool(false)),
            AstNode::Null => self.load_nothing(node_id),

            AstNode::Let {
                variable_name,
                initializer,
                ..
            }
            | AstNode::Const {
                variable_name,
                initializer,
                ..
            } => self.generate_binding(variable_name, initializer, node_id),

            AstNode::While { condition, block } => self.generate_while(condition, block, node_id),
            AstNode::For {
                variable,
                range,
                block,
            } => self.generate_for(variable, range, block, node_id),
            AstNode::Loop { block } => self.generate_loop(block, node_id),
            AstNode::Return(value) => self.generate_return(value, node_id),
            AstNode::Break => self.generate_break(node_id),
            AstNode::Continue => self.generate_continue(node_id),

            AstNode::Closure { block, .. } => self.generate_closure(block, node_id),
            AstNode::Call(_) => self.generate_call(node_id, None),
            AstNode::NamedValue { value, .. } => self.generate_node(value),
            AstNode::UnaryOp { op, expr } => self.generate_unary(op, expr, node_id),
            AstNode::BinaryOp { lhs, op, rhs } => self.generate_binary(lhs, op, rhs, node_id),
            AstNode::Range { lhs, rhs } => self.generate_range(lhs, rhs, node_id),
            AstNode::List(_) => self.generate_list(node_id),
            AstNode::Table(_) => self.generate_table(node_id),
            AstNode::Record(_) => self.generate_record(node_id),
            AstNode::MemberAccess { .. } => self.generate_member_access(node_id),
            AstNode::Block(_) => self.generate_block(node_id),
            AstNode::Pipeline(_) => self.generate_pipeline(node_id),
            AstNode::If {
                condition,
                then_block,
                else_block,
            } => self.generate_if(condition, then_block, else_block, node_id),
            AstNode::Try {
                try_block,
                catch_block,
                finally_block,
            } => self.generate_try(try_block, catch_block, finally_block, node_id),
            AstNode::Match(_) => self.generate_match(node_id),
            AstNode::Statement(inner) => self.generate_statement(inner, node_id),
            AstNode::Spread(expr) => self.generate_node(expr),
            AstNode::Redirection { source, op, target } => {
                self.generate_redirection(source, op, target, node_id, None)
            }
            AstNode::EnvAssignment { name, value } => {
                self.generate_env_assignment(name, value, node_id)
            }
            AstNode::Export { declaration } => self.generate_node(declaration),

            AstNode::FlagLong
            | AstNode::FlagShort
            | AstNode::FlagShortGroup
            | AstNode::Type { .. }
            | AstNode::TypeArgs(_)
            | AstNode::RecordType { .. }
            | AstNode::Params(_)
            | AstNode::Param { .. }
            | AstNode::InOutTypes(_)
            | AstNode::InOutType(_, _)
            | AstNode::Def { .. }
            | AstNode::Extern { .. }
            | AstNode::Alias { .. }
            | AstNode::Module { .. }
            | AstNode::Use { .. }
            | AstNode::Source { .. }
            | AstNode::ExportEnv { .. }
            | AstNode::Hide { .. }
            | AstNode::Overlay { .. }
            | AstNode::PluginUse { .. } => self.load_nothing(node_id),

            AstNode::Pow
            | AstNode::Multiply
            | AstNode::Divide
            | AstNode::FloorDiv
            | AstNode::Modulo
            | AstNode::Plus
            | AstNode::Minus
            | AstNode::Equal
            | AstNode::NotEqual
            | AstNode::LessThan
            | AstNode::GreaterThan
            | AstNode::LessThanOrEqual
            | AstNode::GreaterThanOrEqual
            | AstNode::RegexMatch
            | AstNode::NotRegexMatch
            | AstNode::In
            | AstNode::NotIn
            | AstNode::Has
            | AstNode::NotHas
            | AstNode::Like
            | AstNode::NotLike
            | AstNode::StartsWith
            | AstNode::NotStartsWith
            | AstNode::EndsWith
            | AstNode::NotEndsWith
            | AstNode::Append
            | AstNode::BitOr
            | AstNode::BitXor
            | AstNode::BitAnd
            | AstNode::BitShiftLeft
            | AstNode::BitShiftRight
            | AstNode::And
            | AstNode::Xor
            | AstNode::Or
            | AstNode::Not
            | AstNode::Assignment
            | AstNode::AddAssignment
            | AstNode::SubtractAssignment
            | AstNode::MultiplyAssignment
            | AstNode::DivideAssignment
            | AstNode::AppendAssignment => self.load_string(node_id),

            AstNode::Garbage => {
                self.error("garbage AST node cannot be lowered to IR", node_id);
                self.load_nothing(node_id)
            }
        }
    }

    fn generate_node_with_input(&mut self, node_id: NodeId, input: Option<RegId>) -> RegId {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Call(_) => self.generate_call(node_id, input),
            AstNode::Redirection { source, op, target } => {
                self.generate_redirection(source, op, target, node_id, input)
            }
            _ => {
                if let Some(input) = input {
                    self.add_instruction(node_id, Instruction::Drain { src: input });
                }
                self.generate_node(node_id)
            }
        }
    }

    fn generate_int(&mut self, node_id: NodeId) -> RegId {
        let text = self.node_source_string(node_id);
        match parse_int_literal(&text) {
            Ok(value) => self.load_literal(node_id, Literal::Int(value)),
            Err(message) => {
                self.error(message, node_id);
                self.load_literal(node_id, Literal::Int(0))
            }
        }
    }

    fn generate_float(&mut self, node_id: NodeId) -> RegId {
        let text = self.node_source_string(node_id).replace('_', "");
        match text.parse::<f64>() {
            Ok(value) => self.load_literal(node_id, Literal::Float(value)),
            Err(err) => {
                self.error(format!("invalid float literal: {err}"), node_id);
                self.load_literal(node_id, Literal::Float(0.0))
            }
        }
    }

    fn generate_name(&mut self, node_id: NodeId) -> RegId {
        let text = self.node_source_string(node_id);
        if text.starts_with("$.") || text.starts_with('.') {
            self.load_cell_path_literal(node_id)
        } else {
            self.load_string(node_id)
        }
    }

    fn generate_binding(
        &mut self,
        variable_name: NodeId,
        initializer: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let initializer = self.generate_node(initializer);
        let var_id = self.variable_id(variable_name);
        self.add_instruction(
            node_id,
            Instruction::StoreVariable {
                var_id,
                src: initializer,
            },
        );
        self.load_nothing(node_id)
    }

    fn generate_statement(&mut self, inner: NodeId, node_id: NodeId) -> RegId {
        let reg = self.generate_node(inner);
        self.add_instruction(node_id, Instruction::Drain { src: reg });
        self.load_nothing(node_id)
    }

    fn generate_block(&mut self, node_id: NodeId) -> RegId {
        let nodes = self.compiler.get_block(node_id).nodes.clone();

        if nodes.is_empty() {
            return self.load_nothing(node_id);
        }

        let mut last = None;
        for (idx, child) in nodes.iter().copied().enumerate() {
            let is_last = idx + 1 == nodes.len();
            if let AstNode::Statement(inner) = self.compiler.ast_nodes[child.0] {
                let reg = self.generate_node(inner);
                self.add_instruction(child, Instruction::Drain { src: reg });
                if is_last {
                    return self.load_nothing(child);
                }
                continue;
            }

            let reg = self.generate_node(child);
            if is_last {
                last = Some(reg);
            } else {
                self.add_instruction(child, Instruction::Drain { src: reg });
            }
        }

        last.unwrap_or_else(|| self.load_nothing(node_id))
    }

    fn generate_pipeline(&mut self, node_id: NodeId) -> RegId {
        let AstNode::Pipeline(pipeline_id) = self.compiler.ast_nodes[node_id.0] else {
            unreachable!("internal error: expected pipeline");
        };
        let nodes = self.compiler.pipelines[pipeline_id.0].nodes.clone();
        let mut input = None;

        for (idx, child) in nodes.iter().copied().enumerate() {
            let reg = if idx == 0 {
                self.generate_node(child)
            } else {
                self.generate_node_with_input(child, input.take())
            };

            if idx + 1 == nodes.len() {
                return reg;
            }

            input = Some(reg);
        }

        self.load_nothing(node_id)
    }

    fn generate_call(&mut self, node_id: NodeId, input: Option<RegId>) -> RegId {
        let parts = self.compiler.get_call(node_id).parts.clone();
        let Some(decl_id) = self.compiler.decl_resolution.get(&node_id).copied() else {
            return self.generate_external_call_placeholder(node_id, &parts, input);
        };

        let name_parts = self.call_name_part_count(node_id, decl_id, &parts);
        if let Some(run_external_decl) = self.mapped_run_external_decl(decl_id) {
            return self.generate_run_external_call(
                node_id,
                &parts[name_parts..],
                input,
                run_external_decl,
            );
        }

        let mut compiled_args = Vec::new();
        for arg in parts.iter().copied().skip(name_parts) {
            self.compile_call_arg(arg, &mut compiled_args);
        }

        for arg in compiled_args {
            self.push_call_arg(arg);
        }

        let io_reg = input.unwrap_or_else(|| self.load_nothing(node_id));
        let decl_id = self.map_decl_id(decl_id, node_id);
        self.add_instruction(
            node_id,
            Instruction::Call {
                decl_id,
                src_dst: io_reg,
            },
        );
        io_reg
    }

    fn generate_external_call_placeholder(
        &mut self,
        node_id: NodeId,
        parts: &[NodeId],
        input: Option<RegId>,
    ) -> RegId {
        if let Some(decl_id) = self.run_external_decl {
            return self.generate_run_external_call(node_id, parts, input, decl_id);
        }

        if let Some(input) = input {
            self.add_instruction(node_id, Instruction::Drain { src: input });
        }

        let out = self.load_literal(
            node_id,
            Literal::List {
                capacity: parts.len(),
            },
        );

        for part in parts {
            match self.compiler.ast_nodes[part.0] {
                AstNode::Spread(expr) => {
                    let items = self.generate_node(expr);
                    self.add_instruction(
                        *part,
                        Instruction::ListSpread {
                            src_dst: out,
                            items,
                        },
                    );
                }
                AstNode::Name
                | AstNode::FlagLong
                | AstNode::FlagShort
                | AstNode::FlagShortGroup
                | AstNode::NamedValue { .. } => {
                    let item = self.load_source_string(*part);
                    self.add_instruction(*part, Instruction::ListPush { src_dst: out, item });
                }
                _ => {
                    let item = self.generate_node(*part);
                    self.add_instruction(*part, Instruction::ListPush { src_dst: out, item });
                }
            }
        }

        out
    }

    fn generate_run_external_call(
        &mut self,
        node_id: NodeId,
        parts: &[NodeId],
        input: Option<RegId>,
        decl_id: NuDeclId,
    ) -> RegId {
        let Some((head, args)) = parts.split_first() else {
            self.error("external call missing command name", node_id);
            return input.unwrap_or_else(|| self.load_nothing(node_id));
        };

        let command = self.load_external_command_name(*head);
        self.add_instruction(*head, Instruction::PushPositional { src: command });

        for arg in args {
            self.push_external_arg(*arg);
        }

        let io_reg = input.unwrap_or_else(|| self.load_nothing(node_id));
        self.add_instruction(
            node_id,
            Instruction::Call {
                decl_id,
                src_dst: io_reg,
            },
        );
        io_reg
    }

    fn push_external_arg(&mut self, node_id: NodeId) {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Spread(expr) => {
                let src = self.generate_node(expr);
                self.add_instruction(node_id, Instruction::AppendRest { src });
            }
            AstNode::FlagLong
            | AstNode::FlagShort
            | AstNode::FlagShortGroup
            | AstNode::NamedValue { .. } => {
                let src = self.load_source_string(node_id);
                self.add_instruction(node_id, Instruction::PushPositional { src });
            }
            _ => {
                let src = self.generate_node(node_id);
                self.add_instruction(node_id, Instruction::PushPositional { src });
            }
        }
    }

    fn compile_call_arg(&mut self, node_id: NodeId, compiled_args: &mut Vec<CompiledArg>) {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::NamedValue { name, value } => {
                let value_reg = self.generate_node(value);
                match self.compiler.ast_nodes[name.0] {
                    AstNode::FlagLong => {
                        compiled_args.push(CompiledArg::LongNamed(
                            self.flag_name_bytes(name),
                            value_reg,
                            node_id,
                        ));
                    }
                    AstNode::FlagShort | AstNode::FlagShortGroup => {
                        compiled_args.push(CompiledArg::ShortNamed(
                            self.flag_name_bytes(name),
                            value_reg,
                            node_id,
                        ));
                    }
                    _ => compiled_args.push(CompiledArg::Positional(value_reg, node_id)),
                }
            }
            AstNode::FlagLong => {
                compiled_args.push(CompiledArg::LongFlag(
                    self.flag_name_bytes(node_id),
                    node_id,
                ));
            }
            AstNode::FlagShort => {
                compiled_args.push(CompiledArg::ShortFlag(
                    self.flag_name_bytes(node_id),
                    node_id,
                ));
            }
            AstNode::FlagShortGroup => {
                compiled_args.push(CompiledArg::ShortGroup(
                    self.short_group_names(node_id),
                    node_id,
                ));
            }
            AstNode::Spread(expr) => {
                let reg = self.generate_node(expr);
                compiled_args.push(CompiledArg::Spread(reg, node_id));
            }
            _ => {
                let reg = self.generate_node(node_id);
                compiled_args.push(CompiledArg::Positional(reg, node_id));
            }
        }
    }

    fn push_call_arg(&mut self, arg: CompiledArg) {
        match arg {
            CompiledArg::Positional(src, node_id) => {
                self.add_instruction(node_id, Instruction::PushPositional { src });
            }
            CompiledArg::Spread(src, node_id) => {
                self.add_instruction(node_id, Instruction::AppendRest { src });
            }
            CompiledArg::LongFlag(name, node_id) => {
                let name = self.add_data(node_id, &name);
                self.add_instruction(node_id, Instruction::PushFlag { name });
            }
            CompiledArg::ShortFlag(short, node_id) => {
                let short = self.add_data(node_id, &short);
                self.add_instruction(node_id, Instruction::PushShortFlag { short });
            }
            CompiledArg::ShortGroup(shorts, node_id) => {
                for short in shorts {
                    let short = self.add_data(node_id, &short);
                    self.add_instruction(node_id, Instruction::PushShortFlag { short });
                }
            }
            CompiledArg::LongNamed(name, src, node_id) => {
                let name = self.add_data(node_id, &name);
                self.add_instruction(node_id, Instruction::PushNamed { name, src });
            }
            CompiledArg::ShortNamed(short, src, node_id) => {
                let short = self.add_data(node_id, &short);
                self.add_instruction(node_id, Instruction::PushShortNamed { short, src });
            }
        }
    }

    fn generate_unary(&mut self, op: NodeId, expr: NodeId, node_id: NodeId) -> RegId {
        match self.compiler.ast_nodes[op.0] {
            AstNode::Not => {
                let reg = self.generate_node(expr);
                self.add_instruction(node_id, Instruction::Not { src_dst: reg });
                reg
            }
            AstNode::Plus => self.generate_node(expr),
            AstNode::Minus => {
                let zero = self.load_literal(op, Literal::Int(0));
                let rhs = self.generate_node(expr);
                self.add_instruction(
                    node_id,
                    Instruction::BinaryOp {
                        lhs_dst: zero,
                        op: Operator::Math(Math::Subtract),
                        rhs,
                    },
                );
                zero
            }
            _ => self.generate_node(expr),
        }
    }

    fn generate_binary(&mut self, lhs: NodeId, op: NodeId, rhs: NodeId, node_id: NodeId) -> RegId {
        if self.is_assignment_operator(op) {
            return self.generate_assignment(lhs, op, rhs, node_id);
        }

        let lhs_reg = self.generate_node(lhs);
        let rhs_reg = self.generate_node(rhs);
        let Some(plan) = self.node_to_operator(op) else {
            return self.load_nothing(node_id);
        };

        self.add_instruction(
            node_id,
            Instruction::BinaryOp {
                lhs_dst: lhs_reg,
                op: plan.operator,
                rhs: rhs_reg,
            },
        );
        if plan.negate {
            self.add_instruction(node_id, Instruction::Not { src_dst: lhs_reg });
        }
        lhs_reg
    }

    fn generate_assignment(
        &mut self,
        lhs: NodeId,
        op: NodeId,
        rhs: NodeId,
        node_id: NodeId,
    ) -> RegId {
        match self.compiler.ast_nodes[lhs.0] {
            AstNode::Variable => self.generate_variable_assignment(lhs, op, rhs, node_id),
            AstNode::MemberAccess { .. } => self.generate_member_assignment(lhs, op, rhs, node_id),
            AstNode::Name => self.generate_env_name_assignment(lhs, op, rhs, node_id),
            _ => {
                self.error("invalid assignment target", lhs);
                self.load_nothing(node_id)
            }
        }
    }

    fn generate_variable_assignment(
        &mut self,
        lhs: NodeId,
        op: NodeId,
        rhs: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let value = if matches!(self.compiler.ast_nodes[op.0], AstNode::Assignment) {
            self.generate_node(rhs)
        } else {
            let current = self.load_variable(lhs);
            let rhs = self.generate_node(rhs);
            let Some(operator) = self.assignment_operator_to_binary(op) else {
                return self.load_nothing(node_id);
            };
            self.add_instruction(
                node_id,
                Instruction::BinaryOp {
                    lhs_dst: current,
                    op: operator,
                    rhs,
                },
            );
            current
        };

        let var_id = self.variable_id(lhs);
        self.add_instruction(node_id, Instruction::StoreVariable { var_id, src: value });
        self.load_nothing(node_id)
    }

    fn generate_member_assignment(
        &mut self,
        lhs: NodeId,
        op: NodeId,
        rhs: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let (root, members) = self.member_access_parts(lhs);
        if members.is_empty() {
            self.error("invalid cell-path assignment target", lhs);
            return self.load_nothing(node_id);
        }

        if self.is_env_variable(root) {
            return self.generate_env_member_assignment(root, &members, op, rhs, node_id);
        }

        let base = self.generate_node(root);
        let path = self.load_cell_path(node_id, members);
        let new_value = if matches!(self.compiler.ast_nodes[op.0], AstNode::Assignment) {
            self.generate_node(rhs)
        } else {
            let path_for_clone = self.clone_register(path, lhs);
            let current = self.next_register();
            self.add_instruction(
                lhs,
                Instruction::CloneCellPath {
                    dst: current,
                    src: base,
                    path: path_for_clone,
                },
            );
            let rhs = self.generate_node(rhs);
            let Some(operator) = self.assignment_operator_to_binary(op) else {
                return self.load_nothing(node_id);
            };
            self.add_instruction(
                node_id,
                Instruction::BinaryOp {
                    lhs_dst: current,
                    op: operator,
                    rhs,
                },
            );
            current
        };

        self.add_instruction(
            node_id,
            Instruction::UpsertCellPath {
                src_dst: base,
                path,
                new_value,
            },
        );

        if matches!(self.compiler.ast_nodes[root.0], AstNode::Variable) {
            let var_id = self.variable_id(root);
            self.add_instruction(node_id, Instruction::StoreVariable { var_id, src: base });
        }

        self.load_nothing(node_id)
    }

    fn generate_env_member_assignment(
        &mut self,
        _root: NodeId,
        members: &[PathMember],
        op: NodeId,
        rhs: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let key = self.member_to_key(&members[0]);
        let key_slice = self.add_data(node_id, key.as_bytes());

        let value =
            if members.len() == 1 && matches!(self.compiler.ast_nodes[op.0], AstNode::Assignment) {
                self.generate_node(rhs)
            } else {
                let base = self.next_register();
                self.add_instruction(
                    node_id,
                    Instruction::LoadEnvOpt {
                        dst: base,
                        key: key_slice,
                    },
                );

                let new_value = if matches!(self.compiler.ast_nodes[op.0], AstNode::Assignment) {
                    self.generate_node(rhs)
                } else {
                    let current = if members.len() == 1 {
                        base
                    } else {
                        let path = self.load_cell_path(node_id, members[1..].to_vec());
                        let current = self.next_register();
                        self.add_instruction(
                            node_id,
                            Instruction::CloneCellPath {
                                dst: current,
                                src: base,
                                path,
                            },
                        );
                        current
                    };
                    let rhs = self.generate_node(rhs);
                    let Some(operator) = self.assignment_operator_to_binary(op) else {
                        return self.load_nothing(node_id);
                    };
                    self.add_instruction(
                        node_id,
                        Instruction::BinaryOp {
                            lhs_dst: current,
                            op: operator,
                            rhs,
                        },
                    );
                    current
                };

                if members.len() > 1 {
                    let path = self.load_cell_path(node_id, members[1..].to_vec());
                    self.add_instruction(
                        node_id,
                        Instruction::UpsertCellPath {
                            src_dst: base,
                            path,
                            new_value,
                        },
                    );
                    base
                } else {
                    new_value
                }
            };

        self.add_instruction(
            node_id,
            Instruction::StoreEnv {
                key: key_slice,
                src: value,
            },
        );
        self.load_nothing(node_id)
    }

    fn generate_env_name_assignment(
        &mut self,
        name: NodeId,
        op: NodeId,
        rhs: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let key = self.node_source_string(name);
        let key_slice = self.add_data(name, key.as_bytes());
        let value = if matches!(self.compiler.ast_nodes[op.0], AstNode::Assignment) {
            self.generate_node(rhs)
        } else {
            let current = self.next_register();
            self.add_instruction(
                name,
                Instruction::LoadEnvOpt {
                    dst: current,
                    key: key_slice,
                },
            );
            let rhs = self.generate_node(rhs);
            let Some(operator) = self.assignment_operator_to_binary(op) else {
                return self.load_nothing(node_id);
            };
            self.add_instruction(
                node_id,
                Instruction::BinaryOp {
                    lhs_dst: current,
                    op: operator,
                    rhs,
                },
            );
            current
        };

        self.add_instruction(
            node_id,
            Instruction::StoreEnv {
                key: key_slice,
                src: value,
            },
        );
        self.load_nothing(node_id)
    }

    fn generate_range(&mut self, lhs: NodeId, rhs: NodeId, node_id: NodeId) -> RegId {
        let (start, step, end) = if let AstNode::Range {
            lhs: start,
            rhs: step,
        } = self.compiler.ast_nodes[lhs.0]
        {
            (
                self.generate_node(start),
                self.generate_node(step),
                self.generate_node(rhs),
            )
        } else {
            (
                self.generate_node(lhs),
                self.load_nothing(node_id),
                self.generate_node(rhs),
            )
        };

        let inclusion = if self.node_source_string(node_id).contains("..<") {
            RangeInclusion::RightExclusive
        } else {
            RangeInclusion::Inclusive
        };

        self.load_literal(
            node_id,
            Literal::Range {
                start,
                step,
                end,
                inclusion,
            },
        )
    }

    fn generate_list(&mut self, node_id: NodeId) -> RegId {
        let items = self.compiler.get_list(node_id).items.clone();
        let out = self.load_literal(
            node_id,
            Literal::List {
                capacity: items.len(),
            },
        );

        for item in items {
            if let AstNode::Spread(expr) = self.compiler.ast_nodes[item.0] {
                let items = self.generate_node(expr);
                self.add_instruction(
                    item,
                    Instruction::ListSpread {
                        src_dst: out,
                        items,
                    },
                );
            } else {
                let reg = self.generate_node(item);
                self.add_instruction(
                    item,
                    Instruction::ListPush {
                        src_dst: out,
                        item: reg,
                    },
                );
            }
        }

        out
    }

    fn generate_table(&mut self, node_id: NodeId) -> RegId {
        let table = self.compiler.get_table(node_id).clone();
        let out = self.load_literal(
            node_id,
            Literal::List {
                capacity: table.rows.len(),
            },
        );

        let header_items = self.list_items_or_single(table.header);
        let mut header_regs = Vec::with_capacity(header_items.len());
        for header in header_items {
            header_regs.push(self.generate_node(header));
        }

        for row in table.rows {
            let row_items = self.list_items_or_single(row);
            let row_reg = self.load_literal(
                row,
                Literal::Record {
                    capacity: header_regs.len(),
                },
            );
            for (header_reg, value_node) in header_regs.iter().copied().zip(row_items) {
                let key = self.clone_register(header_reg, value_node);
                let value = self.generate_node(value_node);
                self.add_instruction(
                    value_node,
                    Instruction::RecordInsert {
                        src_dst: row_reg,
                        key,
                        val: value,
                    },
                );
            }
            self.add_instruction(
                row,
                Instruction::ListPush {
                    src_dst: out,
                    item: row_reg,
                },
            );
        }

        for header_reg in header_regs {
            self.add_instruction(node_id, Instruction::Drop { src: header_reg });
        }

        out
    }

    fn generate_record(&mut self, node_id: NodeId) -> RegId {
        let pairs = self.compiler.get_record(node_id).pairs.clone();
        let out = self.load_literal(
            node_id,
            Literal::Record {
                capacity: pairs.len(),
            },
        );

        for (key_node, value_node) in pairs {
            let key = self.generate_node(key_node);
            let value = self.generate_node(value_node);
            self.add_instruction(
                node_id,
                Instruction::RecordInsert {
                    src_dst: out,
                    key,
                    val: value,
                },
            );
        }

        out
    }

    fn generate_member_access(&mut self, node_id: NodeId) -> RegId {
        let (root, members) = self.member_access_parts(node_id);
        if members.is_empty() {
            return self.generate_node(root);
        }

        if self.is_env_variable(root) {
            return self.generate_env_member_access(&members, node_id);
        }

        let base = self.generate_node(root);
        let path = self.load_cell_path(node_id, members);
        self.add_instruction(
            node_id,
            Instruction::FollowCellPath {
                src_dst: base,
                path,
            },
        );
        base
    }

    fn generate_env_member_access(&mut self, members: &[PathMember], node_id: NodeId) -> RegId {
        let key = self.member_to_key(&members[0]);
        let key_slice = self.add_data(node_id, key.as_bytes());
        let out = self.next_register();
        self.add_instruction(
            node_id,
            Instruction::LoadEnvOpt {
                dst: out,
                key: key_slice,
            },
        );

        if members.len() > 1 {
            let path = self.load_cell_path(node_id, members[1..].to_vec());
            self.add_instruction(node_id, Instruction::FollowCellPath { src_dst: out, path });
        }

        out
    }

    fn generate_if(
        &mut self,
        condition: NodeId,
        then_block: NodeId,
        else_block: Option<NodeId>,
        node_id: NodeId,
    ) -> RegId {
        let out = self.next_register();

        let condition = self.generate_node(condition);
        self.add_instruction(node_id, Instruction::Not { src_dst: condition });
        let false_branch = self.add_branch_if(node_id, condition, PLACEHOLDER_INDEX);

        let then_reg = self.generate_node(then_block);
        self.move_register(out, then_reg, then_block);
        let end_jump = self.add_jump(node_id, PLACEHOLDER_INDEX);

        let false_index = self.here();
        self.patch_branch(false_branch, false_index);

        if let Some(else_block) = else_block {
            let else_reg = self.generate_node(else_block);
            self.move_register(out, else_reg, else_block);
        } else {
            self.add_instruction(
                node_id,
                Instruction::LoadLiteral {
                    dst: out,
                    lit: Literal::Nothing,
                },
            );
        }

        let end_index = self.here();
        self.patch_branch(end_jump, end_index);
        out
    }

    fn generate_while(&mut self, condition: NodeId, block: NodeId, node_id: NodeId) -> RegId {
        let loop_start = self.here();
        let condition = self.generate_node(condition);
        self.add_instruction(node_id, Instruction::Not { src_dst: condition });
        let end_branch = self.add_branch_if(node_id, condition, PLACEHOLDER_INDEX);

        self.loop_stack.push(LoopContext {
            continue_target: loop_start,
            break_jumps: Vec::new(),
        });
        let body = self.generate_node(block);
        self.add_instruction(block, Instruction::Drain { src: body });
        self.add_instruction(node_id, Instruction::Jump { index: loop_start });

        let end = self.here();
        self.patch_branch(end_branch, end);
        self.patch_loop_breaks(end);
        self.load_nothing(node_id)
    }

    fn generate_loop(&mut self, block: NodeId, node_id: NodeId) -> RegId {
        let loop_start = self.here();
        self.loop_stack.push(LoopContext {
            continue_target: loop_start,
            break_jumps: Vec::new(),
        });

        let body = self.generate_node(block);
        self.add_instruction(block, Instruction::Drain { src: body });
        self.add_instruction(node_id, Instruction::Jump { index: loop_start });

        let end = self.here();
        self.patch_loop_breaks(end);
        self.load_nothing(node_id)
    }

    fn generate_for(
        &mut self,
        variable: NodeId,
        range: NodeId,
        block: NodeId,
        node_id: NodeId,
    ) -> RegId {
        let stream = self.generate_node(range);
        let loop_start = self.here();
        let item = self.next_register();
        let iterate = self.add_instruction_index(
            node_id,
            Instruction::Iterate {
                dst: item,
                stream,
                end_index: PLACEHOLDER_INDEX,
            },
        );

        let var_id = self.variable_id(variable);
        self.add_instruction(variable, Instruction::StoreVariable { var_id, src: item });

        self.loop_stack.push(LoopContext {
            continue_target: loop_start,
            break_jumps: Vec::new(),
        });
        let body = self.generate_node(block);
        self.add_instruction(block, Instruction::Drain { src: body });
        self.add_instruction(node_id, Instruction::Jump { index: loop_start });

        let end = self.here();
        self.patch_branch(iterate, end);
        self.patch_loop_breaks(end);
        self.load_nothing(node_id)
    }

    fn generate_return(&mut self, value: Option<NodeId>, node_id: NodeId) -> RegId {
        let value = if let Some(value) = value {
            self.generate_node(value)
        } else {
            self.load_nothing(node_id)
        };
        self.add_instruction(node_id, Instruction::ReturnEarly { src: value });
        self.load_nothing(node_id)
    }

    fn generate_break(&mut self, node_id: NodeId) -> RegId {
        if self.loop_stack.is_empty() {
            self.error("'break' can only be used inside a loop", node_id);
        } else {
            let jump = self.add_jump(node_id, PLACEHOLDER_INDEX);
            self.loop_stack
                .last_mut()
                .expect("loop stack checked above")
                .break_jumps
                .push(jump);
        }
        self.load_nothing(node_id)
    }

    fn generate_continue(&mut self, node_id: NodeId) -> RegId {
        if let Some(loop_context) = self.loop_stack.last() {
            self.add_instruction(
                node_id,
                Instruction::Jump {
                    index: loop_context.continue_target,
                },
            );
        } else {
            self.error("'continue' can only be used inside a loop", node_id);
        }
        self.load_nothing(node_id)
    }

    fn generate_try(
        &mut self,
        try_block: NodeId,
        catch_block: Option<NodeId>,
        finally_block: Option<NodeId>,
        node_id: NodeId,
    ) -> RegId {
        let out = self.next_register();
        let error_handler = catch_block.map(|_| {
            self.add_instruction_index(
                node_id,
                Instruction::OnError {
                    index: PLACEHOLDER_INDEX,
                },
            )
        });

        let try_reg = self.generate_node(try_block);
        self.move_register(out, try_reg, try_block);

        if catch_block.is_some() {
            self.add_instruction(node_id, Instruction::PopErrorHandler);
        }

        let after_catch_jump = catch_block.map(|_| self.add_jump(node_id, PLACEHOLDER_INDEX));

        if let Some(catch_block) = catch_block {
            let catch_index = self.here();
            if let Some(error_handler) = error_handler {
                self.patch_branch(error_handler, catch_index);
            }

            let catch_reg = self.generate_catch_body(catch_block);
            self.move_register(out, catch_reg, catch_block);
        }

        let after_catch = self.here();
        if let Some(after_catch_jump) = after_catch_jump {
            self.patch_branch(after_catch_jump, after_catch);
        }

        if let Some(finally_block) = finally_block {
            let finally_reg = self.generate_node(finally_block);
            self.add_instruction(finally_block, Instruction::Drain { src: finally_reg });
        }

        out
    }

    fn generate_catch_body(&mut self, node_id: NodeId) -> RegId {
        if let AstNode::Closure { block, .. } = self.compiler.ast_nodes[node_id.0] {
            self.generate_node(block)
        } else {
            self.generate_node(node_id)
        }
    }

    fn generate_match(&mut self, node_id: NodeId) -> RegId {
        let match_node = self.compiler.get_match(node_id).clone();
        let out = self.next_register();
        let target = self.generate_node(match_node.target);
        let mut end_jumps = Vec::new();

        for (pattern, body) in match_node.match_arms {
            let mut success_jumps = Vec::new();
            let next_arm_jump = if self.is_wildcard_pattern(pattern) {
                None
            } else {
                for pattern in self.pattern_alternatives(pattern) {
                    let cond = self.generate_pattern_condition(target, pattern);
                    success_jumps.push(self.add_branch_if(pattern, cond, PLACEHOLDER_INDEX));
                }
                Some(self.add_jump(pattern, PLACEHOLDER_INDEX))
            };

            let body_start = self.here();
            for jump in success_jumps {
                self.patch_branch(jump, body_start);
            }

            let body_reg = self.generate_node(body);
            self.move_register(out, body_reg, body);
            end_jumps.push(self.add_jump(body, PLACEHOLDER_INDEX));

            if let Some(next_arm_jump) = next_arm_jump {
                let next_arm = self.here();
                self.patch_branch(next_arm_jump, next_arm);
            }
        }

        self.add_instruction(
            node_id,
            Instruction::LoadLiteral {
                dst: out,
                lit: Literal::Nothing,
            },
        );

        let end = self.here();
        for jump in end_jumps {
            self.patch_branch(jump, end);
        }

        out
    }

    fn generate_pattern_condition(&mut self, target: RegId, pattern: NodeId) -> RegId {
        let lhs = self.clone_register(target, pattern);
        let rhs = self.generate_node(pattern);
        self.add_instruction(
            pattern,
            Instruction::BinaryOp {
                lhs_dst: lhs,
                op: Operator::Comparison(Comparison::Equal),
                rhs,
            },
        );
        lhs
    }

    fn generate_redirection(
        &mut self,
        source: NodeId,
        op: NodeId,
        target: NodeId,
        node_id: NodeId,
        input: Option<RegId>,
    ) -> RegId {
        let (redirect_out, redirect_err, append) = self.redirection_info(op);
        let path = self.generate_node(target);
        let file_num = self.next_file_num(node_id);
        self.add_instruction(
            target,
            Instruction::OpenFile {
                file_num,
                path,
                append,
            },
        );

        let file = RedirectMode::File { file_num };
        if redirect_out {
            self.add_instruction(node_id, Instruction::RedirectOut { mode: file });
        }
        if redirect_err {
            self.add_instruction(node_id, Instruction::RedirectErr { mode: file });
        }

        let output = self.generate_node_with_input(source, input);
        if redirect_out || redirect_err {
            let copy = self.clone_register(output, node_id);
            self.add_instruction(
                node_id,
                Instruction::WriteFile {
                    file_num,
                    src: copy,
                },
            );
        }
        self.add_instruction(node_id, Instruction::CloseFile { file_num });
        output
    }

    fn generate_env_assignment(&mut self, name: NodeId, value: NodeId, node_id: NodeId) -> RegId {
        let key = self.node_source_string(name);
        let key = self.add_data(name, key.as_bytes());
        let value = self.generate_node(value);
        self.add_instruction(node_id, Instruction::StoreEnv { key, src: value });
        self.load_nothing(node_id)
    }

    fn generate_closure(&mut self, block: NodeId, node_id: NodeId) -> RegId {
        if let AstNode::Block(block_id) = self.compiler.ast_nodes[block.0] {
            let block_id = self.map_block_id(block_id, node_id);
            self.load_literal(node_id, Literal::Closure(block_id))
        } else {
            self.error("closure body is not a block", node_id);
            self.load_nothing(node_id)
        }
    }

    fn load_variable(&mut self, node_id: NodeId) -> RegId {
        let dst = self.next_register();
        let var_id = self.variable_id(node_id);
        self.add_instruction(
            node_id,
            Instruction::LoadVariable {
                dst,
                var_id,
                preserve_origin: false,
            },
        );
        dst
    }

    fn load_string(&mut self, node_id: NodeId) -> RegId {
        let value = self.string_value(node_id);
        let data = self.add_data(node_id, value.as_bytes());
        let literal = if is_raw_string_source(&self.node_source_string(node_id)) {
            Literal::RawString(data)
        } else {
            Literal::String(data)
        };
        self.load_literal(node_id, literal)
    }

    fn load_source_string(&mut self, node_id: NodeId) -> RegId {
        let value = self.node_source_string(node_id);
        let data = self.add_data(node_id, value.as_bytes());
        self.load_literal(node_id, Literal::String(data))
    }

    fn load_external_command_name(&mut self, node_id: NodeId) -> RegId {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Name => {
                let value = self.node_source_string(node_id);
                let value = value.strip_prefix('^').unwrap_or(&value);
                let value = trim_decl_name(value);
                let data = self.add_data(node_id, value.as_bytes());
                self.load_literal(node_id, Literal::String(data))
            }
            AstNode::String => self.load_string(node_id),
            _ => self.generate_node(node_id),
        }
    }

    fn load_cell_path_literal(&mut self, node_id: NodeId) -> RegId {
        let members = self.cell_path_members_from_source(node_id);
        self.load_cell_path(node_id, members)
    }

    fn load_cell_path(&mut self, node_id: NodeId, members: Vec<PathMember>) -> RegId {
        self.load_literal(node_id, Literal::CellPath(Box::new(CellPath { members })))
    }

    fn load_literal(&mut self, node_id: NodeId, lit: Literal) -> RegId {
        let dst = self.next_register();
        self.add_instruction(node_id, Instruction::LoadLiteral { dst, lit });
        dst
    }

    fn load_nothing(&mut self, node_id: NodeId) -> RegId {
        self.load_literal(node_id, Literal::Nothing)
    }

    fn add_instruction(&mut self, node_id: NodeId, instruction: Instruction) {
        self.add_instruction_index(node_id, instruction);
    }

    fn add_instruction_index(&mut self, node_id: NodeId, instruction: Instruction) -> usize {
        let instruction_index = self.block.instructions.len();
        let span = self.compiler.get_span(node_id);
        self.block.spans.push(Span {
            start: span.start,
            end: span.end,
        });
        self.block.ast.push(None);
        self.block.comments.push(Box::<str>::from(""));
        self.block.instructions.push(instruction);
        instruction_index
    }

    fn add_branch_if(&mut self, node_id: NodeId, cond: RegId, index: usize) -> usize {
        self.add_instruction_index(node_id, Instruction::BranchIf { cond, index })
    }

    fn add_jump(&mut self, node_id: NodeId, index: usize) -> usize {
        self.add_instruction_index(node_id, Instruction::Jump { index })
    }

    fn patch_branch(&mut self, instruction_index: usize, target_index: usize) {
        if let Some(instruction) = self.block.instructions.get_mut(instruction_index) {
            if instruction.set_branch_target(target_index).is_err() {
                self.error(
                    "internal error: attempted to patch a non-branch instruction",
                    NodeId(0),
                );
            }
        }
    }

    fn patch_loop_breaks(&mut self, target_index: usize) {
        let Some(loop_context) = self.loop_stack.pop() else {
            return;
        };

        for jump in loop_context.break_jumps {
            self.patch_branch(jump, target_index);
        }
    }

    fn move_register(&mut self, dst: RegId, src: RegId, node_id: NodeId) {
        if dst != src {
            self.add_instruction(node_id, Instruction::Move { dst, src });
        }
    }

    fn clone_register(&mut self, src: RegId, node_id: NodeId) -> RegId {
        let dst = self.next_register();
        self.add_instruction(node_id, Instruction::Clone { dst, src });
        dst
    }

    fn here(&self) -> usize {
        self.block.instructions.len()
    }

    fn next_file_num(&mut self, node_id: NodeId) -> u32 {
        let file_num = self.block.file_count;
        self.block.file_count = self.block.file_count.checked_add(1).unwrap_or_else(|| {
            self.error("IR file number overflow", node_id);
            file_num
        });
        file_num
    }

    fn add_data(&mut self, node_id: NodeId, data: &[u8]) -> DataSlice {
        if data.is_empty() {
            return DataSlice::empty();
        }

        let start = self.data.len();
        if start + data.len() >= u32::MAX as usize {
            self.error("IR data section overflow", node_id);
            return DataSlice::empty();
        }

        self.data.extend_from_slice(data);
        DataSlice {
            start: start as u32,
            len: data.len() as u32,
        }
    }

    fn variable_id(&mut self, node_id: NodeId) -> NuVarId {
        if let Some(var_id) = self.compiler.var_resolution.get(&node_id) {
            self.map_var_id(*var_id, node_id)
        } else {
            match self.node_source_string(node_id).as_str() {
                "$in" => IN_VARIABLE_ID,
                "$env" => ENV_VARIABLE_ID,
                "$nu" => NU_VARIABLE_ID,
                _ => {
                    self.error("unresolved variable in IR generation", node_id);
                    NuVarId::new(node_id.0)
                }
            }
        }
    }

    fn map_var_id(&mut self, var_id: crate::resolver::VarId, node_id: NodeId) -> NuVarId {
        if let Some(var_map) = self.var_map {
            if let Some(Some(mapped)) = var_map.get(var_id.0) {
                return *mapped;
            }
            self.error("missing runtime variable ID mapping", node_id);
        }
        NuVarId::new(var_id.0)
    }

    fn map_decl_id(&mut self, decl_id: crate::resolver::DeclId, node_id: NodeId) -> NuDeclId {
        if let Some(decl_map) = self.decl_map {
            if let Some(Some(mapped)) = decl_map.get(decl_id.0) {
                return *mapped;
            }
            self.error("missing runtime declaration ID mapping", node_id);
        }
        NuDeclId::new(decl_id.0)
    }

    fn mapped_run_external_decl(&self, decl_id: crate::resolver::DeclId) -> Option<NuDeclId> {
        let run_external_decl = self.run_external_decl?;
        let mapped = self
            .decl_map
            .and_then(|decl_map| decl_map.get(decl_id.0))
            .and_then(|decl_id| *decl_id)?;

        (mapped.get() == run_external_decl.get()).then_some(run_external_decl)
    }

    fn map_block_id(&mut self, block_id: crate::parser::BlockId, node_id: NodeId) -> NuBlockId {
        if let Some(block_map) = self.block_map {
            if let Some(Some(mapped)) = block_map.get(block_id.0) {
                return *mapped;
            }
            self.error("missing runtime block ID mapping", node_id);
        }
        NuBlockId::new(block_id.0)
    }

    fn call_name_part_count(
        &mut self,
        node_id: NodeId,
        decl_id: crate::resolver::DeclId,
        parts: &[NodeId],
    ) -> usize {
        let max_name_parts = parts
            .iter()
            .take_while(|part| matches!(self.compiler.ast_nodes[part.0], AstNode::Name))
            .count();

        let Some(first) = parts.first() else {
            return 0;
        };
        let first_start = self.compiler.get_span(*first).start;
        let decl_name = self.compiler.decls[decl_id.0].name();

        for last_name_part in (0..max_name_parts).rev() {
            let last_end = self.compiler.get_span(parts[last_name_part]).end;
            let candidate = String::from_utf8_lossy(
                self.compiler
                    .get_span_contents_manual(first_start, last_end),
            );
            if trim_decl_name(&candidate) == decl_name {
                return last_name_part + 1;
            }
        }

        self.error("resolved call name did not match call parts", node_id);
        1
    }

    fn node_to_operator(&mut self, node_id: NodeId) -> Option<OperatorPlan> {
        let (operator, negate) = match self.compiler.get_node(node_id) {
            AstNode::Pow => (Operator::Math(Math::Pow), false),
            AstNode::Multiply => (Operator::Math(Math::Multiply), false),
            AstNode::Divide => (Operator::Math(Math::Divide), false),
            AstNode::FloorDiv => (Operator::Math(Math::FloorDivide), false),
            AstNode::Modulo => (Operator::Math(Math::Modulo), false),
            AstNode::Plus => (Operator::Math(Math::Add), false),
            AstNode::Minus => (Operator::Math(Math::Subtract), false),
            AstNode::Append => (Operator::Math(Math::Concatenate), false),
            AstNode::Equal => (Operator::Comparison(Comparison::Equal), false),
            AstNode::NotEqual => (Operator::Comparison(Comparison::NotEqual), false),
            AstNode::LessThan => (Operator::Comparison(Comparison::LessThan), false),
            AstNode::GreaterThan => (Operator::Comparison(Comparison::GreaterThan), false),
            AstNode::LessThanOrEqual => (Operator::Comparison(Comparison::LessThanOrEqual), false),
            AstNode::GreaterThanOrEqual => {
                (Operator::Comparison(Comparison::GreaterThanOrEqual), false)
            }
            AstNode::RegexMatch | AstNode::Like => {
                (Operator::Comparison(Comparison::RegexMatch), false)
            }
            AstNode::NotRegexMatch | AstNode::NotLike => {
                (Operator::Comparison(Comparison::NotRegexMatch), false)
            }
            AstNode::In | AstNode::Has => (Operator::Comparison(Comparison::In), false),
            AstNode::NotIn | AstNode::NotHas => (Operator::Comparison(Comparison::NotIn), false),
            AstNode::StartsWith => (Operator::Comparison(Comparison::StartsWith), false),
            AstNode::NotStartsWith => (Operator::Comparison(Comparison::StartsWith), true),
            AstNode::EndsWith => (Operator::Comparison(Comparison::EndsWith), false),
            AstNode::NotEndsWith => (Operator::Comparison(Comparison::EndsWith), true),
            AstNode::BitOr => (Operator::Bits(Bits::BitOr), false),
            AstNode::BitXor => (Operator::Bits(Bits::BitXor), false),
            AstNode::BitAnd => (Operator::Bits(Bits::BitAnd), false),
            AstNode::BitShiftLeft => (Operator::Bits(Bits::ShiftLeft), false),
            AstNode::BitShiftRight => (Operator::Bits(Bits::ShiftRight), false),
            AstNode::And => (Operator::Boolean(Boolean::And), false),
            AstNode::Xor => (Operator::Boolean(Boolean::Xor), false),
            AstNode::Or => (Operator::Boolean(Boolean::Or), false),
            node => {
                self.error(format!("unrecognized operator {:?}", node), node_id);
                return None;
            }
        };

        Some(OperatorPlan { operator, negate })
    }

    fn assignment_operator_to_binary(&mut self, node_id: NodeId) -> Option<Operator> {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::AddAssignment => Some(Operator::Math(Math::Add)),
            AstNode::SubtractAssignment => Some(Operator::Math(Math::Subtract)),
            AstNode::MultiplyAssignment => Some(Operator::Math(Math::Multiply)),
            AstNode::DivideAssignment => Some(Operator::Math(Math::Divide)),
            AstNode::AppendAssignment => Some(Operator::Math(Math::Concatenate)),
            AstNode::Assignment => None,
            node => {
                self.error(
                    format!("unrecognized assignment operator {:?}", node),
                    node_id,
                );
                None
            }
        }
    }

    fn is_assignment_operator(&self, node_id: NodeId) -> bool {
        matches!(
            self.compiler.ast_nodes[node_id.0],
            AstNode::Assignment
                | AstNode::AddAssignment
                | AstNode::SubtractAssignment
                | AstNode::MultiplyAssignment
                | AstNode::DivideAssignment
                | AstNode::AppendAssignment
        )
    }

    fn list_items_or_single(&self, node_id: NodeId) -> Vec<NodeId> {
        if let AstNode::List(_) = self.compiler.ast_nodes[node_id.0] {
            self.compiler.get_list(node_id).items.clone()
        } else {
            vec![node_id]
        }
    }

    fn member_access_parts(&self, node_id: NodeId) -> (NodeId, Vec<PathMember>) {
        let mut fields = Vec::new();
        let mut current = node_id;

        while let AstNode::MemberAccess { target, field } = self.compiler.ast_nodes[current.0] {
            fields.push(self.path_member_for_field(field, current));
            current = target;
        }

        fields.reverse();
        (current, fields)
    }

    fn path_member_for_field(&self, field: NodeId, parent: NodeId) -> PathMember {
        let span = self.nu_span(field);
        let optional = self.compiler.get_span(parent).end > self.compiler.get_span(field).end;

        match self.compiler.ast_nodes[field.0] {
            AstNode::Int => self
                .node_source_string(field)
                .replace('_', "")
                .parse::<usize>()
                .map_or_else(
                    |_| {
                        PathMember::string(
                            self.node_source_string(field),
                            optional,
                            Casing::Sensitive,
                            span,
                        )
                    },
                    |value| PathMember::int(value, optional, span),
                ),
            AstNode::String => {
                PathMember::string(self.string_value(field), optional, Casing::Sensitive, span)
            }
            _ => PathMember::string(
                self.node_source_string(field),
                optional,
                Casing::Sensitive,
                span,
            ),
        }
    }

    fn cell_path_members_from_source(&self, node_id: NodeId) -> Vec<PathMember> {
        let source = self.compiler.get_span_contents(node_id);
        let span_start = self.compiler.get_span(node_id).start;
        let mut pos = usize::from(source.first() == Some(&b'$'));
        let mut members = Vec::new();

        while pos < source.len() {
            if source[pos] != b'.' {
                break;
            }
            pos += 1;

            if pos >= source.len() {
                break;
            }

            let member_start = pos;
            let mut quoted = false;
            let member_text;
            let member_end;

            if matches!(source[pos], b'\'' | b'"' | b'`') {
                quoted = true;
                let quote = source[pos];
                pos += 1;
                let value_start = pos;
                while pos < source.len() && source[pos] != quote {
                    pos += 1;
                }
                member_text = String::from_utf8_lossy(&source[value_start..pos]).into_owned();
                if pos < source.len() {
                    pos += 1;
                }
                member_end = pos;
            } else {
                while pos < source.len() && !matches!(source[pos], b'.' | b'?') {
                    pos += 1;
                }
                member_end = pos;
                member_text =
                    String::from_utf8_lossy(&source[member_start..member_end]).into_owned();
            }

            let optional = if pos < source.len() && source[pos] == b'?' {
                pos += 1;
                true
            } else {
                false
            };

            let span = Span {
                start: span_start + member_start,
                end: span_start + member_end,
            };
            members.push(self.path_member_from_text(member_text, quoted, optional, span));
        }

        members
    }

    fn path_member_from_text(
        &self,
        text: String,
        quoted: bool,
        optional: bool,
        span: Span,
    ) -> PathMember {
        if !quoted {
            if let Ok(value) = text.parse::<usize>() {
                return PathMember::int(value, optional, span);
            }
        }

        PathMember::string(text, optional, Casing::Sensitive, span)
    }

    fn pattern_alternatives(&self, pattern: NodeId) -> Vec<NodeId> {
        if let AstNode::List(_) = self.compiler.ast_nodes[pattern.0] {
            if self.node_source_string(pattern).contains('|') {
                return self.compiler.get_list(pattern).items.clone();
            }
        }
        vec![pattern]
    }

    fn is_wildcard_pattern(&self, pattern: NodeId) -> bool {
        self.node_source_string(pattern).trim() == "_"
    }

    fn is_env_variable(&self, node_id: NodeId) -> bool {
        matches!(self.compiler.ast_nodes[node_id.0], AstNode::Variable)
            && self.node_source_string(node_id) == "$env"
    }

    fn member_to_key(&self, member: &PathMember) -> String {
        match member {
            PathMember::String { val, .. } => val.clone(),
            PathMember::Int { val, .. } => val.to_string(),
        }
    }

    fn redirection_info(&self, op: NodeId) -> (bool, bool, bool) {
        let op = self.node_source_string(op);
        let append = op.ends_with(">>");
        let stderr = op.starts_with("e>") || op.starts_with("o+e");
        let stdout = !op.starts_with("e>") || op.starts_with("o+e");
        (stdout, stderr, append)
    }

    fn flag_name_bytes(&self, node_id: NodeId) -> Vec<u8> {
        let source = self.compiler.get_span_contents(node_id);
        if source.starts_with(b"--") {
            source[2..].to_vec()
        } else if source.starts_with(b"-") {
            source[1..].to_vec()
        } else {
            source.to_vec()
        }
    }

    fn short_group_names(&self, node_id: NodeId) -> Vec<Vec<u8>> {
        self.flag_name_bytes(node_id)
            .into_iter()
            .map(|byte| vec![byte])
            .collect()
    }

    fn string_value(&self, node_id: NodeId) -> String {
        let source = self.node_source_string(node_id);

        if let Some(raw) = raw_string_value(&source) {
            return raw.to_string();
        }

        if let Some(stripped) = source
            .strip_prefix("$\"")
            .and_then(|value| value.strip_suffix('"'))
        {
            return stripped.to_string();
        }

        if let Some(stripped) = source
            .strip_prefix("$'")
            .and_then(|value| value.strip_suffix('\''))
        {
            return stripped.to_string();
        }

        if let Some(stripped) = source
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            return unescape_double_quoted(stripped);
        }

        if let Some(stripped) = source
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
        {
            return stripped.to_string();
        }

        if let Some(stripped) = source
            .strip_prefix('`')
            .and_then(|value| value.strip_suffix('`'))
        {
            return stripped.to_string();
        }

        source
    }

    fn node_source_string(&self, node_id: NodeId) -> String {
        String::from_utf8_lossy(self.compiler.get_span_contents(node_id)).into_owned()
    }

    fn nu_span(&self, node_id: NodeId) -> Span {
        let span = self.compiler.get_span(node_id);
        Span {
            start: span.start,
            end: span.end,
        }
    }

    fn error(&mut self, message: impl Into<String>, node_id: NodeId) {
        self.errors.push(SourceError {
            message: message.into(),
            node_id,
            severity: Severity::Error,
        });
    }
}

fn parse_int_literal(text: &str) -> Result<i64, String> {
    let text = text.replace('_', "");
    if let Some(hex) = text.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).map_err(|err| format!("invalid hex integer literal: {err}"))
    } else if let Some(octal) = text.strip_prefix("0o") {
        i64::from_str_radix(octal, 8).map_err(|err| format!("invalid octal integer literal: {err}"))
    } else if let Some(binary) = text.strip_prefix("0b") {
        i64::from_str_radix(binary, 2)
            .map_err(|err| format!("invalid binary integer literal: {err}"))
    } else {
        text.parse::<i64>()
            .map_err(|err| format!("invalid integer literal: {err}"))
    }
}

fn raw_string_value(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    if bytes.first() != Some(&b'r') {
        return None;
    }

    let mut pos = 1;
    while pos < bytes.len() && bytes[pos] == b'#' {
        pos += 1;
    }

    if bytes.get(pos) != Some(&b'\'') {
        return None;
    }

    let hashes = pos - 1;
    let value_start = pos + 1;
    if source.len() < value_start + hashes + 1 {
        return None;
    }

    let value_end = source.len() - hashes - 1;
    Some(&source[value_start..value_end])
}

fn is_raw_string_source(source: &str) -> bool {
    raw_string_value(source).is_some()
}

fn unescape_double_quoted(source: &str) -> String {
    let mut output = String::new();
    let mut chars = source.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('"') => output.push('"'),
            Some('\\') => output.push('\\'),
            Some('b') => output.push('\u{0008}'),
            Some('n') => output.push('\n'),
            Some('f') => output.push('\u{000c}'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }

    output
}

fn trim_decl_name(name: &str) -> &str {
    if (name.starts_with('\'') && name.ends_with('\''))
        || (name.starts_with('"') && name.ends_with('"'))
        || (name.starts_with('`') && name.ends_with('`'))
    {
        &name[1..name.len() - 1]
    } else {
        name
    }
}
