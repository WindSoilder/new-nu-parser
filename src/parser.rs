use crate::compiler::{Compiler, RollbackPoint, Span};
use crate::errors::{Severity, SourceError};
use crate::lexer::{Token, Tokens};

use tracy_client::span;

pub struct Parser {
    pub compiler: Compiler,
    tokens: Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamsId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InOutTypesId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeArgsId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineId(pub usize);

#[derive(Debug, Clone)]
pub struct Block {
    pub nodes: Vec<NodeId>,
}

impl Block {
    pub fn new(nodes: Vec<NodeId>) -> Block {
        Block { nodes }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub nodes: Vec<NodeId>,
}

impl Params {
    pub fn new(nodes: Vec<NodeId>) -> Self {
        Self { nodes }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InOutTypes {
    pub nodes: Vec<NodeId>,
}

impl InOutTypes {
    pub fn new(nodes: Vec<NodeId>) -> Self {
        Self { nodes }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub parts: Vec<NodeId>,
}

impl Call {
    pub fn new(parts: Vec<NodeId>) -> Self {
        Self { parts }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub items: Vec<NodeId>,
}

impl List {
    pub fn new(items: Vec<NodeId>) -> Self {
        Self { items }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub header: NodeId,
    pub rows: Vec<NodeId>,
}

impl Table {
    pub fn new(header: NodeId, rows: Vec<NodeId>) -> Self {
        Self { header, rows }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub pairs: Vec<(NodeId, NodeId)>,
}

impl Record {
    pub fn new(pairs: Vec<(NodeId, NodeId)>) -> Self {
        Self { pairs }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub target: NodeId,
    pub match_arms: Vec<(NodeId, NodeId)>,
}

impl Match {
    pub fn new(target: NodeId, match_arms: Vec<(NodeId, NodeId)>) -> Self {
        Self { target, match_arms }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeArgs {
    pub args: Vec<NodeId>,
}

impl TypeArgs {
    pub fn new(args: Vec<NodeId>) -> Self {
        Self { args }
    }
}

// Pipeline just contains a list of expressions.
//
// It's not allowed if there is only one element in pipeline, in that
// case, it's just an expression.
//
// Making such restriction can reduce indirect access on expression, which
// can improve performance in parse time.
#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub nodes: Vec<NodeId>,
}

impl Pipeline {
    pub fn new(nodes: Vec<NodeId>) -> Self {
        debug_assert!(
            nodes.len() > 1,
            "a pipeline must contain at least 2 nodes, or else it's actually an expression"
        );
        Self { nodes }
    }

    pub fn get_expressions(&self) -> &Vec<NodeId> {
        &self.nodes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockContext {
    /// This block is a whole block of code not wrapped in curlies (e.g., a file)
    Bare,
    /// This block is wrapped in curlies
    Curlies,
    /// This block should be parsed as part of a closure starting after closure params
    Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsContext {
    /// Params for a command signature
    Squares,
    /// Params for a closure
    Pipes,
    /// Fields for a record
    Angles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarewordContext {
    /// Bareword is a string (e.g., in a list or command argument)
    String,
    /// Bareword is a command head
    Call,
}

#[derive(Clone, Copy)]
enum OperatorPattern {
    Token(Token, AstNode),
    Keyword(&'static [u8], AstNode),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AstNode {
    Int,
    Float,
    String,
    Name,
    Type {
        name: NodeId,
        args: Option<NodeId>,
        optional: bool,
    },
    TypeArgs(TypeArgsId),
    RecordType {
        /// Contains [AstNode::Params]
        fields: NodeId,
        optional: bool,
    },
    Variable,

    // Booleans
    True,
    False,

    // Empty values
    Null,

    // Operators
    Pow,
    Multiply,
    Divide,
    FloorDiv,
    Modulo,
    Plus,
    Minus,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    RegexMatch,
    NotRegexMatch,
    In,
    NotIn,
    Has,
    NotHas,
    Like,
    NotLike,
    StartsWith,
    NotStartsWith,
    EndsWith,
    NotEndsWith,
    Append,
    BitOr,
    BitXor,
    BitAnd,
    BitShiftLeft,
    BitShiftRight,
    And,
    Xor,
    Or,
    Not,

    // Assignments
    Assignment,
    AddAssignment,
    SubtractAssignment,
    MultiplyAssignment,
    DivideAssignment,
    AppendAssignment,

    // Statements
    Let {
        variable_name: NodeId,
        ty: Option<NodeId>,
        initializer: NodeId,
        is_mutable: bool,
    },
    Const {
        variable_name: NodeId,
        ty: Option<NodeId>,
        initializer: NodeId,
    },
    While {
        condition: NodeId,
        block: NodeId,
    },
    For {
        variable: NodeId,
        range: NodeId,
        block: NodeId,
    },
    Loop {
        block: NodeId,
    },
    Return(Option<NodeId>),
    Break,
    Continue,

    // Definitions
    Def {
        name: NodeId,
        type_params: Option<NodeId>,
        params: NodeId,
        in_out_types: Option<NodeId>,
        block: NodeId,
        env: bool,
        wrapped: bool,
    },
    Extern {
        name: NodeId,
        params: NodeId,
    },
    Params(ParamsId),
    Param {
        name: NodeId,
        ty: Option<NodeId>,
    },
    InOutTypes(InOutTypesId),
    /// Input/output type pair for a command
    InOutType(NodeId, NodeId),
    Closure {
        params: Option<NodeId>,
        block: NodeId,
    },
    Alias {
        new_name: NodeId,
        old_name: NodeId,
    },
    Module {
        name: NodeId,
        block: Option<NodeId>,
    },
    Use {
        pattern: NodeId,
    },
    Source {
        source: NodeId,
        env: bool,
    },
    Export {
        declaration: NodeId,
    },
    ExportEnv {
        block: NodeId,
    },
    Hide {
        pattern: NodeId,
    },
    Overlay {
        action: NodeId,
    },
    PluginUse {
        source: NodeId,
    },

    /// Long flag ('--' + one or more letters)
    FlagLong,
    /// Short flag ('-' + single letter)
    FlagShort,
    /// Group of short flags ('-' + more than 1 letters)
    FlagShortGroup,

    // Expressions
    Call(CallId),
    NamedValue {
        name: NodeId,
        value: NodeId,
    },
    UnaryOp {
        op: NodeId,
        expr: NodeId,
    },
    BinaryOp {
        lhs: NodeId,
        op: NodeId,
        rhs: NodeId,
    },
    Range {
        lhs: NodeId,
        rhs: NodeId,
    },
    List(ListId),
    Table(TableId),
    Record(RecordId),
    MemberAccess {
        target: NodeId,
        field: NodeId,
    },
    Block(BlockId),
    Pipeline(PipelineId),
    If {
        condition: NodeId,
        then_block: NodeId,
        else_block: Option<NodeId>,
    },
    Try {
        try_block: NodeId,
        catch_block: Option<NodeId>,
        finally_block: Option<NodeId>,
    },
    Match(MatchId),
    Statement(NodeId),
    Spread(NodeId),
    Redirection {
        source: NodeId,
        op: NodeId,
        target: NodeId,
    },
    EnvAssignment {
        name: NodeId,
        value: NodeId,
    },
    Garbage,
}

pub const ASSIGNMENT_PRECEDENCE: usize = 10;

impl AstNode {
    pub fn precedence(&self) -> usize {
        match self {
            AstNode::Pow => 100,
            AstNode::Multiply | AstNode::Divide | AstNode::FloorDiv | AstNode::Modulo => 95,
            AstNode::Plus | AstNode::Minus => 90,
            AstNode::LessThan
            | AstNode::LessThanOrEqual
            | AstNode::GreaterThan
            | AstNode::GreaterThanOrEqual
            | AstNode::Equal
            | AstNode::NotEqual
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
            | AstNode::Append => 80,
            AstNode::BitAnd => 70,
            AstNode::BitXor => 65,
            AstNode::BitOr => 60,
            AstNode::And => 50,
            AstNode::Xor => 45,
            AstNode::Or => 40,
            AstNode::Assignment
            | AstNode::AddAssignment
            | AstNode::SubtractAssignment
            | AstNode::MultiplyAssignment
            | AstNode::DivideAssignment
            | AstNode::AppendAssignment => ASSIGNMENT_PRECEDENCE,
            _ => 0,
        }
    }
}

impl Parser {
    pub fn new(compiler: Compiler, tokens: Tokens) -> Self {
        Self { compiler, tokens }
    }

    fn position(&self) -> usize {
        self.tokens.peek_span().start
    }

    fn get_span_end(&self, node_id: NodeId) -> usize {
        self.compiler.spans[node_id.0].end
    }

    pub fn parse(mut self) -> Compiler {
        let _span = span!();
        self.block(BlockContext::Bare);
        self.compiler
    }

    pub fn expression(&mut self) -> NodeId {
        let _span = span!();
        self.expression_with_bareword(BarewordContext::Call)
    }

    fn expression_with_bareword(&mut self, bareword_context: BarewordContext) -> NodeId {
        self.range(bareword_context)
    }

    fn range(&mut self, bareword_context: BarewordContext) -> NodeId {
        if let Some(op_span) = self.match_range_operator() {
            let lhs = self.empty_range_bound(op_span.start);
            let rhs = self.optional_range_bound(op_span.end, bareword_context);
            let span_end = self.get_span_end(rhs);
            return self.create_node(AstNode::Range { lhs, rhs }, op_span.start, span_end);
        }

        let lhs = self.logical_or(bareword_context);

        if let Some(op_span) = self.match_range_operator() {
            let rhs = self.optional_range_bound(op_span.end, bareword_context);
            let mut expr = self.create_node(
                AstNode::Range { lhs, rhs },
                self.compiler.spans[lhs.0].start,
                self.get_span_end(rhs),
            );

            if let Some(second_op_span) = self.match_range_operator() {
                let rhs = self.optional_range_bound(second_op_span.end, bareword_context);
                expr = self.create_node(
                    AstNode::Range { lhs: expr, rhs },
                    self.compiler.spans[lhs.0].start,
                    self.get_span_end(rhs),
                );
            }

            expr
        } else {
            lhs
        }
    }

    fn optional_range_bound(
        &mut self,
        fallback_pos: usize,
        bareword_context: BarewordContext,
    ) -> NodeId {
        if self.is_expression_start() {
            self.logical_or(bareword_context)
        } else {
            self.empty_range_bound(fallback_pos)
        }
    }

    fn empty_range_bound(&mut self, pos: usize) -> NodeId {
        self.create_node(AstNode::Null, pos, pos)
    }

    fn logical_or(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"or", AstNode::Or)];
        self.binary_left(bareword_context, Parser::logical_xor, &ops)
    }

    fn logical_xor(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"xor", AstNode::Xor)];
        self.binary_left(bareword_context, Parser::logical_and, &ops)
    }

    fn logical_and(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"and", AstNode::And)];
        self.binary_left(bareword_context, Parser::bit_or, &ops)
    }

    fn bit_or(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"bit-or", AstNode::BitOr)];
        self.binary_left(bareword_context, Parser::bit_xor, &ops)
    }

    fn bit_xor(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"bit-xor", AstNode::BitXor)];
        self.binary_left(bareword_context, Parser::bit_and, &ops)
    }

    fn bit_and(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [OperatorPattern::Keyword(b"bit-and", AstNode::BitAnd)];
        self.binary_left(bareword_context, Parser::comparison, &ops)
    }

    fn comparison(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [
            OperatorPattern::Token(Token::EqualsEquals, AstNode::Equal),
            OperatorPattern::Token(Token::ExclamationEquals, AstNode::NotEqual),
            OperatorPattern::Token(Token::LessThan, AstNode::LessThan),
            OperatorPattern::Token(Token::LessThanEqual, AstNode::LessThanOrEqual),
            OperatorPattern::Token(Token::GreaterThan, AstNode::GreaterThan),
            OperatorPattern::Token(Token::GreaterThanEqual, AstNode::GreaterThanOrEqual),
            OperatorPattern::Token(Token::EqualsTilde, AstNode::RegexMatch),
            OperatorPattern::Token(Token::ExclamationTilde, AstNode::NotRegexMatch),
            OperatorPattern::Token(Token::PlusPlus, AstNode::Append),
            OperatorPattern::Keyword(b"in", AstNode::In),
            OperatorPattern::Keyword(b"not-in", AstNode::NotIn),
            OperatorPattern::Keyword(b"has", AstNode::Has),
            OperatorPattern::Keyword(b"not-has", AstNode::NotHas),
            OperatorPattern::Keyword(b"like", AstNode::Like),
            OperatorPattern::Keyword(b"not-like", AstNode::NotLike),
            OperatorPattern::Keyword(b"starts-with", AstNode::StartsWith),
            OperatorPattern::Keyword(b"not-starts-with", AstNode::NotStartsWith),
            OperatorPattern::Keyword(b"ends-with", AstNode::EndsWith),
            OperatorPattern::Keyword(b"not-ends-with", AstNode::NotEndsWith),
        ];
        self.binary_left(bareword_context, Parser::shift, &ops)
    }

    fn shift(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [
            OperatorPattern::Keyword(b"bit-shl", AstNode::BitShiftLeft),
            OperatorPattern::Keyword(b"bit-shr", AstNode::BitShiftRight),
        ];
        self.binary_left(bareword_context, Parser::addition, &ops)
    }

    fn addition(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [
            OperatorPattern::Token(Token::Plus, AstNode::Plus),
            OperatorPattern::Token(Token::Dash, AstNode::Minus),
        ];
        self.binary_left(bareword_context, Parser::multiply, &ops)
    }

    fn multiply(&mut self, bareword_context: BarewordContext) -> NodeId {
        let ops = [
            OperatorPattern::Token(Token::Asterisk, AstNode::Multiply),
            OperatorPattern::Token(Token::ForwardSlash, AstNode::Divide),
            OperatorPattern::Token(Token::ForwardSlashForwardSlash, AstNode::FloorDiv),
            OperatorPattern::Keyword(b"mod", AstNode::Modulo),
        ];
        self.binary_left(bareword_context, Parser::power, &ops)
    }

    fn power(&mut self, bareword_context: BarewordContext) -> NodeId {
        let lhs = self.unary(bareword_context);
        let ops = [OperatorPattern::Token(
            Token::AsteriskAsterisk,
            AstNode::Pow,
        )];

        if let Some(op) = self.match_binary_operator(&ops) {
            let rhs = if self.is_expression_start() {
                self.power(bareword_context)
            } else {
                self.error("incomplete expression")
            };
            let (span_start, span_end) = self.spanning(lhs, rhs);
            self.create_node(AstNode::BinaryOp { lhs, op, rhs }, span_start, span_end)
        } else {
            lhs
        }
    }

    fn unary(&mut self, bareword_context: BarewordContext) -> NodeId {
        let op = if self.is_keyword(b"not") {
            let span = self.tokens.peek_span();
            Some(self.advance_node(AstNode::Not, span))
        } else {
            match self.tokens.peek() {
                (Token::Plus, span) => Some(self.advance_node(AstNode::Plus, span)),
                (Token::Dash, span) => Some(self.advance_node(AstNode::Minus, span)),
                _ => None,
            }
        };

        if let Some(op) = op {
            let expr = self.unary(bareword_context);
            let (span_start, span_end) = self.spanning(op, expr);
            self.create_node(AstNode::UnaryOp { op, expr }, span_start, span_end)
        } else {
            self.postfix(bareword_context)
        }
    }

    fn postfix(&mut self, bareword_context: BarewordContext) -> NodeId {
        let mut expr = self.primary(bareword_context);
        let span_start = self.compiler.spans[expr.0].start;

        while self.is_dot() {
            self.tokens.advance();

            if self.is_horizontal_space() {
                self.error("missing path name");
                return expr;
            }

            let field = self.path_member();
            let mut span_end = self.get_span_end(field);
            if let Some(span) = self.match_token(Token::QuestionMark) {
                span_end = span.end;
            }

            expr = self.create_node(
                AstNode::MemberAccess {
                    target: expr,
                    field,
                },
                span_start,
                span_end,
            );
        }

        expr
    }

    fn primary(&mut self, bareword_context: BarewordContext) -> NodeId {
        while self.is_comment() {
            self.tokens.advance();
        }

        let (token, span) = self.tokens.peek();

        match token {
            Token::LCurly => self.record_or_closure(),
            Token::LParen => self.subexpression(),
            Token::LSquare => self.list_or_table(),
            Token::Int => self.advance_node(AstNode::Int, span),
            Token::Float => self.advance_node(AstNode::Float, span),
            Token::DoubleQuotedString
            | Token::SingleQuotedString
            | Token::RawString
            | Token::BacktickBareword
            | Token::Datetime => self.advance_node(AstNode::String, span),
            Token::DqStringInterpStart | Token::SqStringInterpStart => self.interpolated_string(),
            Token::Dollar => {
                if self
                    .peek_next_token()
                    .is_some_and(|(next, _)| next == Token::Dot)
                {
                    self.cell_path_literal(true)
                } else {
                    self.variable()
                }
            }
            Token::Dot => self.cell_path_literal(false),
            Token::Caret if bareword_context == BarewordContext::Call => self.call(),
            Token::Bareword => match self.compiler.get_span_contents_manual(span.start, span.end) {
                b"if" => self.if_expression(),
                b"try" => self.try_expression(),
                b"match" => self.match_expression(),
                b"true" => self.advance_node(AstNode::True, span),
                b"false" => self.advance_node(AstNode::False, span),
                b"null" => self.advance_node(AstNode::Null, span),
                _ => match bareword_context {
                    BarewordContext::String => {
                        let node_id = self.name();
                        self.compiler.ast_nodes[node_id.0] = AstNode::String;
                        node_id
                    }
                    BarewordContext::Call => self.call(),
                },
            },
            _ => self.error("incomplete expression"),
        }
    }

    fn binary_left(
        &mut self,
        bareword_context: BarewordContext,
        next: fn(&mut Self, BarewordContext) -> NodeId,
        ops: &[OperatorPattern],
    ) -> NodeId {
        let mut expr = next(self, bareword_context);

        while let Some(op) = self.match_binary_operator(ops) {
            let rhs = if self.is_expression_start() {
                next(self, bareword_context)
            } else {
                self.error("incomplete expression")
            };
            let (span_start, span_end) = self.spanning(expr, rhs);
            expr = self.create_node(
                AstNode::BinaryOp { lhs: expr, op, rhs },
                span_start,
                span_end,
            );
        }

        expr
    }

    fn match_binary_operator(&mut self, ops: &[OperatorPattern]) -> Option<NodeId> {
        for op in ops {
            let matched = match *op {
                OperatorPattern::Token(token, node) if self.check(token) => {
                    let span = self.tokens.peek_span();
                    self.tokens.advance();
                    Some((node, span))
                }
                OperatorPattern::Keyword(keyword, node) => {
                    self.match_keyword_span(keyword).map(|span| (node, span))
                }
                _ => None,
            };

            if let Some((node, span)) = matched {
                let missing_space_before_op = !self.has_horizontal_space_before(span.start);
                let op = self.create_node(node, span.start, span.end);
                let missing_space_after_op = !self.is_horizontal_space();

                if missing_space_before_op {
                    self.error_on_node("missing space before operator", op);
                }
                if missing_space_after_op {
                    self.error_on_node("missing space after operator", op);
                }

                return Some(op);
            }
        }

        None
    }

    fn match_range_operator(&mut self) -> Option<Span> {
        let mut span = self.match_token(Token::DotDot)?;
        if self.check(Token::LessThan) && self.tokens.peek_span().start == span.end {
            let end = self.tokens.peek_span().end;
            self.tokens.advance();
            span.end = end;
        }
        Some(span)
    }

    fn pipeline(&mut self, first_element: NodeId, span_start: usize) -> NodeId {
        let mut expressions = vec![first_element];

        while self.is_pipeline_pipe() {
            self.pipeline_pipe();
            self.skip_newlines();
            expressions.push(self.pipe_element());
        }

        if expressions.len() == 1 {
            first_element
        } else {
            self.compiler.pipelines.push(Pipeline::new(expressions));
            let span_end = self.position();
            self.create_node(
                AstNode::Pipeline(PipelineId(self.compiler.pipelines.len() - 1)),
                span_start,
                span_end,
            )
        }
    }

    pub fn pipeline_or_expression_or_assignment(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let first = self.expression_command();

        if self.is_assignment_operator() {
            if !self.is_assignment_target(first) {
                self.error_on_node("invalid assignment target", first);
            }

            let op = self.assignment_operator();
            let rhs = self.pipeline_or_expression();
            let span_end = self.get_span_end(rhs);
            return self.create_node(
                AstNode::BinaryOp {
                    lhs: first,
                    op,
                    rhs,
                },
                span_start,
                span_end,
            );
        }

        let first = self.finish_redirections(first, span_start);
        self.pipeline(first, span_start)
    }

    pub fn pipeline_or_expression(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let first = self.pipe_element();
        self.pipeline(first, span_start)
    }

    fn pipe_element(&mut self) -> NodeId {
        let span_start = self.position();
        let command = self.expression_command();
        self.finish_redirections(command, span_start)
    }

    fn expression_command(&mut self) -> NodeId {
        let mut last_env_assignment = None;
        while self.is_environment_assignment() {
            last_env_assignment = Some(self.environment_assignment());
        }

        if self.is_command_start() {
            self.command()
        } else if self.is_expression_start() {
            self.expression_with_bareword(BarewordContext::Call)
        } else if let Some(env_assignment) = last_env_assignment {
            env_assignment
        } else {
            self.error("expected command or expression")
        }
    }

    fn command(&mut self) -> NodeId {
        self.call()
    }

    pub fn call(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let call_name = self.call_name();
        let is_external = self.is_external_call_name(call_name);
        let mut parts = vec![call_name];
        let mut is_head = true;

        while self.has_tokens() && !self.is_command_boundary() {
            if !is_external
                && self.is_name()
                && is_head
                && !self.is_unquoted_word_argument_with_continuation()
            {
                parts.push(self.name());
            } else {
                is_head = false;
                let arg = if is_external {
                    self.external_argument()
                } else {
                    self.argument()
                };
                parts.push(arg);
            }
        }

        let span_end = parts
            .last()
            .map_or(span_start, |part| self.get_span_end(*part));

        self.compiler.calls.push(Call::new(parts));
        self.create_node(
            AstNode::Call(CallId(self.compiler.calls.len() - 1)),
            span_start,
            span_end,
        )
    }

    fn external_argument(&mut self) -> NodeId {
        if self.is_spread() {
            self.spread()
        } else if self.is_external_expression_argument_start() {
            self.expression_with_bareword(BarewordContext::String)
        } else {
            self.external_word_argument()
        }
    }

    fn external_word_argument(&mut self) -> NodeId {
        let span_start = self.position();
        let mut span_end = span_start;
        let mut consumed = false;

        while self.has_tokens() && !self.is_command_boundary() {
            let (token, span) = self.tokens.peek();
            if consumed && self.has_horizontal_space_before(span.start) {
                break;
            }
            if !is_external_word_token(token) {
                break;
            }

            self.tokens.advance();
            span_end = span.end;
            consumed = true;
        }

        if consumed {
            self.create_node(AstNode::String, span_start, span_end)
        } else {
            self.error("expected external argument")
        }
    }

    fn argument(&mut self) -> NodeId {
        if self.is_long_flag_start() || self.is_short_flag_start() {
            self.flag_argument()
        } else if self.is_spread() {
            self.spread()
        } else if self.is_unquoted_word_argument_with_continuation() {
            self.external_word_argument()
        } else {
            self.expression_with_bareword(BarewordContext::String)
        }
    }

    fn finish_redirections(&mut self, mut source: NodeId, span_start: usize) -> NodeId {
        while self.is_file_redirection() {
            let op = self.file_redirection_operator();
            let target = if self.is_expression_start() {
                self.expression_with_bareword(BarewordContext::String)
            } else {
                self.error("expected redirection target")
            };
            let span_end = self.get_span_end(target);
            source = self.create_node(
                AstNode::Redirection { source, op, target },
                span_start,
                span_end,
            );
        }

        source
    }

    fn environment_assignment(&mut self) -> NodeId {
        let span_start = self.position();
        let name = self.name();
        self.equals();
        let value = if self.is_string() {
            self.string()
        } else if self.is_dollar() {
            self.variable()
        } else if self.is_name() {
            let value = self.name();
            self.compiler.ast_nodes[value.0] = AstNode::String;
            value
        } else {
            self.error("expected environment assignment value")
        };
        let span_end = self.get_span_end(value);
        self.create_node(AstNode::EnvAssignment { name, value }, span_start, span_end)
    }

    fn assignment_operator(&mut self) -> NodeId {
        let (token, span) = self.tokens.peek();
        match token {
            Token::Equals => self.advance_node(AstNode::Assignment, span),
            Token::PlusEquals => self.advance_node(AstNode::AddAssignment, span),
            Token::DashEquals => self.advance_node(AstNode::SubtractAssignment, span),
            Token::AsteriskEquals => self.advance_node(AstNode::MultiplyAssignment, span),
            Token::ForwardSlashEquals => self.advance_node(AstNode::DivideAssignment, span),
            Token::PlusPlusEquals => self.advance_node(AstNode::AppendAssignment, span),
            _ => self.error("expected assignment operator"),
        }
    }

    fn is_assignment_target(&self, node_id: NodeId) -> bool {
        matches!(
            self.compiler.ast_nodes[node_id.0],
            AstNode::Variable | AstNode::MemberAccess { .. } | AstNode::Name
        )
    }

    pub fn simple_expression(&mut self, bareword_context: BarewordContext) -> NodeId {
        self.postfix(bareword_context)
    }

    pub fn advance_node(&mut self, node: AstNode, span: Span) -> NodeId {
        self.tokens.advance();
        self.create_node(node, span.start, span.end)
    }

    pub fn variable(&mut self) -> NodeId {
        if self.is_dollar() {
            let span_start = self.position();
            self.tokens.advance();

            if let (Token::Bareword, name_span) = self.tokens.peek() {
                self.tokens.advance();
                self.create_node(AstNode::Variable, span_start, name_span.end)
            } else {
                self.error("variable name must be a bareword")
            }
        } else {
            self.error("expected variable starting with '$'")
        }
    }

    pub fn variable_decl(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        if self.is_dollar() {
            self.tokens.advance();
        }

        if let (Token::Bareword, name_span) = self.tokens.peek() {
            self.tokens.advance();
            self.create_node(AstNode::Variable, span_start, name_span.end)
        } else {
            self.error("variable assignment name must be a bareword")
        }
    }

    fn subexpression(&mut self) -> NodeId {
        let span_start = self.position();
        self.lparen();

        self.skip_terminators();
        if self.is_rparen() {
            let span_end = self.tokens.peek_span().end;
            self.rparen();
            return self.create_node(AstNode::Null, span_start, span_end);
        }

        let mut nodes = vec![];
        while self.has_tokens() && !self.is_rparen() {
            self.skip_terminators();
            if self.is_rparen() {
                break;
            }

            let before = self.tokens.pos();
            nodes.push(self.pipeline_or_expression_or_assignment());

            if self.is_semicolon() {
                self.tokens.advance();
            } else if !self.is_rparen() && !self.is_newline() && !self.is_comment() {
                break;
            }

            if self.tokens.pos() == before {
                self.error("expected statement in subexpression");
                break;
            }
        }

        if nodes.len() == 1 {
            let node = nodes[0];
            self.rparen();
            node
        } else {
            self.rparen();
            let span_end = self.position();
            self.compiler.blocks.push(Block::new(nodes));
            self.create_node(
                AstNode::Block(BlockId(self.compiler.blocks.len() - 1)),
                span_start,
                span_end,
            )
        }
    }

    pub fn list_or_table(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let mut is_table = false;
        let mut items = vec![];

        self.lsquare();
        self.skip_list_item_separators();
        let mut span_end = self.position();

        while self.has_tokens() {
            if self.is_rsquare() {
                span_end = self.tokens.peek_span().end;
                self.tokens.advance();
                break;
            }

            if self.is_semicolon() {
                if items.len() != 1 {
                    self.error("semicolon to create table should immediately follow headers");
                } else if !matches!(self.compiler.get_node(items[0]), AstNode::List(_)) {
                    self.error_on_node("tables require a list for their headers", items[0])
                }
                self.tokens.advance();
                self.skip_list_item_separators();
                is_table = true;
                continue;
            }

            items.push(self.list_item());
            self.skip_list_item_separators();
        }

        if is_table {
            let header = if items.is_empty() {
                self.compiler.lists.push(List::new(vec![]));
                self.create_node(
                    AstNode::List(ListId(self.compiler.lists.len() - 1)),
                    span_start,
                    span_start,
                )
            } else {
                items.remove(0)
            };
            self.compiler.tables.push(Table::new(header, items));
            self.create_node(
                AstNode::Table(TableId(self.compiler.tables.len() - 1)),
                span_start,
                span_end,
            )
        } else {
            self.compiler.lists.push(List::new(items));
            self.create_node(
                AstNode::List(ListId(self.compiler.lists.len() - 1)),
                span_start,
                span_end,
            )
        }
    }

    fn list_item(&mut self) -> NodeId {
        if self.is_spread() {
            self.spread()
        } else if self.is_expression_start() {
            self.expression_with_bareword(BarewordContext::String)
        } else {
            self.error("expected list item")
        }
    }

    pub fn record_or_closure(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let span_end;
        let mut items = vec![];

        self.lcurly();
        self.skip_newlines();

        if self.is_pipe() || self.is_pipe_pipe() {
            let params = Some(self.signature_params(ParamsContext::Pipes));
            let block = self.block(BlockContext::Closure);
            span_end = self.tokens.peek_span().end;
            self.rcurly();
            return self.create_node(AstNode::Closure { params, block }, span_start, span_end);
        }

        if self.is_rcurly() {
            span_end = self.tokens.peek_span().end;
            self.rcurly();
            self.compiler.records.push(Record::new(items));
            return self.create_node(
                AstNode::Record(RecordId(self.compiler.records.len() - 1)),
                span_start,
                span_end,
            );
        }

        let rollback_point = self.get_rollback_point();
        let mut first_pass = true;

        loop {
            self.skip_separators();
            if self.is_eof() {
                span_end = self.position();
                break;
            }
            if self.is_rcurly() {
                span_end = self.tokens.peek_span().end;
                self.rcurly();
                break;
            }

            let key = self.record_key();
            self.skip_newlines();

            if first_pass && !self.is_colon() {
                self.apply_rollback(rollback_point);
                let block = self.block(BlockContext::Closure);
                span_end = self.tokens.peek_span().end;
                self.rcurly();
                return self.create_node(
                    AstNode::Closure {
                        params: None,
                        block,
                    },
                    span_start,
                    span_end,
                );
            }

            self.colon();
            self.skip_newlines();
            if self.is_rcurly() || self.is_eof() {
                span_end = self.position();
                break;
            }
            let val = self.expression_with_bareword(BarewordContext::String);
            items.push((key, val));
            first_pass = false;

            if !self.match_separator() && !self.is_rcurly() {
                continue;
            }

            if self.is_eof() {
                span_end = self.position();
                break;
            }
        }

        self.compiler.records.push(Record::new(items));
        self.create_node(
            AstNode::Record(RecordId(self.compiler.records.len() - 1)),
            span_start,
            span_end,
        )
    }

    fn record_key(&mut self) -> NodeId {
        if self.is_string() {
            self.string()
        } else if self.is_name() {
            self.name()
        } else {
            self.error("expected record key")
        }
    }

    fn path_member(&mut self) -> NodeId {
        match self.tokens.peek() {
            (Token::Bareword, _) => self.name(),
            (Token::Int, span) => self.advance_node(AstNode::Int, span),
            (Token::DoubleQuotedString, _)
            | (Token::SingleQuotedString, _)
            | (Token::RawString, _)
            | (Token::BacktickBareword, _) => self.string(),
            _ => self.error("expected path member"),
        }
    }

    fn cell_path_literal(&mut self, has_dollar: bool) -> NodeId {
        let span_start = self.position();
        if has_dollar {
            self.tokens.advance();
        }

        let mut span_end = span_start;
        while self.is_dot() {
            self.tokens.advance();
            let member = self.path_member();
            span_end = self.get_span_end(member);
            if let Some(span) = self.match_token(Token::QuestionMark) {
                span_end = span.end;
            }
        }

        self.create_node(AstNode::Name, span_start, span_end)
    }

    fn interpolated_string(&mut self) -> NodeId {
        let span_start = self.position();
        let mut span_end = self.tokens.peek_span().end;
        self.tokens.advance();

        while self.has_tokens() {
            let (token, span) = self.tokens.peek();
            span_end = span.end;
            self.tokens.advance();
            if token == Token::StrInterpEnd {
                break;
            }
        }

        self.create_node(AstNode::String, span_start, span_end)
    }

    pub fn operator(&mut self) -> NodeId {
        let (token, span) = self.tokens.peek();

        match token {
            Token::Plus => self.advance_node(AstNode::Plus, span),
            Token::PlusPlus => self.advance_node(AstNode::Append, span),
            Token::Dash => self.advance_node(AstNode::Minus, span),
            Token::Asterisk => self.advance_node(AstNode::Multiply, span),
            Token::ForwardSlash => self.advance_node(AstNode::Divide, span),
            Token::ForwardSlashForwardSlash => self.advance_node(AstNode::FloorDiv, span),
            Token::LessThan => self.advance_node(AstNode::LessThan, span),
            Token::LessThanEqual => self.advance_node(AstNode::LessThanOrEqual, span),
            Token::GreaterThan => self.advance_node(AstNode::GreaterThan, span),
            Token::GreaterThanEqual => self.advance_node(AstNode::GreaterThanOrEqual, span),
            Token::EqualsEquals => self.advance_node(AstNode::Equal, span),
            Token::ExclamationEquals => self.advance_node(AstNode::NotEqual, span),
            Token::EqualsTilde => self.advance_node(AstNode::RegexMatch, span),
            Token::ExclamationTilde => self.advance_node(AstNode::NotRegexMatch, span),
            Token::AsteriskAsterisk => self.advance_node(AstNode::Pow, span),
            Token::Equals => self.advance_node(AstNode::Assignment, span),
            Token::PlusEquals => self.advance_node(AstNode::AddAssignment, span),
            Token::DashEquals => self.advance_node(AstNode::SubtractAssignment, span),
            Token::AsteriskEquals => self.advance_node(AstNode::MultiplyAssignment, span),
            Token::ForwardSlashEquals => self.advance_node(AstNode::DivideAssignment, span),
            Token::PlusPlusEquals => self.advance_node(AstNode::AppendAssignment, span),
            Token::Bareword => match self.compiler.get_span_contents_manual(span.start, span.end) {
                b"mod" => self.advance_node(AstNode::Modulo, span),
                b"in" => self.advance_node(AstNode::In, span),
                b"not-in" => self.advance_node(AstNode::NotIn, span),
                b"has" => self.advance_node(AstNode::Has, span),
                b"not-has" => self.advance_node(AstNode::NotHas, span),
                b"like" => self.advance_node(AstNode::Like, span),
                b"not-like" => self.advance_node(AstNode::NotLike, span),
                b"starts-with" => self.advance_node(AstNode::StartsWith, span),
                b"not-starts-with" => self.advance_node(AstNode::NotStartsWith, span),
                b"ends-with" => self.advance_node(AstNode::EndsWith, span),
                b"not-ends-with" => self.advance_node(AstNode::NotEndsWith, span),
                b"bit-or" => self.advance_node(AstNode::BitOr, span),
                b"bit-xor" => self.advance_node(AstNode::BitXor, span),
                b"bit-and" => self.advance_node(AstNode::BitAnd, span),
                b"bit-shl" => self.advance_node(AstNode::BitShiftLeft, span),
                b"bit-shr" => self.advance_node(AstNode::BitShiftRight, span),
                b"and" => self.advance_node(AstNode::And, span),
                b"xor" => self.advance_node(AstNode::Xor, span),
                b"or" => self.advance_node(AstNode::Or, span),
                b"not" => self.advance_node(AstNode::Not, span),
                op => self.error(format!(
                    "Unknown operator: '{}'",
                    String::from_utf8_lossy(op)
                )),
            },
            _ => self.error("expected: operator"),
        }
    }

    pub fn operator_precedence(&mut self, operator: NodeId) -> usize {
        self.compiler.get_node(operator).precedence()
    }

    pub fn spanning(&self, from: NodeId, to: NodeId) -> (usize, usize) {
        (
            self.compiler.spans[from.0].start,
            self.compiler.spans[to.0].end,
        )
    }

    pub fn string(&mut self) -> NodeId {
        match self.tokens.peek() {
            (Token::DoubleQuotedString, span)
            | (Token::SingleQuotedString, span)
            | (Token::RawString, span)
            | (Token::BacktickBareword, span)
            | (Token::Datetime, span) => self.advance_node(AstNode::String, span),
            (Token::DqStringInterpStart, _) | (Token::SqStringInterpStart, _) => {
                self.interpolated_string()
            }
            _ => self.error("expected: string"),
        }
    }

    pub fn name(&mut self) -> NodeId {
        match self.tokens.peek() {
            (Token::Bareword, span) => self.advance_node(AstNode::Name, span),
            _ => self.error("expected: name"),
        }
    }

    pub fn call_name(&mut self) -> NodeId {
        let span = self.consume_call_name_span(&[]);
        self.create_node(AstNode::Name, span.start, span.end)
    }

    fn command_name(&mut self, stop_tokens: &[Token]) -> NodeId {
        if self.is_string() {
            return self.string();
        }

        if !self.has_tokens() || self.is_command_boundary() {
            return self.error("expected command name");
        }

        let mut span = self.consume_call_name_span(stop_tokens);
        while self.is_name() && !self.check_any(stop_tokens) {
            let next = self.consume_call_name_span(stop_tokens);
            span.end = next.end;
        }
        self.create_node(AstNode::Name, span.start, span.end)
    }

    fn consume_call_name_span(&mut self, stop_tokens: &[Token]) -> Span {
        let (token, mut span) = self.tokens.peek();
        if token == Token::Eof {
            return span;
        }

        loop {
            self.tokens.advance();
            if self.is_eof() {
                break;
            }

            let next_token = self.tokens.peek_token();
            let next_span = self.tokens.peek_span();

            if next_span.start > span.end
                || self.is_call_name_boundary_token(next_token)
                || stop_tokens.contains(&next_token)
            {
                break;
            }

            span.end = next_span.end;
        }

        span
    }

    pub fn has_tokens(&self) -> bool {
        self.tokens.peek_token() != Token::Eof
    }

    pub fn match_expression(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"match");
        let target = self.expression_with_bareword(BarewordContext::String);
        self.lcurly();
        self.skip_separators();

        let mut match_arms = vec![];
        let mut span_end = self.position();

        while self.has_tokens() && !self.is_rcurly() {
            let pattern = self.pattern();

            if self.is_keyword(b"if") {
                self.tokens.advance();
                self.expression_with_bareword(BarewordContext::String);
            }

            if !self.is_thick_arrow() {
                return self.error("expected thick arrow (=>) between match cases");
            }
            self.tokens.advance();

            let pattern_result = if self.is_lcurly() {
                self.block(BlockContext::Curlies)
            } else {
                self.expression_with_bareword(BarewordContext::String)
            };

            span_end = self.get_span_end(pattern_result);
            match_arms.push((pattern, pattern_result));
            self.skip_separators();
        }

        if self.is_rcurly() {
            span_end = self.tokens.peek_span().end;
            self.rcurly();
        } else {
            self.error("expected right curly brace '}' after match");
        }

        self.compiler.matches.push(Match::new(target, match_arms));
        self.create_node(
            AstNode::Match(MatchId(self.compiler.matches.len() - 1)),
            span_start,
            span_end,
        )
    }

    fn pattern(&mut self) -> NodeId {
        let span_start = self.position();
        let mut patterns = vec![self.single_pattern()];

        while self.is_pipe() {
            self.pipe();
            patterns.push(self.single_pattern());
        }

        if patterns.len() == 1 {
            patterns[0]
        } else {
            let span_end = self.get_span_end(*patterns.last().expect("pattern list is not empty"));
            self.compiler.lists.push(List::new(patterns));
            self.create_node(
                AstNode::List(ListId(self.compiler.lists.len() - 1)),
                span_start,
                span_end,
            )
        }
    }

    fn single_pattern(&mut self) -> NodeId {
        if self.is_keyword(b"_") {
            let node = self.name();
            self.compiler.ast_nodes[node.0] = AstNode::String;
            node
        } else if self.is_lsquare() {
            self.list_or_table()
        } else if self.is_lcurly() {
            self.record_or_closure()
        } else {
            self.expression_with_bareword(BarewordContext::String)
        }
    }

    pub fn if_expression(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"if");
        let condition = self.expression();
        self.skip_newlines();

        let then_block = self.block(BlockContext::Curlies);
        self.skip_newlines();

        let (else_block, span_end) = if self.is_keyword(b"else") {
            self.tokens.advance();
            self.skip_newlines();

            let block = if self.is_keyword(b"if") {
                self.if_expression()
            } else if self.is_keyword(b"match") {
                self.match_expression()
            } else {
                self.block(BlockContext::Curlies)
            };
            (Some(block), self.get_span_end(block))
        } else {
            (None, self.get_span_end(then_block))
        };

        self.create_node(
            AstNode::If {
                condition,
                then_block,
                else_block,
            },
            span_start,
            span_end,
        )
    }

    pub fn try_expression(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"try");

        let try_block = self.block(BlockContext::Curlies);
        let mut span_end = self.get_span_end(try_block);
        self.skip_newlines();

        let catch_block = if self.is_keyword(b"catch") {
            self.tokens.advance();
            self.skip_newlines();

            let block = if self.is_lcurly() && self.peek_after_lcurly_is_pipe() {
                self.record_or_closure()
            } else {
                self.block(BlockContext::Curlies)
            };
            span_end = self.get_span_end(block);
            Some(block)
        } else {
            None
        };

        let finally_block = if self.is_keyword(b"finally") {
            self.tokens.advance();
            self.skip_newlines();

            let block = self.block(BlockContext::Curlies);
            span_end = self.get_span_end(block);
            Some(block)
        } else {
            None
        };

        self.create_node(
            AstNode::Try {
                try_block,
                catch_block,
                finally_block,
            },
            span_start,
            span_end,
        )
    }

