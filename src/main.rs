use std::{
    collections::HashMap,
    io::{self, BufRead, IsTerminal},
    process::exit,
    sync::Arc,
};

use new_nu_parser::compiler::Compiler;
use new_nu_parser::ir_generator::IrGenerator;
use new_nu_parser::lexer::lex;
use new_nu_parser::parser::{AstNode, BlockId, NodeId, Parser};
use new_nu_parser::protocol::Declaration;
use new_nu_parser::resolver::{DeclId, Resolver, VarId, Variable};
use new_nu_parser::typechecker::Typechecker;
use nu_protocol::{
    ast::Block as NuBlock,
    engine::{Call as NuCall, Command as NuCommand, EngineState, Stack, StateWorkingSet},
    Category, DeclId as NuDeclId, Flag, PipelineData, PositionalArg, ShellError, Signature,
    Span as NuSpan, SyntaxShape, Type as NuType, Value, VarId as NuVarId,
};
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

const RUNTIME_DECL_NODE_BASE: usize = usize::MAX / 2;

fn main() {
    let options = Options::parse();
    let mut runtime = NuRuntime::new().unwrap_or_else(|err| {
        eprintln!("can't initialize Nushell eval engine: {err}");
        exit(1);
    });

    let mut compiler = Compiler::new();
    for fname in &options.files {
        let contents = std::fs::read(fname).unwrap_or_else(|_| {
            eprintln!("can't find {fname}");
            exit(1);
        });

        compiler = run_source(
            compiler,
            &mut runtime,
            fname,
            &contents,
            options.do_print,
            options.do_eval,
        )
        .unwrap_or_else(|err| {
            eprintln!("{err}");
            exit(1);
        });
    }

    if options.repl {
        run_repl(&mut runtime, options.do_print, options.do_eval).unwrap_or_else(|err| {
            eprintln!("{err}");
            exit(1);
        });
    }
}

struct Options {
    do_print: bool,
    do_eval: bool,
    repl: bool,
    files: Vec<String>,
}

impl Options {
    fn parse() -> Self {
        let mut do_eval = true;
        let mut repl = false;
        let mut files = Vec::new();
        let mut print_override = None;

        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--no-eval" => do_eval = false,
                "--no-print" => print_override = Some(false),
                "--print" | "--debug" => print_override = Some(true),
                "--repl" => repl = true,
                "-h" | "--help" => {
                    print_usage();
                    exit(0);
                }
                _ => files.push(arg),
            }
        }

        let repl = repl || files.is_empty();
        let do_print = print_override.unwrap_or(!repl);

        Self {
            do_print,
            do_eval,
            repl,
            files,
        }
    }
}

fn print_usage() {
    println!("usage: new-nu-parser [--repl] [--no-eval] [--no-print|--print] [file ...]");
}

fn run_repl(runtime: &mut NuRuntime, do_print: bool, do_eval: bool) -> Result<(), String> {
    if !io::stdin().is_terminal() {
        return run_stdin_repl(runtime, do_print, do_eval);
    }

    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("new-nu-parser".to_string()),
        DefaultPromptSegment::Empty,
    );
    let mut entry_num = 1usize;

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(line)) | Ok(Signal::HostCommand(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed, ":q" | ":quit") {
                    println!();
                    break;
                }

                runtime.clear_local_maps();
                let fname = format!("<repl:{entry_num}>");
                if let Err(err) = run_source(
                    Compiler::new(),
                    runtime,
                    &fname,
                    line.as_bytes(),
                    do_print,
                    do_eval,
                ) {
                    eprintln!("{err}");
                }
                entry_num += 1;
            }
            Ok(Signal::CtrlC) => {}
            Ok(Signal::CtrlD) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(err) => return Err(format!("REPL error. Error: {err}")),
        }
    }

    Ok(())
}

