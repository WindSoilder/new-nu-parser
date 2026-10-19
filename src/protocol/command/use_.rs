use super::prelude::*;
use crate::typechecker::Type;
use nu_protocol::{
    ast::{Expr, Expression},
    engine::CommandType,
    shell_error::generic::GenericError,
};

#[derive(Clone)]
pub struct Use;

impl Command for Use {
    fn name(&self) -> &str {
        "use"
    }

    fn description(&self) -> &str {
        "Use definitions from a module, making them available in your shell."
    }

    fn signature(&self) -> Signature {
        Signature::build("use")
            .input_output_types(vec![(Type::None, Type::None)])
            .allow_variants_without_examples(true)
            // FIXME: let's ignore Type::OneOf for now.
            // .param(Parameter::Required(
            //     PositionalArg::new("module", Type::OneOf(vec![Type::String, Type::None]))
            //         .desc("Module or module file (`null` for no-op).")
            //         .completion(Completion::Builtin(BuiltinCompletion::NuFile {
            //             std_virtual_path: true,
            //         })),
            // ))
            .param(Parameter::Rest(
                PositionalArg::new("members", Type::Any)
                    .desc("Which members of the module to import.")
                    .completion(Completion::Builtin(BuiltinCompletion::ModuleExports)),
            ))
            .category(Category::Core)
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["module", "import", "include", "scope"]
    }

    fn extra_description(&self) -> &str {
        "See `help std` for the standard library module.
See `help modules` to list all available modules.

This command is a parser keyword. For details, check:
  https://www.nushell.sh/book/thinking_in_nu.html"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Keyword
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _caller_stack: &mut Stack,
        _call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        unimplemented!(); 
    }

    fn examples(&self) -> Vec<Example<'_>> {
        vec![
            Example {
                description: "Define a custom command in a module and call it.",
                example: r#"module spam { export def foo [] { "foo" } }; use spam foo; foo"#,
                result: Some(Value::test_string("foo")),
            },
            Example {
                description: "Define a custom command that participates in the environment in a module and call it.",
                example: r#"module foo { export def --env bar [] { $env.FOO_BAR = "BAZ" } }; use foo bar; bar; $env.FOO_BAR"#,
                result: Some(Value::test_string("BAZ")),
            },
            Example {
                description: "Use a plain module name to import its definitions qualified by the module name.",
                example: r#"module spam { export def foo [] { "foo" }; export def bar [] { "bar" } }; use spam; (spam foo) + (spam bar)"#,
                result: Some(Value::test_string("foobar")),
            },
            Example {
                description: "Specify * to use all definitions in a module.",
                example: r#"module spam { export def foo [] { "foo" }; export def bar [] { "bar" } }; use spam *; (foo) + (bar)"#,
                result: Some(Value::test_string("foobar")),
            },
            Example {
                description: "To use commands with spaces, like subcommands, surround them with quotes.",
                example: r#"module spam { export def 'foo bar' [] { "baz" } }; use spam 'foo bar'; foo bar"#,
                result: Some(Value::test_string("baz")),
            },
            Example {
                description: "To use multiple definitions from a module, wrap them in a list.",
                example: r#"module spam { export def foo [] { "foo" }; export def 'foo bar' [] { "baz" } }; use spam ['foo', 'foo bar']; (foo) + (foo bar)"#,
                result: Some(Value::test_string("foobaz")),
            },
        ]
    }
}