    pub fn signature_params(&mut self, params_context: ParamsContext) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        if params_context == ParamsContext::Pipes && self.is_pipe_pipe() {
            let span_end = self.tokens.peek_span().end;
            self.tokens.advance();
            self.compiler.params.push(Params::new(vec![]));
            return self.create_node(
                AstNode::Params(ParamsId(self.compiler.params.len() - 1)),
                span_start,
                span_end,
            );
        }

        match params_context {
            ParamsContext::Pipes => self.pipe(),
            ParamsContext::Squares => self.lsquare(),
            ParamsContext::Angles => self.less_than(),
        }

        let mut param_list = vec![];

        while self.has_tokens() && !self.is_params_end(params_context) {
            if self.match_separator() {
                continue;
            }

            param_list.push(self.signature_parameter(params_context));
        }

        let span_end = match params_context {
            ParamsContext::Pipes => self.consume(Token::Pipe, "expected: pipe symbol '|'"),
            ParamsContext::Squares => self.consume(Token::RSquare, "expected: right bracket ']'"),
            ParamsContext::Angles => self.consume(
                Token::GreaterThan,
                "expected: greater than/right angle bracket '>'",
            ),
        }
        .map_or_else(|| self.position(), |span| span.end);

        self.compiler.params.push(Params::new(param_list));
        self.create_node(
            AstNode::Params(ParamsId(self.compiler.params.len() - 1)),
            span_start,
            span_end,
        )
    }

