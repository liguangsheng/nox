use super::*;

#[test]
fn lexes_keywords_literals_and_spans() {
    let tokens =
        lex("export record User { name: str, } let answer: int = 42; answer.name; true && false || true; for i in 0..3 {} match (answer) { 42 => {} _ => {} } [1]").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Export);
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Record));
    assert!(tokens
        .iter()
        .any(|token| token.kind == TokenKind::Identifier("answer".to_string())));
    assert!(tokens.iter().any(|token| {
        token.kind == TokenKind::Identifier("User".to_string())
            && token.span == Span { start: 14, end: 18 }
    }));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Dot));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Let));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Colon));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Int(42)));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::AndAnd));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::OrOr));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::For));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::In));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Match));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::FatArrow));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::DotDot));
    assert!(tokens
        .iter()
        .any(|token| token.kind == TokenKind::LeftBracket));
    assert!(tokens
        .iter()
        .any(|token| token.kind == TokenKind::RightBracket));
}

#[test]
fn lexes_string_escape_sequences() {
    let tokens = lex(r#""line\n\t\"quote\"\\";"#).unwrap();
    assert_eq!(
        tokens[0].kind,
        TokenKind::String("line\n\t\"quote\"\\".to_string())
    );
}

#[test]
fn rejects_unsupported_string_escape() {
    let err = lex(r#""bad \r";"#).unwrap_err();
    assert!(err.message.contains("unsupported escape sequence '\\r'"));
}

#[test]
fn rejects_multiline_string_literal() {
    let err = lex("\"bad\nstring\";").unwrap_err();
    assert!(err.message.contains("multiline strings are not supported"));
}

#[test]
fn compiles_ast_to_bytecode_module() {
    let tokens = lex(r#"
            let value: int = 1;
            fn id(input: int) -> int {
                return input;
            }
            id(value);
            "#)
    .unwrap();
    let module = parse(tokens).unwrap();
    let bytecode = compile(&module);
    assert!(bytecode
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, bytecode::Instruction::Define { .. })));
    assert!(bytecode
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, bytecode::Instruction::Call { .. })));
    assert!(bytecode
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, bytecode::Instruction::Function { .. })));
}

#[test]
fn parses_option_and_result_type_syntax() {
    let tokens = lex(r#"
        fn read(input: option[int]) -> result[str, str] {
            return "unused";
        }
        "#)
    .unwrap();
    let module = parse(tokens).unwrap();
    let Stmt::Function {
        params,
        return_type,
        ..
    } = &module.statements[0]
    else {
        panic!("expected function declaration");
    };
    assert_eq!(params[0].ty, Type::Option(Box::new(Type::Int)));
    assert_eq!(
        *return_type,
        Type::Result {
            ok: Box::new(Type::Str),
            err: Box::new(Type::Str),
        }
    );
    assert_eq!(params[0].ty.to_string(), "option[int]");
    assert_eq!(return_type.to_string(), "result[str, str]");
}

#[test]
fn rejects_invalid_option_and_result_type_arity() {
    for (source, message) in [
        ("let value: option[] = null;", "expected type name"),
        (
            "let value: option[int, str] = null;",
            "expected ']' after option type",
        ),
        (
            "let value: result[int] = null;",
            "expected ',' after result ok type",
        ),
        (
            "let value: result[int, str, bool] = null;",
            "expected ']' after result type",
        ),
    ] {
        let tokens = lex(source).unwrap();
        let diagnostics = parse_all(tokens).unwrap_err();
        assert!(
            diagnostics[0].message.contains(message),
            "expected {message:?}, got {:?}",
            diagnostics[0].message
        );
    }
}

#[test]
fn bytecode_verifier_rejects_invalid_jump_target() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::Jump {
            target: 2,
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("invalid jump target"));
}

#[test]
fn bytecode_verifier_rejects_stack_underflow() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::Pop {
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("stack underflow"));
}

