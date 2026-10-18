pub use nu_protocol::{
    BlockId, DeclId, FileId, Id, JobId, ModuleId, OverlayId, RegId, SpanId, VarId, VirtualPathId,
};

pub mod marker {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Node;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Params;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InOutTypes;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Call;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Attribute;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct List;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Table;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Record;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Match;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TypeArgs;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Pipeline;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct StringInterp;
}

/// Identifies a bottom-level AST node stored in `Compiler::ast_nodes`.
pub type NodeId = Id<marker::Node>;
pub type ParamsId = Id<marker::Params>;
pub type InOutTypesId = Id<marker::InOutTypes>;
pub type CallId = Id<marker::Call>;
pub type AttributeId = Id<marker::Attribute>;
pub type ListId = Id<marker::List>;
pub type TableId = Id<marker::Table>;
pub type RecordId = Id<marker::Record>;
pub type MatchId = Id<marker::Match>;
pub type TypeArgsId = Id<marker::TypeArgs>;
pub type PipelineId = Id<marker::Pipeline>;
pub type StringInterpId = Id<marker::StringInterp>;