    fn signature_parameter(&mut self, params_context: ParamsContext) -> NodeId {
        let span_start = self.position();
        let name = if params_context == ParamsContext::Angles {
            self.record_key()
        } else if self.is_spread() {
            self.tokens.advance();
            self.name()
        } else if self.is_long_flag_start() || self.is_short_flag_start() {
            let flag = self.flag_node();
            if self.is_lparen() {
                self.lparen();
                if self.is_short_flag_start() {
                    self.flag_node();
                }
                self.rparen();
            }
            flag
        } else {
            self.name()
        };

        if self.is_question_mark() {
            self.tokens.advance();
        }

        let ty = self.optional_type_annotation();
        let mut span_end = ty.map_or_else(|| self.get_span_end(name), |ty| self.get_span_end(ty));

        if self.is_equals() {
            self.equals();
            let default_value = self.expression_with_bareword(BarewordContext::String);
            span_end = self.get_span_end(default_value);
        }

        self.create_node(AstNode::Param { name, ty }, span_start, span_end)
    }

    pub fn type_params(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.less_than();

        let mut param_list = vec![];

        while self.has_tokens() && !self.is_greater_than() {
            if self.match_separator() {
                continue;
            }

            param_list.push(self.name());
        }

        let span_end = self.tokens.peek_span().end;
        self.greater_than();

        self.compiler.params.push(Params::new(param_list));
        self.create_node(
            AstNode::Params(ParamsId(self.compiler.params.len() - 1)),
            span_start,
            span_end,
        )
    }

