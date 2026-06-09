use std::collections::HashMap;

use crate::{compiler::Span, parser::NodeId};

pub struct Flag {
    long: Vec<u8>,
    short: Option<char>,
}

pub struct Positional {
    name: Vec<u8>,
    optional: bool,
}

pub struct CommandSignature {
    flags: Vec<Flag>,
    positionals: Vec<Positional>,
}

impl CommandSignature {
    pub fn get_long_flag(&self, name: &[u8]) -> Option<&Flag> {
        self.flags.iter().find(|flag| flag.long == name)
    }

    pub fn get_long_name_from_long(&self, name: &[u8]) -> Option<&[u8]> {
        self.get_long_flag(name).map(|flag| flag.long.as_slice())
    }

    pub fn get_long_name_from_short(&self, short: char) -> Option<&[u8]> {
        self.get_short_flag(short).map(|flag| flag.long.as_slice())
    }

    pub fn get_short_flag(&self, short: char) -> Option<&Flag> {
        self.flags.iter().find(|flag| flag.short == Some(short))
    }
}

struct OverlayNew;

trait ParserCommand {
    fn signature(&self) -> CommandSignature;
}

impl ParserCommand for OverlayNew {
    fn signature(&self) -> CommandSignature {
        CommandSignature {
            flags: vec![Flag {
                long: b"reload".to_vec(),
                short: Some('r'),
            }],
            positionals: vec![Positional {
                name: b"overlay_name".to_vec(),
                optional: false,
            }],
        }
    }
}

// --------- Argument
pub enum Argument {
    Flag(NodeId),
    Positional(NodeId),
}

pub struct Arguments {
    inner: HashMap<Vec<u8>, Argument>,
    span: Span,
}

impl Arguments {
    pub fn new() -> Self {
        Arguments {
            inner: Vec::new(),
            span: Span::new(0, 0),
        }
    }

    pub fn set_span(&mut self, start: usize, end: usize) {
        self.span = Span::new(start, end);
    }

    pub fn push(&mut self, arg: Argument) {
        self.inner.push(arg);
    }
}
