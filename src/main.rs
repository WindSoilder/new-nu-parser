use ariadne::{Label, Report, ReportKind, Source};
use new_nu_parser::errors::SourceError;
use std::process::exit;

use new_nu_parser::compiler::{Compiler, Spanned};
use new_nu_parser::ir_generator::IrGenerator;
use new_nu_parser::lexer::{LexError, lex};
use new_nu_parser::parser::Parser;
use new_nu_parser::resolver::Resolver;
use new_nu_parser::typechecker::Typechecker;

fn main() {
    let mut compiler = Compiler::new();
    let mut do_print = true;

    for arg in std::env::args().skip(1) {
        if arg == "--no-print" {
            do_print = false;
        }
    }

    for fname in std::env::args().skip(1) {
        if fname == "--no-print" {
            continue;
        }

        let contents = std::fs::read(&fname);

        let Ok(contents) = contents else {
            eprintln!("can't find {}", fname);
            exit(1);
        };

        let span_offset = compiler.span_offset();
        compiler.add_file(&fname, &contents);

        let (tokens, err) = lex(&contents, span_offset);
        if let Err(e) = err {
            let contents_str = String::from_utf8_lossy(&contents);
            report_lex_error(&fname, e, &contents_str);
            exit(1);
        }

        if do_print {
            tokens.print(&compiler.source);
        }

        let parser = Parser::new(compiler, tokens);

        compiler = parser.parse();

        if do_print {
            compiler.print();
        }

        if !compiler.errors.is_empty() {
            let contents_str = String::from_utf8_lossy(&contents);
            report_compile_errors(&fname, "parse", &contents_str, &compiler.errors, &compiler);
            exit(1);
        }

        let mut resolver = Resolver::new(&compiler);
        resolver.resolve();

        if do_print {
            resolver.print();
        }

        compiler.merge_name_bindings(resolver.to_name_bindings());

        if !compiler.errors.is_empty() {
            let contents_str = String::from_utf8_lossy(&contents);
            report_compile_errors(
                &fname,
                "resolve",
                &contents_str,
                &compiler.errors,
                &compiler,
            );
            exit(1);
        }

        let mut typechecker = Typechecker::new(&compiler);
        typechecker.typecheck();

        if do_print {
            typechecker.print();
        }
        if !typechecker.errors.is_empty() {
            let contents_str = String::from_utf8_lossy(&contents);
            report_compile_errors(
                &fname,
                "typecheck",
                &contents_str,
                &typechecker.errors,
                &compiler,
            );
            exit(1);
        }

        let mut ir_generator = IrGenerator::new(&compiler);
        ir_generator.generate();
        if do_print {
            ir_generator.print();
        }

        compiler.merge_types(typechecker.to_types());
    }
}

fn report_lex_error(fname: &str, error: Spanned<LexError>, source_code: &str) {
    let error_span = error.span;
    let message = error.item;

    Report::build(
        ReportKind::Error,
        (fname, (error_span.start..error_span.start)),
    )
    .with_message(format!("Lexing error: {message:?}"))
    .with_label(
        Label::new((fname, error_span.start..error_span.end))
            .with_message(format!("Compiler err: {message:?}")),
    )
    .finish()
    .eprint((fname, Source::from(source_code)))
    .expect("Output error should be successful");
}

fn report_compile_errors(
    fname: &str,
    context: &str,
    source_code: &str,
    errors: &[SourceError],
    compiler: &Compiler,
) {
    let error_span = compiler.spans[errors[0].node_id.0];
    let mut builder = Report::build(
        ReportKind::Error,
        (fname, (error_span.start..error_span.start)),
    )
    .with_message(format!("{context} error"));

    for e in errors {
        let error_span = compiler.spans[e.node_id.0];
        builder = builder.with_label(
            Label::new((fname, error_span.start..error_span.end))
                .with_message(format!("Compiler err: {}", e.message)),
        );
    }
    builder
        .finish()
        .eprint((fname, Source::from(source_code)))
        .expect("Output error should be successful");
}