    pub fn type_args(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.less_than();

        let mut output = vec![];

        while self.has_tokens() && !self.is_greater_than() {
            if self.match_separator() {
                continue;
            }

            output.push(self.typename());
        }

        let span_end = self.tokens.peek_span().end;
        self.greater_than();

        self.compiler.type_args.push(TypeArgs::new(output));
        self.create_node(
            AstNode::TypeArgs(TypeArgsId(self.compiler.type_args.len() - 1)),
            span_start,
            span_end,
        )
    }

    pub fn typename(&mut self) -> NodeId {
        let _span = span!();
        if let (Token::Bareword, span) = self.tokens.peek() {
            let span_start = span.start;
            let name = self.name();
            let name_text = self.compiler.get_span_contents(name);

            if name_text == b"record" && self.is_less_than() {
                let fields = self.signature_params(ParamsContext::Angles);
                let optional = self.match_token(Token::QuestionMark).is_some();
                let span_end = self.position();
                return self.create_node(
                    AstNode::RecordType { fields, optional },
                    span_start,
                    span_end,
                );
            }

            let args = if self.is_less_than() {
                Some(self.type_args())
            } else {
                None
            };

            let optional = self.match_token(Token::QuestionMark).is_some();
            let span_end = if let Some(args) = args {
                self.get_span_end(args)
            } else if optional {
                self.position()
            } else {
                span.end
            };

            self.create_node(
                AstNode::Type {
                    name,
                    args,
                    optional,
                },
                span_start,
                span_end,
            )
        } else {
            self.error("expect name")
        }
    }