fn run_stdin_repl(runtime: &mut NuRuntime, do_print: bool, do_eval: bool) -> Result<(), String> {
    let stdin = io::stdin();
    for (idx, line) in stdin.lock().lines().enumerate() {
        let line = line.map_err(|err| format!("REPL input error. Error: {err}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, ":q" | ":quit") {
            break;
        }

        runtime.clear_local_maps();
        let fname = format!("<stdin-repl:{}>", idx + 1);
        if let Err(err) = run_source(
            Compiler::new(),
            runtime,
            &fname,
            line.as_bytes(),
            do_print,
            do_eval,
        ) {
            eprintln!("{err}");
        }
    }

    Ok(())
}

fn run_source(
    mut compiler: Compiler,
    runtime: &mut NuRuntime,
    fname: &str,
    contents: &[u8],
    do_print: bool,
    do_eval: bool,
) -> Result<Compiler, String> {
    let span_offset = compiler.span_offset();
    compiler.add_file(fname, contents);
    runtime.add_file(fname, contents);

    let (tokens, err) = lex(contents, span_offset);
    if let Err(e) = err {
        if do_print {
            tokens.print(&compiler.source);
        }
        return Err(format!(
            "Lexing error. Error: {:?}, '{}'",
            e,
            String::from_utf8_lossy(compiler.get_span_contents_manual(e.span.start, e.span.end))
        ));
    }

    if do_print {
        tokens.print(&compiler.source);
    }

    let parser = Parser::new(compiler, tokens);
    let mut compiler = parser.parse();

    if do_print {
        compiler.print();
    }

    if !compiler.errors.is_empty() {
        return Err(compiler.display_state());
    }

    let mut resolver = Resolver::new(&compiler);
    resolver.resolve();

    if do_print {
        resolver.print();
    }

    compiler.merge_name_bindings(resolver.to_name_bindings());
    runtime.bind_nushell_vars(&mut compiler);
    runtime.bind_nushell_decls(&mut compiler);

    if !compiler.errors.is_empty() {
        return Err(compiler.display_state());
    }

    let mut typechecker = Typechecker::new(&compiler);
    typechecker.typecheck();

    if do_print {
        typechecker.print();
    }

    compiler.merge_types(typechecker.to_types());

    if !compiler.errors.is_empty() {
        return Err(compiler.display_state());
    }

    let mut ir_generator = IrGenerator::new(&compiler);
    ir_generator.generate();
    if do_print {
        ir_generator.print();
    }

    if !ir_generator.errors().is_empty() {
        return Err(ir_generator.display_state());
    }

    if do_eval {
        runtime
            .evaluate(&compiler)
            .map_err(|err| format!("Evaluation error. Error: {err}"))?;
    }

    Ok(compiler)
}

struct NuRuntime {
    engine_state: EngineState,
    stack: Stack,
    var_map: Vec<Option<NuVarId>>,
    decl_map: Vec<Option<NuDeclId>>,
    block_map: Vec<Option<nu_protocol::BlockId>>,
    engine_decl_to_local: HashMap<usize, DeclId>,
}

#[derive(Clone, Copy, Default)]
struct BlockInfo {
    node_id: Option<NodeId>,
    params: Option<NodeId>,
    name: Option<NodeId>,
    redirect_env: bool,
}

impl NuRuntime {
    fn new() -> Result<Self, String> {
        let engine_state = nu_cmd_lang::create_default_context();
        let engine_state = nu_command::add_shell_command_context(engine_state);
        let mut stack = Stack::new();

        for (key, value) in std::env::vars() {
            stack.add_env_var(key, Value::string(value, NuSpan::unknown()));
        }

        let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
        stack.set_cwd(cwd).map_err(format_shell_error)?;

        Ok(Self {
            engine_state,
            stack,
            var_map: Vec::new(),
            decl_map: Vec::new(),
            block_map: Vec::new(),
            engine_decl_to_local: HashMap::new(),
        })
    }

    fn add_file(&mut self, fname: &str, contents: &[u8]) {
        self.engine_state
            .add_file(Arc::<str>::from(fname), Arc::<[u8]>::from(contents));
    }

    fn clear_local_maps(&mut self) {
        self.var_map.clear();
        self.decl_map.clear();
        self.block_map.clear();
        self.engine_decl_to_local.clear();
    }

    fn bind_nushell_vars(&mut self, compiler: &mut Compiler) {
        let mut bindings = Vec::new();

        {
            let working_set = StateWorkingSet::new(&self.engine_state);

            for node_idx in 0..compiler.ast_nodes.len() {
                let node_id = NodeId(node_idx);
                if !matches!(compiler.ast_nodes[node_idx], AstNode::Variable)
                    || compiler.var_resolution.contains_key(&node_id)
                {
                    continue;
                }

                let name = compiler.get_span_contents(node_id);
                if is_reserved_runtime_variable(name) {
                    continue;
                }

                if let Some(nu_var_id) = working_set.find_variable(name) {
                    let is_mutable = self.engine_state.get_var(nu_var_id).mutable;
                    bindings.push((node_id, nu_var_id, is_mutable));
                }
            }
        }

        if bindings.is_empty() {
            return;
        }

        for (node_id, nu_var_id, is_mutable) in &bindings {
            let local_var_id = VarId(compiler.variables.len());
            compiler.variables.push(Variable {
                is_mutable: *is_mutable,
            });
            compiler.var_resolution.insert(*node_id, local_var_id);
            self.var_map.resize(compiler.variables.len(), None);
            self.var_map[local_var_id.0] = Some(*nu_var_id);
        }

        compiler.errors.retain(|err| {
            !bindings.iter().any(|(node_id, _, _)| {
                err.node_id == *node_id && err.message.starts_with("variable `")
            })
        });
    }

    fn bind_nushell_decls(&mut self, compiler: &mut Compiler) {
        self.decl_map.resize(compiler.decls.len(), None);

        for node_idx in 0..compiler.ast_nodes.len() {
            let node_id = NodeId(node_idx);
            if !matches!(compiler.ast_nodes[node_idx], AstNode::Call(_))
                || compiler.decl_resolution.contains_key(&node_id)
            {
                continue;
            }

            let parts = compiler.get_call(node_id).parts.clone();
            let Some((name, nu_decl_id)) =
                find_engine_decl_for_call(&self.engine_state, compiler, &parts)
            else {
                continue;
            };

            let local_decl_id = self.local_decl_for_engine_decl(compiler, name, nu_decl_id);
            compiler.decl_resolution.insert(node_id, local_decl_id);
        }
    }

    fn local_decl_for_engine_decl(
        &mut self,
        compiler: &mut Compiler,
        name: String,
        nu_decl_id: NuDeclId,
    ) -> DeclId {
        if let Some(local_decl_id) = self.engine_decl_to_local.get(&nu_decl_id.get()).copied() {
            return local_decl_id;
        }

        let local_decl_id = DeclId(compiler.decls.len());
        let decl_node = runtime_decl_node(nu_decl_id);
        compiler.decls.push(Box::new(Declaration::new(name)));
        compiler.decl_nodes.push(decl_node);
        compiler.decl_resolution.insert(decl_node, local_decl_id);

        self.decl_map.resize(compiler.decls.len(), None);
        self.decl_map[local_decl_id.0] = Some(nu_decl_id);
        self.engine_decl_to_local
            .insert(nu_decl_id.get(), local_decl_id);

        local_decl_id
    }

    fn evaluate(&mut self, compiler: &Compiler) -> Result<(), String> {
        let root_block_id = self.prepare(compiler)?;
        let block = self.engine_state.get_block(root_block_id).clone();
        let eval_block = nu_engine::get_eval_block_with_early_return(&self.engine_state);
        let output = eval_block(
            &self.engine_state,
            &mut self.stack,
            &block,
            PipelineData::empty(),
        )
        .map_err(format_shell_error)?;

        output
            .print_table(&self.engine_state, &mut self.stack, false, false)
            .map_err(format_shell_error)
    }

    fn prepare(&mut self, compiler: &Compiler) -> Result<nu_protocol::BlockId, String> {
        let root_node = NodeId(
            compiler
                .ast_nodes
                .len()
                .checked_sub(1)
                .ok_or_else(|| "no parsed block to evaluate".to_string())?,
        );
        let root_block = local_block_id(compiler, root_node)
            .ok_or_else(|| "top-level parser output is not a block".to_string())?;
        let block_infos = collect_block_infos(compiler);

        self.var_map.resize(compiler.variables.len(), None);
        self.decl_map.resize(compiler.decls.len(), None);
        self.block_map.resize(compiler.blocks.len(), None);

        let mut new_blocks = Vec::new();
        let mut working_set = StateWorkingSet::new(&self.engine_state);

        for var_idx in 0..compiler.variables.len() {
            if self.var_map[var_idx].is_some() {
                continue;
            }

            let (name, span) = variable_name_and_span(compiler, VarId(var_idx));
            let nu_var_id = working_set.add_variable(
                name.into_bytes(),
                span,
                NuType::Any,
                compiler.variables[var_idx].is_mutable,
            );
            self.var_map[var_idx] = Some(nu_var_id);
        }

        for block_idx in 0..compiler.blocks.len() {
            if self.block_map[block_idx].is_some() {
                continue;
            }

            let block_id = working_set.add_block(Arc::new(NuBlock::new()));
            self.block_map[block_idx] = Some(block_id);
            new_blocks.push(block_idx);
        }

        for decl_idx in 0..compiler.decls.len() {
            if self.decl_map[decl_idx].is_some() {
                continue;
            }

            let decl_node = compiler.decl_nodes[decl_idx];
            let decl = self.make_custom_decl(compiler, decl_node, &block_infos)?;
            let decl_id = working_set.add_decl(decl);
            self.decl_map[decl_idx] = Some(decl_id);
        }

        for block_idx in new_blocks {
            let block_id =
                self.block_map[block_idx].ok_or_else(|| "missing runtime block id".to_string())?;
            let node_id = block_infos
                .get(block_idx)
                .and_then(|info| info.node_id)
                .ok_or_else(|| format!("missing AST node for block {block_idx}"))?;
            let nu_block = self.make_block(compiler, node_id, block_infos[block_idx])?;
            *working_set.get_block_mut(block_id) = nu_block;
        }

        self.engine_state
            .merge_delta(working_set.render())
            .map_err(format_shell_error)?;

        self.block_map[root_block.0].ok_or_else(|| "missing root block".to_string())
    }

    fn make_custom_decl(
        &self,
        compiler: &Compiler,
        decl_node: NodeId,
        block_infos: &[BlockInfo],
    ) -> Result<Box<dyn NuCommand>, String> {
        if decl_node.0 >= compiler.ast_nodes.len() {
            let name = compiler
                .decl_resolution
                .get(&decl_node)
                .and_then(|decl_id| compiler.decls.get(decl_id.0))
                .map(|decl| decl.name().to_string())
                .unwrap_or_else(|| "<runtime>".to_string());
            return Ok(Box::new(UnsupportedDecl::new(name, "runtime")));
        }

        match compiler.ast_nodes[decl_node.0] {
            AstNode::Def {
                name,
                params,
                block,
                ..
            } => {
                let block_id = local_block_id(compiler, block)
                    .and_then(|block_id| self.block_map[block_id.0])
                    .ok_or_else(|| "missing block id for custom command".to_string())?;
                let info = block_infos
                    .get(local_block_id(compiler, block).expect("checked above").0)
                    .copied()
                    .unwrap_or_default();
                let mut signature =
                    self.signature_for_params(compiler, Some(name), Some(params), info)?;
                signature.category = Category::Custom("new-nu-parser".to_string());
                Ok(signature.into_block_command(block_id))
            }
            AstNode::Alias { old_name, .. } => {
                if let Some(mapped) = compiler
                    .decl_resolution
                    .get(&old_name)
                    .and_then(|decl_id| self.decl_map.get(decl_id.0))
                    .and_then(|decl_id| *decl_id)
                {
                    let decl = self.engine_state.get_decl(mapped).clone_box();
                    Ok(decl)
                } else {
                    Ok(Box::new(UnsupportedDecl::new(
                        compiler.decls_name(decl_node),
                        "alias",
                    )))
                }
            }
            AstNode::Extern { name, .. } => Ok(Box::new(UnsupportedDecl::new(
                trim_decl_name(compiler.node_as_str(name)).to_string(),
                "extern",
            ))),
            _ => Ok(Box::new(UnsupportedDecl::new(
                compiler.decls_name(decl_node),
                "declaration",
            ))),
        }
    }

    fn make_block(
        &self,
        compiler: &Compiler,
        node_id: NodeId,
        info: BlockInfo,
    ) -> Result<NuBlock, String> {
        let mut ir_generator =
            IrGenerator::with_id_maps(compiler, &self.var_map, &self.decl_map, &self.block_map)
                .with_run_external_decl(self.run_external_decl());
        ir_generator.generate_for_node(node_id);

        if !ir_generator.errors().is_empty() {
            return Err(ir_generator.display_state());
        }

        let mut block = NuBlock::new();
        block.signature =
            Box::new(self.signature_for_params(compiler, info.name, info.params, info)?);
        block.ir_block = Some(ir_generator.block());
        block.span = Some(nu_span(compiler, node_id));
        block.redirect_env = info.redirect_env;
        Ok(block)
    }

    fn signature_for_params(
        &self,
        compiler: &Compiler,
        name: Option<NodeId>,
        params: Option<NodeId>,
        _info: BlockInfo,
    ) -> Result<Signature, String> {
        let name = name
            .map(|node_id| trim_decl_name(compiler.node_as_str(node_id)).to_string())
            .unwrap_or_default();
        let mut signature = Signature::new(name).input_output_type(NuType::Any, NuType::Any);

        if let Some(params) = params {
            add_params_to_signature(compiler, params, &self.var_map, &mut signature)?;
        }

        Ok(signature)
    }

    fn run_external_decl(&self) -> Option<NuDeclId> {
        self.engine_state.find_decl(b"run-external", &[])
    }
}

#[derive(Clone)]
struct UnsupportedDecl {
    name: String,
    description: String,
}

impl UnsupportedDecl {
    fn new(name: String, kind: &'static str) -> Self {
        Self {
            description: format!(
                "{kind} declarations are not executable by the new parser bridge yet"
            ),
            name,
        }
    }
}

impl NuCommand for UnsupportedDecl {
    fn name(&self) -> &str {
        &self.name
    }

    fn signature(&self) -> Signature {
        Signature::new(self.name.clone())
            .input_output_type(NuType::Any, NuType::Any)
            .allows_unknown_args()
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &NuCall,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        Err(ShellError::GenericError {
            error: self.description.clone(),
            msg: format!("`{}` cannot be evaluated yet", self.name),
            span: Some(call.head),
            help: None,
            inner: vec![],
        })
    }
}

fn collect_block_infos(compiler: &Compiler) -> Vec<BlockInfo> {
    let mut infos = vec![BlockInfo::default(); compiler.blocks.len()];

    for (idx, node) in compiler.ast_nodes.iter().enumerate() {
        if let AstNode::Block(block_id) = node {
            infos[block_id.0].node_id = Some(NodeId(idx));
        }
    }

    for node in &compiler.ast_nodes {
        match *node {
            AstNode::Closure { params, block } => {
                if let Some(block_id) = local_block_id(compiler, block) {
                    infos[block_id.0].params = params;
                }
            }
            AstNode::Def {
                name,
                params,
                block,
                env,
                ..
            } => {
                if let Some(block_id) = local_block_id(compiler, block) {
                    infos[block_id.0].name = Some(name);
                    infos[block_id.0].params = Some(params);
                    infos[block_id.0].redirect_env = env;
                }
            }
            AstNode::ExportEnv { block } => {
                if let Some(block_id) = local_block_id(compiler, block) {
                    infos[block_id.0].redirect_env = true;
                }
            }
            _ => {}
        }
    }

    infos
}

fn add_params_to_signature(
    compiler: &Compiler,
    params: NodeId,
    var_map: &[Option<NuVarId>],
    signature: &mut Signature,
) -> Result<(), String> {
    for param in &compiler.get_params(params).nodes {
        let AstNode::Param { name, ty } = compiler.ast_nodes[param.0] else {
            continue;
        };
        let var_id = compiler
            .var_resolution
            .get(&name)
            .and_then(|var_id| var_map.get(var_id.0))
            .and_then(|var_id| *var_id)
            .ok_or_else(|| {
                format!(
                    "missing variable id for parameter `{}`",
                    compiler.node_as_str(name)
                )
            })?;

        let param_text = compiler.node_as_str(*param);
        let name_text = compiler.node_as_str(name);

        if name_text.starts_with('-') {
            let (long, short) = flag_names(param_text, name_text);
            signature.named.push(Flag {
                long,
                short,
                arg: ty.map(|_| SyntaxShape::Any),
                required: false,
                desc: String::new(),
                var_id: Some(var_id),
                default_value: None,
            });
        } else if param_text.trim_start().starts_with("...") {
            signature.rest_positional = Some(positional_arg(name_text, var_id));
        } else if param_is_optional(param_text, name_text) {
            signature
                .optional_positional
                .push(positional_arg(name_text, var_id));
        } else {
            signature
                .required_positional
                .push(positional_arg(name_text, var_id));
        }
    }

    Ok(())
}

fn positional_arg(name: &str, var_id: NuVarId) -> PositionalArg {
    PositionalArg {
        name: trim_var_name(name).to_string(),
        desc: String::new(),
        shape: SyntaxShape::Any,
        var_id: Some(var_id),
        default_value: None,
    }
}

fn flag_names(param_text: &str, name_text: &str) -> (String, Option<char>) {
    let long = name_text
        .strip_prefix("--")
        .map(|name| name.to_string())
        .unwrap_or_default();
    let mut short = name_text
        .strip_prefix('-')
        .filter(|_| long.is_empty())
        .and_then(|name| name.chars().next());

    if let Some(idx) = param_text.find("(-") {
        short = param_text[idx + 2..].chars().next();
    }

    (long, short)
}

fn param_is_optional(param_text: &str, name_text: &str) -> bool {
    let after_name = param_text
        .find(name_text)
        .map(|idx| &param_text[idx + name_text.len()..])
        .unwrap_or("");
    let before_type = after_name.split(':').next().unwrap_or(after_name);
    before_type.contains('?') || before_type.contains('=')
}

fn variable_name_and_span(compiler: &Compiler, var_id: VarId) -> (String, NuSpan) {
    compiler
        .var_resolution
        .iter()
        .find_map(|(node_id, resolved)| {
            (*resolved == var_id).then(|| {
                (
                    trim_var_name(compiler.node_as_str(*node_id)).to_string(),
                    nu_span(compiler, *node_id),
                )
            })
        })
        .unwrap_or_else(|| {
            (
                format!("__new_nu_parser_var_{}", var_id.0),
                NuSpan::unknown(),
            )
        })
}

fn find_engine_decl_for_call(
    engine_state: &EngineState,
    compiler: &Compiler,
    parts: &[NodeId],
) -> Option<(String, NuDeclId)> {
    let max_name_parts = parts
        .iter()
        .take_while(|part| matches!(compiler.ast_nodes[part.0], AstNode::Name))
        .count();

    let first = parts.first()?;
    let first_start = compiler.get_span(*first).start;

    for last_name_part in (0..max_name_parts).rev() {
        let last_end = compiler.get_span(parts[last_name_part]).end;
        let candidate =
            String::from_utf8_lossy(compiler.get_span_contents_manual(first_start, last_end));
        let name = trim_decl_name(&candidate);
        if let Some(decl_id) = engine_state.find_decl(name.as_bytes(), &[]) {
            return Some((name.to_string(), decl_id));
        }
    }

    None
}

fn local_block_id(compiler: &Compiler, node_id: NodeId) -> Option<BlockId> {
    match compiler.ast_nodes.get(node_id.0) {
        Some(AstNode::Block(block_id)) => Some(*block_id),
        _ => None,
    }
}

fn runtime_decl_node(nu_decl_id: NuDeclId) -> NodeId {
    NodeId(RUNTIME_DECL_NODE_BASE + nu_decl_id.get())
}

fn nu_span(compiler: &Compiler, node_id: NodeId) -> NuSpan {
    let span = compiler.get_span(node_id);
    NuSpan::new(span.start, span.end)
}

fn trim_var_name(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
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

fn is_reserved_runtime_variable(name: &[u8]) -> bool {
    matches!(name, b"$in" | b"$env" | b"$nu")
}

fn format_shell_error(err: ShellError) -> String {
    format!("{err:?}")
}

trait CompilerDeclName {
    fn decls_name(&self, decl_node: NodeId) -> String;
}

impl CompilerDeclName for Compiler {
    fn decls_name(&self, decl_node: NodeId) -> String {
        self.decl_resolution
            .get(&decl_node)
            .and_then(|decl_id| self.decls.get(decl_id.0))
            .map(|decl| decl.name().to_string())
            .unwrap_or_else(|| "<declaration>".to_string())
    }
}
