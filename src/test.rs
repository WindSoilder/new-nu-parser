use crate::ir_generator::IrGenerator;
use crate::keyword_commands_prototype::BoundFlag;
use crate::lexer::lex;
use crate::parser::AstNode;
use crate::resolver::Resolver;
use crate::typechecker::Typechecker;
use crate::{compiler::Compiler, parser::Parser};

use std::path::Path;

fn evaluate_example(fname: &Path) -> String {
    let mut compiler = Compiler::new();
    let contents = std::fs::read_to_string(fname).expect("We only run tests found by glob");
    // normalize newlines
    let replaced = contents.replace("\r\n", "\n");
    let contents = replaced.as_bytes();

    let span_offset = compiler.span_offset();
    compiler.add_file(&fname.to_string_lossy(), contents);

    let (tokens, err) = lex(contents, span_offset);
    if let Err(e) = err {
        tokens.eprint(contents);
        eprintln!("Lexing error. Error: {:?}", e);
        std::process::exit(1);
    }

    let parser = Parser::new(compiler, tokens);
    compiler = parser.parse();

    let mut result = compiler.display_state();

    if !compiler.errors.is_empty() {
        return result;
    }

    let mut resolver = Resolver::new(&compiler);
    resolver.resolve();
    result.push_str(&resolver.display_state());

    compiler.merge_name_bindings(resolver.to_name_bindings());

    if !compiler.errors.is_empty() {
        return result;
    }

    let mut typechecker = Typechecker::new(&compiler);
    typechecker.typecheck();
    result.push_str(&typechecker.display_state());

    compiler.merge_types(typechecker.to_types());

    let mut ir_generator = IrGenerator::new(&compiler);
    ir_generator.generate();
    result.push_str(&ir_generator.display_state());

    result
}

fn evaluate_lexer(fname: &Path) -> String {
    let contents = std::fs::read_to_string(fname).expect("We only run tests found by glob");
    // normalize newlines
    let replaced = contents.replace("\r\n", "\n");
    let contents = replaced.as_bytes();

    let (tokens, err) = lex(contents, 0);
    let mut res = tokens.display(contents);

    if let Err(e) = err {
        res.push_str(&format!("Lexing error. Error: {:?}", e));
    }

    res
}

#[test]
fn test_node_output() {
    insta::glob!("../tests", "*.nu", |path| {
        insta::assert_snapshot!(evaluate_example(path));
    });
}

#[test]
fn test_lexer() {
    insta::glob!("../tests/lex", "*.nu", |path| {
        insta::assert_snapshot!(evaluate_lexer(path));
    });
}

#[test]
fn test_overlay_new_keyword_arguments_are_bound_in_resolver() {
    let mut compiler = Compiler::new();
    let contents = b"overlay new --reload spam\n";
    compiler.add_file("test", contents);

    let (tokens, err) = lex(contents, 0);
    assert!(err.is_ok());

    let parser = Parser::new(compiler, tokens);
    compiler = parser.parse();
    assert!(compiler.errors.is_empty());

    let mut resolver = Resolver::new(&compiler);
    resolver.resolve();
    assert!(resolver.errors.is_empty());

    let overlay_new = compiler
        .ast_nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            matches!(node, AstNode::OverlayNew { .. }).then_some(crate::parser::NodeId(idx))
        })
        .expect("expected overlay new node");

    let bound_arguments = resolver
        .signature_argument_bindings
        .get(&overlay_new)
        .expect("expected resolved overlay new arguments");

    assert!(matches!(
        bound_arguments.flags[0],
        Some(BoundFlag::Switch { .. })
    ));

    let name = bound_arguments.positionals[0].expect("expected overlay name");
    assert_eq!(compiler.get_span_contents(name), b"spam");
}

#[test]
fn test_plugin_use_keyword_binds_value_taking_flag() {
    let mut compiler = Compiler::new();
    let contents = b"plugin use --plugin-config config spam\n";
    compiler.add_file("test", contents);

    let (tokens, err) = lex(contents, 0);
    assert!(err.is_ok());

    let parser = Parser::new(compiler, tokens);
    compiler = parser.parse();
    assert!(compiler.errors.is_empty());

    let mut resolver = Resolver::new(&compiler);
    resolver.resolve();
    assert!(resolver.errors.is_empty());

    let plugin_use = compiler
        .ast_nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            matches!(node, AstNode::PluginUse { .. }).then_some(crate::parser::NodeId(idx))
        })
        .expect("expected plugin use node");

    let bound_arguments = resolver
        .signature_argument_bindings
        .get(&plugin_use)
        .expect("expected resolved plugin use arguments");

    let Some(BoundFlag::Value { value, .. }) = bound_arguments.flags[0] else {
        panic!("expected plugin-config flag value");
    };
    assert_eq!(compiler.get_span_contents(value), b"config");

    let name = bound_arguments.positionals[0].expect("expected plugin use name");
    assert_eq!(compiler.get_span_contents(name), b"spam");
}

#[test]
fn test_plugin_use_keyword_binds_equals_flag_value() {
    let mut compiler = Compiler::new();
    let contents = b"plugin use --plugin-config=config spam\n";
    compiler.add_file("test", contents);

    let (tokens, err) = lex(contents, 0);
    assert!(err.is_ok());

    let parser = Parser::new(compiler, tokens);
    compiler = parser.parse();
    assert!(compiler.errors.is_empty());

    let mut resolver = Resolver::new(&compiler);
    resolver.resolve();
    assert!(resolver.errors.is_empty());

    let plugin_use = compiler
        .ast_nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            matches!(node, AstNode::PluginUse { .. }).then_some(crate::parser::NodeId(idx))
        })
        .expect("expected plugin use node");

    let bound_arguments = resolver
        .signature_argument_bindings
        .get(&plugin_use)
        .expect("expected resolved plugin use arguments");

    let Some(BoundFlag::Value { value, .. }) = bound_arguments.flags[0] else {
        panic!("expected plugin-config flag value");
    };
    assert_eq!(compiler.get_span_contents(value), b"config");

    let name = bound_arguments.positionals[0].expect("expected plugin use name");
    assert_eq!(compiler.get_span_contents(name), b"spam");
}