    fn optional_type_annotation(&mut self) -> Option<NodeId> {
        if self.is_colon() {
            Some(self.type_annotation())
        } else {
            None
        }
    }

    fn type_annotation(&mut self) -> NodeId {
        self.colon();
        let ty = self.typename();
        if self.is_at() {
            self.tokens.advance();
            self.command_name(&[Token::Comma, Token::Equals, Token::RSquare, Token::Pipe]);
        }
        ty
    }

    pub fn in_out_type(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        let in_ty = self.typename();
        self.thin_arrow();
        let out_ty = self.typename();

        let span_end = self.position();
        self.create_node(AstNode::InOutType(in_ty, out_ty), span_start, span_end)
    }

    pub fn in_out_types(&mut self) -> NodeId {
        let _span = span!();
        self.colon();

        if self.is_lsquare() {
            let span_start = self.position();
            self.tokens.advance();

            let mut output = vec![];
            while self.has_tokens() && !self.is_rsquare() {
                if self.match_separator() {
                    continue;
                }

                output.push(self.in_out_type());
            }

            let span_end = self.tokens.peek_span().end;
            self.rsquare();

            self.compiler.in_out_types.push(InOutTypes::new(output));
            self.create_node(
                AstNode::InOutTypes(InOutTypesId(self.compiler.in_out_types.len() - 1)),
                span_start,
                span_end,
            )
        } else {
            let ty = self.in_out_type();
            let span = self.compiler.get_span(ty);
            self.compiler.in_out_types.push(InOutTypes::new(vec![ty]));
            self.create_node(
                AstNode::InOutTypes(InOutTypesId(self.compiler.in_out_types.len() - 1)),
                span.start,
                span.end,
            )
        }
    }

    pub fn def_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"def");
        let mut has_env_flag = false;
        let mut has_wrapped_flag = false;

        while self.check(Token::DashDash) {
            self.tokens.advance();
            match self.tokens.peek() {
                (Token::Bareword, span) => {
                    let flag_name = self.compiler.get_span_contents_manual(span.start, span.end);
                    if flag_name == b"env" {
                        if has_env_flag {
                            return self.error("duplicated --env flag");
                        }
                        has_env_flag = true;
                    } else if flag_name == b"wrapped" {
                        if has_wrapped_flag {
                            return self.error("duplicated --wrapped flag");
                        }
                        has_wrapped_flag = true;
                    } else {
                        return self.error("expect --env or --wrapped");
                    }
                    self.tokens.advance();
                }
                _ => return self.error("incomplete flag name"),
            }
        }

        let name = self.command_name(&[Token::LessThan, Token::LSquare]);
        let type_params = if self.is_less_than() {
            Some(self.type_params())
        } else {
            None
        };

        let params = self.signature_params(ParamsContext::Squares);
        let in_out_types = if self.is_colon() {
            Some(self.in_out_types())
        } else {
            None
        };
        let block = self.block(BlockContext::Curlies);

        let span_end = self.get_span_end(block);