#[test]
fn bytecode_verifier_rejects_map_stack_underflow() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::Map {
            value_type: Type::Int,
            entry_count: 1,
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("stack underflow"));
}

#[test]
fn bytecode_verifier_rejects_record_stack_underflow() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::Record {
            name: "User".to_string(),
            fields: vec!["name".to_string()],
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("stack underflow"));
}

#[test]
fn bytecode_verifier_rejects_field_stack_underflow() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::Field {
            name: "name".to_string(),
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("stack underflow"));
}

#[test]
fn bytecode_verifier_rejects_scope_underflow() {
    let module = BytecodeModule {
        instructions: vec![bytecode::Instruction::EndScope {
            span: Span { start: 0, end: 0 },
        }],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("scope stack underflow"));
}

#[test]
fn bytecode_verifier_rejects_stack_mismatch_at_branch_join() {
    let span = Span { start: 0, end: 0 };
    let module = BytecodeModule {
        instructions: vec![
            bytecode::Instruction::Constant {
                value: Value::Bool(true),
                span,
            },
            bytecode::Instruction::JumpIfFalse { target: 3, span },
            bytecode::Instruction::Constant {
                value: Value::Int(1),
                span,
            },
            bytecode::Instruction::Pop { span },
        ],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("stack height mismatch"));
}

#[test]
fn bytecode_verifier_rejects_scope_mismatch_at_branch_join() {
    let span = Span { start: 0, end: 0 };
    let module = BytecodeModule {
        instructions: vec![
            bytecode::Instruction::Constant {
                value: Value::Bool(true),
                span,
            },
            bytecode::Instruction::JumpIfFalse { target: 3, span },
            bytecode::Instruction::BeginScope { span },
            bytecode::Instruction::EndScope { span },
        ],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("scope depth mismatch"));
}

#[test]
fn bytecode_verifier_rejects_incompatible_branches_to_same_target() {
    let span = Span { start: 0, end: 0 };
    let module = BytecodeModule {
        instructions: vec![
            bytecode::Instruction::Constant {
                value: Value::Bool(true),
                span,
            },
            bytecode::Instruction::JumpIfFalse { target: 5, span },
            bytecode::Instruction::Constant {
                value: Value::Int(0),
                span,
            },
            bytecode::Instruction::Constant {
                value: Value::Int(0),
                span,
            },
            bytecode::Instruction::Jump { target: 5, span },
            bytecode::Instruction::Pop { span },
        ],
    };
    let err = bytecode::verify(&module).unwrap_err();
    assert!(err.message.contains("incompatible branches"));
}

#[test]
fn checks_and_inspects_bytecode_without_running() {
    let mut engine = Engine::new();
    engine.check("let value: int = 42; value;").unwrap();

    let bytecode = engine
        .inspect_bytecode("let value: int = 42; value;")
        .unwrap();
    assert!(bytecode.contains("Define"));
    assert!(bytecode.contains("Pop"));
}

#[test]
fn rejects_untyped_variable_declaration() {
    let tokens = lex("let answer = 42;").unwrap();
    let err = parse(tokens).unwrap_err();
    assert_eq!(err.code, "parse.expected-token");
    assert!(err.message.contains("expected ':'"));
}

#[test]
fn parser_collects_multiple_top_level_errors() {
    let tokens = lex(r#"
            let first = 1;
            let second = 2;
            "#)
    .unwrap();
    let diagnostics = parse_all(tokens).unwrap_err();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].code, "parse.expected-token");
    assert_eq!(diagnostics[1].code, "parse.expected-token");
    assert!(diagnostics[0].message.contains("expected ':'"));
    assert!(diagnostics[1].message.contains("expected ':'"));
}

#[test]
fn parser_recovers_after_invalid_for_statement() {
    let tokens = lex(r#"
            for i in 0..3;
            let later = 1;
            "#)
    .unwrap();
    let diagnostics = parse_all(tokens).unwrap_err();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics[0].message.contains("expected '{'"));
    assert!(diagnostics[1].message.contains("expected ':'"));
}
