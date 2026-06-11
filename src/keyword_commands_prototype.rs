use crate::parser::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagArg {
    Switch,
    RequiredValue { name: &'static [u8] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    pub long: &'static [u8],
    pub short: Option<u8>,
    pub arg: FlagArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Positional {
    pub name: &'static [u8],
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSignature {
    pub flags: &'static [Flag],
    pub positionals: &'static [Positional],
}

impl CommandSignature {
    pub fn find_long_flag(&self, name: &[u8]) -> Option<(usize, &Flag)> {
        self.flags
            .iter()
            .enumerate()
            .find(|(_, flag)| flag.long == name)
    }

    pub fn find_short_flag(&self, short: u8) -> Option<(usize, &Flag)> {
        self.flags
            .iter()
            .enumerate()
            .find(|(_, flag)| flag.short == Some(short))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundFlag {
    Switch { flag: NodeId },
    Value { flag: NodeId, value: NodeId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundArguments {
    pub flags: Vec<Option<BoundFlag>>,
    pub positionals: Vec<Option<NodeId>>,
}

impl BoundArguments {
    pub fn new(signature: &CommandSignature) -> Self {
        Self {
            flags: vec![None; signature.flags.len()],
            positionals: vec![None; signature.positionals.len()],
        }
    }
}

const OVERLAY_NEW_FLAGS: &[Flag] = &[Flag {
    long: b"reload",
    short: Some(b'r'),
    arg: FlagArg::Switch,
}];

const OVERLAY_NEW_POSITIONALS: &[Positional] = &[Positional {
    name: b"overlay_name",
    optional: false,
}];

const OVERLAY_NEW_SIGNATURE: CommandSignature = CommandSignature {
    flags: OVERLAY_NEW_FLAGS,
    positionals: OVERLAY_NEW_POSITIONALS,
};

pub struct OverlayNew;

impl OverlayNew {
    pub fn signature() -> &'static CommandSignature {
        &OVERLAY_NEW_SIGNATURE
    }
}

const PLUGIN_USE_FLAGS: &[Flag] = &[Flag {
    long: b"plugin-config",
    short: None,
    arg: FlagArg::RequiredValue {
        name: b"plugin-config",
    },
}];

const PLUGIN_USE_POSITIONALS: &[Positional] = &[Positional {
    name: b"name",
    optional: false,
}];

const PLUGIN_USE_SIGNATURE: CommandSignature = CommandSignature {
    flags: PLUGIN_USE_FLAGS,
    positionals: PLUGIN_USE_POSITIONALS,
};

pub struct PluginUse;

impl PluginUse {
    pub fn signature() -> &'static CommandSignature {
        &PLUGIN_USE_SIGNATURE
    }
}
