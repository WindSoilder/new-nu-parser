use crate::parser::AstNode;
use crate::{
    id::NodeId,
    resolver::Resolver,
};

#[derive(Clone)]
pub enum DeclarationInfo {
    Custom(CustomInfo),
    Default,
}

#[derive(PartialEq, Eq, Clone)]
pub struct Flag {
    pub param: NodeId,
    pub long: Vec<u8>,
    pub short: Option<char>,
}

#[derive(PartialEq, Eq, Clone)]
pub struct SwitchFlag {
    pub param: NodeId,
    pub long: Vec<u8>,
    pub short: Option<char>,
}

#[derive(Clone)]
pub struct CustomInfo {
    pub required: Vec<NodeId>,
    pub optional: Vec<NodeId>,
    pub flags: Vec<Flag>,
    pub switch_flags: Vec<SwitchFlag>,
    pub rest_param: Option<NodeId>,
}

#[derive(Clone)]
pub struct Declaration {
    pub name: String,
    pub info: DeclarationInfo,
}

impl Declaration {
    pub fn new(name: String) -> Self {
        Self { name, info: DeclarationInfo::Default }
    }

    pub fn new_custom(name: String, params_id: NodeId, resolver: &mut Resolver) -> Self {
        let mut flags = vec![];
        let mut switch_flags = vec![];
        let mut rest_param = None;
        let mut required = vec![];
        let mut optional = vec![];
        let params = resolver.compiler.get_params(params_id);
        let mut meet_optional = false;

        for one_param in &params.nodes {
            match resolver.compiler.ast_nodes[one_param.get()] {
                AstNode::PosParam { is_optional, .. } => {
                    if is_optional {
                        meet_optional = true;
                        optional.push(*one_param);
                    } else {
                        if meet_optional {
                            resolver.error(
                                "can not define require parameter after optional parameters",
                                *one_param,
                            )
                        } else {
                            required.push(*one_param);
                        }
                    }
                }
                AstNode::FlagParam { long, short, .. } => {
                    let AstNode::FlagLong(long) = resolver.compiler.ast_nodes[long.get()] else {
                        panic!("internal error: flag's long name is not a long flag");
                    };
                    let long_name = resolver.compiler.get_span_contents(long).to_vec();
                    let short_name: Option<char> = short.map(|x| {
                        let AstNode::FlagShort(x) = resolver.compiler.ast_nodes[x.get()] else {
                            panic!("internal error: flag's short name is not a short flag");
                        };
                        std::str::from_utf8(resolver.compiler.get_span_contents(x))
                            .expect("the contents is a valid utf8 string")
                            .chars()
                            .next()
                            .expect("the contents is not empty")
                    });
                    flags.push(Flag {
                        param: *one_param,
                        long: long_name,
                        short: short_name,
                    });
                }
                AstNode::RestParam { .. } => {
                    if rest_param.is_none() {
                        rest_param = Some(*one_param);
                    } else {
                        resolver.error("multiple rest params are not allowed", *one_param);
                    }
                }
                AstNode::SwitchFlagParam { long, short } => {
                    let AstNode::FlagLong(long) = resolver.compiler.ast_nodes[long.get()] else {
                        panic!("internal error: switch flag's long name is not a long flag");
                    };
                    let long_name = resolver.compiler.get_span_contents(long).to_vec();
                    let short_name: Option<char> = short.map(|x| {
                        let AstNode::FlagShort(x) = resolver.compiler.ast_nodes[x.get()] else {
                            panic!("internal error: switch flag's short name is not a short flag");
                        };
                        std::str::from_utf8(resolver.compiler.get_span_contents(x))
                            .expect("the contents is a valid utf8 string")
                            .chars()
                            .next()
                            .expect("the contents is not empty")
                    });
                    switch_flags.push(SwitchFlag {
                        param: *one_param,
                        long: long_name,
                        short: short_name,
                    });
                }
                _ => panic!("internal error: {} is not a param", one_param.get()),
            }
        }

        let custom_info = CustomInfo {
            required,
            optional,
            flags,
            switch_flags,
            rest_param,
        };
        Self {
            name,
            info: DeclarationInfo::Custom(custom_info),
        }
    }

    pub fn is_custom_decl(&self) -> bool {
        matches!(self.info, DeclarationInfo::Custom(_))
    }
}

// Question: should `Declaration` implement `Command` trait?
// The original code has implemented it, but I added some methods into `Command` trait, so I have to
// comment it out.
//
// impl Command for Declaration {
//     fn name(&self) -> &str {
//         &self.name
//     }
// }