        self.create_node(
            AstNode::Def {
                name,
                type_params,
                params,
                in_out_types,
                block,
                env: has_env_flag,
                wrapped: has_wrapped_flag,
            },
            span_start,
            span_end,
        )
    }

    pub fn extern_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"extern");
        let name = self.command_name(&[Token::LSquare]);
        let params = self.signature_params(ParamsContext::Squares);
        let span_end = self.position();

        self.create_node(AstNode::Extern { name, params }, span_start, span_end)
    }

    pub fn let_statement(&mut self) -> NodeId {
        self.let_like_statement(b"let", false)
    }

    pub fn mut_statement(&mut self) -> NodeId {
        self.let_like_statement(b"mut", true)
    }

    fn let_like_statement(&mut self, keyword: &[u8], is_mutable: bool) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(keyword);
        let variable_name = self.variable_decl();
        let ty = self.optional_type_annotation();

        self.equals();
        let initializer = self.pipeline_or_expression();
        let span_end = self.get_span_end(initializer);

        self.create_node(
            AstNode::Let {
                variable_name,
                ty,
                initializer,
                is_mutable,
            },
            span_start,
            span_end,
        )
    }

    pub fn const_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"const");
        let variable_name = self.variable_decl();
        let ty = self.optional_type_annotation();

        self.equals();
        let initializer = self.expression_with_bareword(BarewordContext::Call);
        let span_end = self.get_span_end(initializer);

        self.create_node(
            AstNode::Const {
                variable_name,
                ty,
                initializer,
            },
            span_start,
            span_end,
        )
    }

    pub fn alias_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"alias");
        let new_name = self.command_name(&[Token::Equals]);
        self.equals();
        let old_name = self.pipeline_or_expression();
        let span_end = self.get_span_end(old_name);
        self.create_node(AstNode::Alias { new_name, old_name }, span_start, span_end)
    }

    fn module_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"module");
        let name = self.command_name(&[Token::LCurly]);
        let block = if self.is_lcurly() {
            Some(self.block(BlockContext::Curlies))
        } else {
            None
        };
        let span_end =
            block.map_or_else(|| self.get_span_end(name), |block| self.get_span_end(block));
        self.create_node(AstNode::Module { name, block }, span_start, span_end)
    }

    fn use_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"use");
        let pattern = self.import_pattern();
        let span_end = self.get_span_end(pattern);
        self.create_node(AstNode::Use { pattern }, span_start, span_end)
    }

    fn source_statement(&mut self, env: bool) -> NodeId {
        let span_start = self.position();
        if env {
            self.keyword(b"source-env");
        } else {
            self.keyword(b"source");
        }
        let source = self.expression_with_bareword(BarewordContext::String);
        let span_end = self.get_span_end(source);
        self.create_node(AstNode::Source { source, env }, span_start, span_end)
    }

    fn export_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"export");
        self.skip_newlines();
        let declaration = if self.is_exportable_declaration_start() {
            self.declaration()
        } else {
            self.error("expected exportable declaration")
        };
        let span_end = self.get_span_end(declaration);
        self.create_node(AstNode::Export { declaration }, span_start, span_end)
    }

    fn export_env_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"export-env");
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block);
        self.create_node(AstNode::ExportEnv { block }, span_start, span_end)
    }

    fn hide_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"hide");
        let pattern = self.import_pattern();
        let span_end = self.get_span_end(pattern);
        self.create_node(AstNode::Hide { pattern }, span_start, span_end)
    }

    fn overlay_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"overlay");
        let action = self.collect_until_boundary("expected overlay action");
        let span_end = self.get_span_end(action);
        self.create_node(AstNode::Overlay { action }, span_start, span_end)
    }

    fn plugin_statement(&mut self) -> NodeId {
        let span_start = self.position();
        self.keyword(b"plugin");
        self.keyword(b"use");
        let source = self.expression_with_bareword(BarewordContext::String);
        let span_end = self.get_span_end(source);
        self.create_node(AstNode::PluginUse { source }, span_start, span_end)
    }

    fn import_pattern(&mut self) -> NodeId {
        self.collect_until_boundary("expected import pattern")
    }

    fn collect_until_boundary(&mut self, message: &str) -> NodeId {
        let span_start = self.position();
        let mut parts = vec![];

        while self.has_tokens() && !self.is_command_boundary() {
            let part = self.collect_boundary_atom(message);
            parts.push(part);
        }

        match parts.len() {
            0 => self.error(message),
            1 => parts[0],
            _ => {
                let span_end = self.get_span_end(*parts.last().expect("parts is not empty"));
                self.compiler.calls.push(Call::new(parts));
                self.create_node(
                    AstNode::Call(CallId(self.compiler.calls.len() - 1)),
                    span_start,
                    span_end,
                )
            }
        }
    }

    fn collect_boundary_atom(&mut self, message: &str) -> NodeId {
        if self.is_long_flag_start() || self.is_short_flag_start() {
            self.flag_argument()
        } else if self.is_spread() {
            self.spread()
        } else if self.is_asterisk() {
            let span = self.tokens.peek_span();
            self.advance_node(AstNode::Name, span)
        } else if self.is_name() {
            let node = self.name();
            self.compiler.ast_nodes[node.0] = AstNode::String;
            node
        } else if self.is_string() {
            self.string()
        } else if self.is_lsquare() {
            self.list_or_table()
        } else if self.is_expression_start() {
            self.expression_with_bareword(BarewordContext::String)
        } else {
            self.error(message)
        }
    }

    pub fn block(&mut self, context: BlockContext) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        let mut code_body = vec![];

        if context == BlockContext::Curlies {
            self.lcurly();
        }

        while self.has_tokens() {
            self.skip_terminators();

            if self.is_block_end(context) {
                if context == BlockContext::Curlies {
                    self.rcurly();
                }
                break;
            }

            let before = self.tokens.pos();
            let statement_start = self.position();
            let statement = self.declaration();
            let statement_end = self.get_span_end(statement);

            if self.is_semicolon() {
                self.tokens.advance();
                code_body.push(self.create_node(
                    AstNode::Statement(statement),
                    statement_start,
                    statement_end,
                ));
            } else {
                code_body.push(statement);
            }

            if self.tokens.pos() == before {
                self.error("expected statement");
                break;
            }
        }

        self.compiler.blocks.push(Block::new(code_body));
        let span_end = self.position();

        self.create_node(
            AstNode::Block(BlockId(self.compiler.blocks.len() - 1)),
            span_start,
            span_end,
        )
    }

    fn declaration(&mut self) -> NodeId {
        if self.is_keyword_sequence(b"export-env") {
            self.export_env_statement()
        } else if self.is_keyword(b"export") {
            self.export_statement()
        } else if self.is_keyword(b"def") {
            self.def_statement()
        } else if self.is_keyword(b"extern") {
            self.extern_statement()
        } else if self.is_keyword(b"alias") {
            self.alias_statement()
        } else if self.is_keyword(b"const") {
            self.const_statement()
        } else if self.is_keyword(b"module") {
            self.module_statement()
        } else if self.is_keyword(b"use") {
            self.use_statement()
        } else if self.is_keyword_sequence(b"source-env") {
            self.source_statement(true)
        } else if self.is_keyword(b"source") {
            self.source_statement(false)
        } else if self.is_keyword(b"hide") {
            self.hide_statement()
        } else if self.is_keyword(b"overlay") {
            self.overlay_statement()
        } else if self.is_keyword(b"plugin") {
            self.plugin_statement()
        } else {
            self.statement()
        }
    }

    fn statement(&mut self) -> NodeId {
        if self.is_keyword(b"let") {
            self.let_statement()
        } else if self.is_keyword(b"mut") {
            self.mut_statement()
        } else if self.is_keyword(b"while") {
            self.while_statement()
        } else if self.is_keyword(b"for") {
            self.for_statement()
        } else if self.is_keyword(b"loop") {
            self.loop_statement()
        } else if self.is_keyword(b"return") {
            self.return_statement()
        } else if self.is_keyword(b"continue") {
            self.continue_statement()
        } else if self.is_keyword(b"break") {
            self.break_statement()
        } else {
            self.pipeline_or_expression_or_assignment()
        }
    }

    pub fn while_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"while");
        let condition = self.expression();
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block);

        self.create_node(AstNode::While { condition, block }, span_start, span_end)
    }

    pub fn for_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"for");

        let variable = self.variable_decl();
        self.keyword(b"in");

        let range = self.expression_with_bareword(BarewordContext::String);
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block);

        self.create_node(
            AstNode::For {
                variable,
                range,
                block,
            },
            span_start,
            span_end,
        )
    }

    pub fn loop_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"loop");
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block);

        self.create_node(AstNode::Loop { block }, span_start, span_end)
    }

    pub fn return_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"return");

        let ret_val = if self.is_expression_start() {
            Some(self.expression())
        } else {
            None
        };
        let span_end = ret_val.map_or(span_start + b"return".len(), |expr| self.get_span_end(expr));

        self.create_node(AstNode::Return(ret_val), span_start, span_end)
    }

    pub fn continue_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"continue");
        let span_end = span_start + b"continue".len();

        self.create_node(AstNode::Continue, span_start, span_end)
    }

    pub fn break_statement(&mut self) -> NodeId {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"break");
        let span_end = span_start + b"break".len();

        self.create_node(AstNode::Break, span_start, span_end)
    }

    fn spread(&mut self) -> NodeId {
        let span_start = self.position();
        self.tokens.advance();
        let expr = if self.is_expression_start() {
            self.expression_with_bareword(BarewordContext::String)
        } else {
            self.error("expected expression after spread")
        };
        let span_end = self.get_span_end(expr);
        self.create_node(AstNode::Spread(expr), span_start, span_end)
    }

    fn flag_argument(&mut self) -> NodeId {
        let flag = self.flag_node();

        if self.is_equals() {
            self.equals();
            let value = if self.is_expression_start() {
                self.expression_with_bareword(BarewordContext::String)
            } else {
                self.error("expected flag value")
            };
            let (span_start, span_end) = self.spanning(flag, value);
            self.create_node(
                AstNode::NamedValue { name: flag, value },
                span_start,
                span_end,
            )
        } else {
            flag
        }
    }

    fn flag_node(&mut self) -> NodeId {
        let span_start = self.position();

        if self.check(Token::DashDash) {
            self.tokens.advance();
            if let (Token::Bareword, span) = self.tokens.peek() {
                self.tokens.advance();
                self.create_node(AstNode::FlagLong, span_start, span.end)
            } else {
                self.error("incomplete long flag name")
            }
        } else if self.check(Token::Dash) {
            self.tokens.advance();
            if let (Token::Bareword, span) = self.tokens.peek() {
                let flag_name = self.compiler.get_span_contents_manual(span.start, span.end);
                let node = if flag_name.len() > 1 {
                    AstNode::FlagShortGroup
                } else {
                    AstNode::FlagShort
                };
                self.tokens.advance();
                self.create_node(node, span_start, span.end)
            } else {
                self.error("incomplete short flag name")
            }
        } else {
            self.error("expected flag")
        }
    }

    fn file_redirection_operator(&mut self) -> NodeId {
        let (token, span) = self.tokens.peek();
        match token {
            Token::GreaterThan
            | Token::OutGreaterThan
            | Token::OutGreaterGreaterThan
            | Token::ErrGreaterThan
            | Token::ErrGreaterGreaterThan
            | Token::OutErrGreaterThan
            | Token::OutErrGreaterGreaterThan => {
                self.tokens.advance();
                if token == Token::GreaterThan
                    && self.check(Token::GreaterThan)
                    && self.tokens.peek_span().start == span.end
                {
                    let span_end = self.tokens.peek_span().end;
                    self.tokens.advance();
                    self.create_node(AstNode::Name, span.start, span_end)
                } else {
                    self.create_node(AstNode::Name, span.start, span.end)
                }
            }
            _ => self.error("expected file redirection"),
        }
    }

    pub fn keyword(&mut self, keyword: &[u8]) {
        let _span = span!();
        if self.match_keyword_span(keyword).is_none() {
            self.error(format!(
                "expected keyword: {}",
                String::from_utf8_lossy(keyword)
            ));
        }
    }

    pub fn is_operator(&self) -> bool {
        let (token, span) = self.tokens.peek();

        match token {
            Token::Plus
            | Token::PlusPlus
            | Token::Dash
            | Token::Asterisk
            | Token::ForwardSlash
            | Token::ForwardSlashForwardSlash
            | Token::LessThan
            | Token::LessThanEqual
            | Token::GreaterThan
            | Token::GreaterThanEqual
            | Token::EqualsEquals
            | Token::ExclamationEquals
            | Token::EqualsTilde
            | Token::ExclamationTilde
            | Token::AsteriskAsterisk
            | Token::Equals
            | Token::PlusEquals
            | Token::DashEquals
            | Token::AsteriskEquals
            | Token::ForwardSlashEquals
            | Token::PlusPlusEquals => true,
            Token::Bareword => matches!(
                self.compiler.get_span_contents_manual(span.start, span.end),
                b"mod"
                    | b"in"
                    | b"not-in"
                    | b"has"
                    | b"not-has"
                    | b"like"
                    | b"not-like"
                    | b"starts-with"
                    | b"not-starts-with"
                    | b"ends-with"
                    | b"not-ends-with"
                    | b"bit-or"
                    | b"bit-xor"
                    | b"bit-and"
                    | b"bit-shl"
                    | b"bit-shr"
                    | b"and"
                    | b"xor"
                    | b"or"
                    | b"not"
            ),
            _ => false,
        }
    }

    pub fn is_equals(&self) -> bool {
        self.tokens.peek_token() == Token::Equals
    }

    pub fn is_comma(&self) -> bool {
        self.tokens.peek_token() == Token::Comma
    }

    pub fn is_lcurly(&self) -> bool {
        self.tokens.peek_token() == Token::LCurly
    }

    pub fn is_rcurly(&self) -> bool {
        self.tokens.peek_token() == Token::RCurly
    }

    pub fn is_lparen(&self) -> bool {
        self.tokens.peek_token() == Token::LParen
    }

    pub fn is_rparen(&self) -> bool {
        self.tokens.peek_token() == Token::RParen
    }

    pub fn is_lsquare(&self) -> bool {
        self.tokens.peek_token() == Token::LSquare
    }

    pub fn is_rsquare(&self) -> bool {
        self.tokens.peek_token() == Token::RSquare
    }

    pub fn is_less_than(&self) -> bool {
        self.tokens.peek_token() == Token::LessThan
    }

    pub fn is_greater_than(&self) -> bool {
        self.tokens.peek_token() == Token::GreaterThan
    }

    pub fn is_pipe(&self) -> bool {
        self.tokens.peek_token() == Token::Pipe
    }

    fn is_pipe_pipe(&self) -> bool {
        self.tokens.peek_token() == Token::PipePipe
    }

    fn is_pipeline_pipe(&self) -> bool {
        matches!(
            self.tokens.peek_token(),
            Token::Pipe | Token::ErrGreaterThanPipe | Token::OutErrGreaterThanPipe
        )
    }

    pub fn is_dollar(&self) -> bool {
        self.tokens.peek_token() == Token::Dollar
    }

    pub fn is_comment(&self) -> bool {
        self.tokens.peek_token() == Token::Comment
    }

    pub fn is_question_mark(&self) -> bool {
        self.tokens.peek_token() == Token::QuestionMark
    }

    pub fn is_thin_arrow(&self) -> bool {
        self.tokens.peek_token() == Token::ThinArrow
    }

    pub fn is_thick_arrow(&self) -> bool {
        self.tokens.peek_token() == Token::ThickArrow
    }

    pub fn is_colon(&self) -> bool {
        self.tokens.peek_token() == Token::Colon
    }

    pub fn is_newline(&self) -> bool {
        self.tokens.peek_token() == Token::Newline
    }

    pub fn is_semicolon(&self) -> bool {
        self.tokens.peek_token() == Token::Semicolon
    }

    pub fn is_dot(&self) -> bool {
        self.tokens.peek_token() == Token::Dot
    }

    pub fn is_dotdot(&self) -> bool {
        self.tokens.peek_token() == Token::DotDot
    }

    pub fn is_coloncolon(&self) -> bool {
        self.tokens.peek_token() == Token::ColonColon
    }

    pub fn is_int(&self) -> bool {
        self.tokens.peek_token() == Token::Int
    }

    pub fn is_float(&self) -> bool {
        self.tokens.peek_token() == Token::Float
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self.tokens.peek_token(),
            Token::DoubleQuotedString
                | Token::SingleQuotedString
                | Token::RawString
                | Token::BacktickBareword
                | Token::Datetime
                | Token::DqStringInterpStart
                | Token::SqStringInterpStart
        )
    }

    pub fn is_keyword(&self, keyword: &[u8]) -> bool {
        if let (Token::Bareword, span) = self.tokens.peek() {
            self.compiler.get_span_contents_manual(span.start, span.end) == keyword
        } else {
            false
        }
    }

    fn is_keyword_sequence(&mut self, keyword: &[u8]) -> bool {
        let pos = self.tokens.pos();
        let matched = self.match_keyword_span(keyword).is_some();
        self.tokens.set_pos(pos);
        matched
    }

    fn match_keyword_span(&mut self, keyword: &[u8]) -> Option<Span> {
        let pos = self.tokens.pos();
        let mut span_start = None;
        let mut span_end = 0;

        for (index, part) in keyword.split(|byte| *byte == b'-').enumerate() {
            if part.is_empty() {
                self.tokens.set_pos(pos);
                return None;
            }

            if index > 0 {
                let (token, span) = self.tokens.peek();
                if token != Token::Dash || span.start != span_end {
                    self.tokens.set_pos(pos);
                    return None;
                }

                self.tokens.advance();
                span_end = span.end;
            }

            let (token, span) = self.tokens.peek();
            if token != Token::Bareword
                || (index > 0 && span.start != span_end)
                || self.compiler.get_span_contents_manual(span.start, span.end) != part
            {
                self.tokens.set_pos(pos);
                return None;
            }

            if span_start.is_none() {
                span_start = Some(span.start);
            }

            self.tokens.advance();
            span_end = span.end;
        }

        span_start.map(|start| Span::new(start, span_end))
    }

    pub fn is_name(&self) -> bool {
        self.tokens.peek_token() == Token::Bareword
    }

    pub fn is_eof(&self) -> bool {
        self.tokens.peek_token() == Token::Eof
    }

    fn is_asterisk(&self) -> bool {
        self.tokens.peek_token() == Token::Asterisk
    }

    fn is_at(&self) -> bool {
        self.tokens.peek_token() == Token::At
    }

    fn is_spread(&self) -> bool {
        self.tokens.peek_token() == Token::DotDotDot
    }

    pub fn is_horizontal_space(&self) -> bool {
        self.has_horizontal_space_before(self.tokens.peek_span().start)
    }

    fn has_horizontal_space_before(&self, span_position: usize) -> bool {
        let whitespace: &[u8] = b" \t";

        span_position > 0 && whitespace.contains(&self.compiler.source[span_position - 1])
    }

    pub fn is_expression(&self) -> bool {
        self.is_expression_start()
    }

    pub fn is_simple_expression(&self) -> bool {
        self.is_expression_start()
    }

    fn is_expression_start(&self) -> bool {
        match self.tokens.peek_token() {
            Token::DoubleQuotedString
            | Token::SingleQuotedString
            | Token::RawString
            | Token::BacktickBareword
            | Token::Datetime
            | Token::DqStringInterpStart
            | Token::SqStringInterpStart
            | Token::Int
            | Token::Float
            | Token::LCurly
            | Token::LSquare
            | Token::LParen
            | Token::Dot
            | Token::DotDot
            | Token::Dollar
            | Token::Plus
            | Token::Dash => true,
            Token::Bareword => !self.is_statement_only_keyword(),
            _ => false,
        }
    }

    fn is_statement_only_keyword(&self) -> bool {
        [
            b"let".as_slice(),
            b"mut".as_slice(),
            b"const".as_slice(),
            b"def".as_slice(),
            b"extern".as_slice(),
            b"alias".as_slice(),
            b"module".as_slice(),
            b"use".as_slice(),
            b"source".as_slice(),
            b"source-env".as_slice(),
            b"export".as_slice(),
            b"export-env".as_slice(),
            b"hide".as_slice(),
            b"overlay".as_slice(),
            b"plugin".as_slice(),
            b"for".as_slice(),
            b"while".as_slice(),
            b"loop".as_slice(),
            b"return".as_slice(),
            b"break".as_slice(),
            b"continue".as_slice(),
            b"else".as_slice(),
            b"catch".as_slice(),
            b"finally".as_slice(),
        ]
        .iter()
        .any(|keyword| self.is_keyword(keyword))
    }

    fn is_command_start(&self) -> bool {
        match self.tokens.peek_token() {
            Token::Caret => true,
            Token::Bareword => !self.is_reserved_expression_word(),
            _ => false,
        }
    }

    fn is_external_call_name(&self, node_id: NodeId) -> bool {
        let name = self.compiler.get_span_contents(node_id);
        name.starts_with(b"^") || name == b"run-external"
    }

    fn is_external_expression_argument_start(&self) -> bool {
        matches!(
            self.tokens.peek_token(),
            Token::DoubleQuotedString
                | Token::SingleQuotedString
                | Token::RawString
                | Token::BacktickBareword
                | Token::Datetime
                | Token::DqStringInterpStart
                | Token::SqStringInterpStart
                | Token::Dollar
                | Token::LCurly
                | Token::LSquare
                | Token::LParen
        )
    }

    fn is_unquoted_word_argument_with_continuation(&mut self) -> bool {
        if !matches!(
            self.tokens.peek_token(),
            Token::Bareword | Token::Int | Token::Float
        ) {
            return false;
        }

        self.peek_next_token().is_some_and(|(token, span)| {
            span.start == self.tokens.peek_span().end && is_external_word_token(token)
        })
    }

    fn is_reserved_expression_word(&self) -> bool {
        [
            b"if".as_slice(),
            b"try".as_slice(),
            b"match".as_slice(),
            b"true".as_slice(),
            b"false".as_slice(),
            b"null".as_slice(),
            b"not".as_slice(),
        ]
        .iter()
        .any(|keyword| self.is_keyword(keyword))
            || self.is_statement_only_keyword()
    }

    fn is_assignment_operator(&self) -> bool {
        matches!(
            self.tokens.peek_token(),
            Token::Equals
                | Token::PlusEquals
                | Token::DashEquals
                | Token::AsteriskEquals
                | Token::ForwardSlashEquals
                | Token::PlusPlusEquals
        )
    }

    fn is_environment_assignment(&mut self) -> bool {
        if !self.is_name() {
            return false;
        }

        let (_, name_span) = self.tokens.peek();
        let Some((Token::Equals, equals_span)) = self.peek_next_token() else {
            return false;
        };

        name_span.end == equals_span.start
    }

    fn is_file_redirection(&self) -> bool {
        matches!(
            self.tokens.peek_token(),
            Token::OutGreaterThan
                | Token::OutGreaterGreaterThan
                | Token::ErrGreaterThan
                | Token::ErrGreaterGreaterThan
                | Token::OutErrGreaterThan
                | Token::OutErrGreaterGreaterThan
        ) || self.tokens.peek_token() == Token::GreaterThan
    }

    fn is_long_flag_start(&mut self) -> bool {
        if !self.check(Token::DashDash) {
            return false;
        }

        self.peek_next_token().is_some_and(|(token, span)| {
            token == Token::Bareword && span.start == self.tokens.peek_span().end
        })
    }

    fn is_short_flag_start(&mut self) -> bool {
        if !self.check(Token::Dash) {
            return false;
        }

        self.peek_next_token().is_some_and(|(token, span)| {
            token == Token::Bareword && span.start == self.tokens.peek_span().end
        })
    }

    fn is_command_boundary(&self) -> bool {
        self.is_command_boundary_token(self.tokens.peek_token()) || self.is_file_redirection()
    }

    fn is_command_boundary_token(&self, token: Token) -> bool {
        matches!(
            token,
            Token::Eof
                | Token::Newline
                | Token::Comment
                | Token::Semicolon
                | Token::Comma
                | Token::Pipe
                | Token::ErrGreaterThanPipe
                | Token::OutErrGreaterThanPipe
                | Token::RCurly
                | Token::RSquare
                | Token::RParen
                | Token::StrInterpRParen
                | Token::ThickArrow
        )
    }

    fn is_call_name_boundary_token(&self, token: Token) -> bool {
        matches!(
            token,
            Token::Eof
                | Token::Newline
                | Token::Comment
                | Token::Semicolon
                | Token::Comma
                | Token::Pipe
                | Token::ErrGreaterThanPipe
                | Token::OutErrGreaterThanPipe
                | Token::LCurly
                | Token::RCurly
                | Token::LSquare
                | Token::RSquare
                | Token::LParen
                | Token::RParen
                | Token::StrInterpRParen
                | Token::ThickArrow
        )
    }

    fn is_block_end(&self, context: BlockContext) -> bool {
        match context {
            BlockContext::Bare => self.is_eof(),
            BlockContext::Curlies => self.is_rcurly() || self.is_eof(),
            BlockContext::Closure => self.is_rcurly() || self.is_eof(),
        }
    }

    fn is_params_end(&self, params_context: ParamsContext) -> bool {
        match params_context {
            ParamsContext::Pipes => self.is_pipe(),
            ParamsContext::Squares => self.is_rsquare(),
            ParamsContext::Angles => self.is_greater_than(),
        }
    }

    fn is_exportable_declaration_start(&self) -> bool {
        self.is_keyword(b"def")
            || self.is_keyword(b"extern")
            || self.is_keyword(b"alias")
            || self.is_keyword(b"const")
            || self.is_keyword(b"module")
            || self.is_keyword(b"use")
    }

    fn check(&self, token: Token) -> bool {
        !self.is_eof() && self.tokens.peek_token() == token
    }

    fn check_any(&self, tokens: &[Token]) -> bool {
        tokens.iter().any(|token| self.check(*token))
    }

    fn match_token(&mut self, token: Token) -> Option<Span> {
        if self.check(token) {
            let span = self.tokens.peek_span();
            self.tokens.advance();
            Some(span)
        } else {
            None
        }
    }

    fn consume(&mut self, token: Token, message: &str) -> Option<Span> {
        if self.check(token) {
            let span = self.tokens.peek_span();
            self.tokens.advance();
            Some(span)
        } else {
            self.error(message);
            None
        }
    }

    fn match_separator(&mut self) -> bool {
        if self.is_comma() || self.is_newline() || self.is_semicolon() || self.is_comment() {
            self.tokens.advance();
            true
        } else {
            false
        }
    }

    fn skip_separators(&mut self) {
        while self.match_separator() {}
    }

    fn skip_list_item_separators(&mut self) {
        while self.is_comma() || self.is_newline() || self.is_comment() {
            self.tokens.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while self.is_newline() || self.is_semicolon() || self.is_comment() {
            self.tokens.advance();
        }
    }

    pub fn skip_newlines(&mut self) {
        while self.is_newline() || self.is_comment() {
            self.tokens.advance();
        }
    }

    fn peek_next_token(&mut self) -> Option<(Token, Span)> {
        if self.is_eof() {
            return None;
        }

        let pos = self.tokens.pos();
        self.tokens.advance();
        let next = self.tokens.peek();
        self.tokens.set_pos(pos);
        Some(next)
    }

    fn peek_after_lcurly_is_pipe(&mut self) -> bool {
        if !self.is_lcurly() {
            return false;
        }

        self.peek_next_token()
            .is_some_and(|(token, _)| token == Token::Pipe || token == Token::PipePipe)
    }

    pub fn error_on_node(&mut self, message: impl Into<String>, node_id: NodeId) {
        self.compiler.errors.push(SourceError {
            message: message.into(),
            node_id,
            severity: Severity::Error,
        });
    }

    pub fn error(&mut self, message: impl Into<String>) -> NodeId {
        let (token, span) = self.tokens.peek();

        if token != Token::Eof {
            self.tokens.advance();
        }

        let node_id = self.create_node(AstNode::Garbage, span.start, span.end);
        self.compiler.errors.push(SourceError {
            message: message.into(),
            node_id,
            severity: Severity::Error,
        });

        node_id
    }

    pub fn create_node(&mut self, ast_node: AstNode, span_start: usize, span_end: usize) -> NodeId {
        self.compiler.spans.push(Span {
            start: span_start,
            end: span_end,
        });
        self.compiler.push_node(ast_node)
    }

    pub fn lparen(&mut self) {
        self.consume(Token::LParen, "expected: left paren '('");
    }

    pub fn rparen(&mut self) {
        self.consume(Token::RParen, "expected: right paren ')'");
    }

    pub fn lsquare(&mut self) {
        self.consume(Token::LSquare, "expected: left bracket '['");
    }

    pub fn rsquare(&mut self) {
        self.consume(Token::RSquare, "expected: right bracket ']'");
    }

    pub fn lcurly(&mut self) {
        self.consume(Token::LCurly, "expected: left bracket '{'");
    }

    pub fn rcurly(&mut self) {
        self.consume(Token::RCurly, "expected: right bracket '}'");
    }

    pub fn pipe(&mut self) {
        self.consume(Token::Pipe, "expected: pipe symbol '|'");
    }

    fn pipeline_pipe(&mut self) {
        if self.is_pipeline_pipe() {
            self.tokens.advance();
        } else {
            self.error("expected: pipe symbol '|'");
        }
    }

    pub fn less_than(&mut self) {
        self.consume(
            Token::LessThan,
            "expected: less than/left angle bracket '<'",
        );
    }

    pub fn greater_than(&mut self) {
        self.consume(
            Token::GreaterThan,
            "expected: greater than/right angle bracket '>'",
        );
    }

    pub fn equals(&mut self) {
        self.consume(Token::Equals, "expected: equals '='");
    }

    pub fn thin_arrow(&mut self) {
        self.consume(Token::ThinArrow, "expected: thin arrow '->'");
    }

    pub fn colon(&mut self) {
        self.consume(Token::Colon, "expected: colon ':'");
    }

    pub fn comma(&mut self) {
        self.consume(Token::Comma, "expected: comma ','");
    }

    fn get_rollback_point(&self) -> RollbackPoint {
        self.compiler.get_rollback_point(self.tokens.pos())
    }

    fn apply_rollback(&mut self, rbp: RollbackPoint) {
        let token_pos = self.compiler.apply_compiler_rollback(rbp);
        self.tokens.set_pos(token_pos);
    }
}

fn is_external_word_token(token: Token) -> bool {
    !matches!(
        token,
        Token::Eof
            | Token::Newline
            | Token::Comment
            | Token::Semicolon
            | Token::Comma
            | Token::Pipe
            | Token::ErrGreaterThanPipe
            | Token::OutErrGreaterThanPipe
            | Token::LCurly
            | Token::RCurly
            | Token::LSquare
            | Token::RSquare
            | Token::LParen
            | Token::RParen
            | Token::StrInterpRParen
            | Token::ThickArrow
            | Token::DoubleQuotedString
            | Token::SingleQuotedString
            | Token::RawString
            | Token::BacktickBareword
            | Token::Datetime
            | Token::DqStrInterp
            | Token::SqStrInterp
            | Token::DqStringInterpStart
            | Token::SqStringInterpStart
            | Token::StrInterpChunk
            | Token::StrInterpLParen
            | Token::StrInterpEnd
            | Token::Dollar
    )
}
