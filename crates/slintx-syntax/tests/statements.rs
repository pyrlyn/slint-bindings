mod common;

use common::{messages, sexpr};
use slintx_syntax::parse_binding_expression;
// The wasm32 runner only sees `#[wasm_bindgen_test]`; natively `#[test]` stays.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

/// Parses a binding value; asserts it round-trips and has no diagnostics.
fn tree(source: &str) -> String {
    let parse = parse_binding_expression(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert!(
        parse.diagnostics().is_empty(),
        "{source:?}: {:?}",
        messages(&parse)
    );
    let s = sexpr(&parse.syntax());
    // Drop the `(BindingExpression ...)` wrapper.
    s["(BindingExpression ".len()..s.len() - 1].to_string()
}

/// Parses broken input; asserts it round-trips and returns the diagnostics.
fn errors(source: &str) -> Vec<String> {
    let parse = parse_binding_expression(source);
    assert_eq!(parse.syntax().to_string(), source);
    messages(&parse)
}

#[test]
fn string_interpolation() {
    assert_eq!(
        tree(r#""a \{x + 1} b \{"c"}!""#),
        r#"(StringTemplate "a \{ (BinaryExpr (NameRef x) + (Literal 1)) } b \{ (Literal "c") }!")"#
    );
    assert_eq!(
        tree(r#""\{ {a: 1}.a }""#),
        r#"(StringTemplate "\{ (FieldAccess (ObjectLiteral { (ObjectMember a : (Literal 1)) }) . a) }")"#
    );
}

#[test]
fn bad_escapes_and_interpolations() {
    assert_eq!(
        errors(r#""a\qb\u{110000}""#),
        [
            "2..4: invalid escape sequence",
            "5..7: invalid escape sequence"
        ]
    );
    assert_eq!(
        errors(r#""\{a b}""#),
        ["5..6: expected `}` to close the interpolation"]
    );
    assert_eq!(
        errors(r#""\{a"#),
        ["4..4: unterminated string interpolation"]
    );
}

#[test]
fn code_blocks_and_statements() {
    assert_eq!(
        tree("{ let a: int = 1; if a > 0 { return a; } else if b { x += 1; } else { foo(); } a }"),
        "(CodeBlock { (LetStatement let a : (NamedType (QualifiedName int)) = (Literal 1) ;) \
         (IfStatement if (BinaryExpr (NameRef a) > (Literal 0)) (CodeBlock { (ReturnStatement return (NameRef a) ;) }) \
         else (IfStatement if (NameRef b) (CodeBlock { (ExprStatement (AssignmentExpr (NameRef x) += (Literal 1)) ;) }) \
         else (CodeBlock { (ExprStatement (CallExpr (NameRef foo) (ArgList ( ))) ;) }))) (ExprStatement (NameRef a)) })"
    );
    assert_eq!(
        tree("{ ; return; }"),
        "(CodeBlock { ; (ReturnStatement return ;) })"
    );
    // `if` followed by `.` is an identifier, not a statement.
    assert_eq!(
        tree("{ if.x }"),
        "(CodeBlock { (ExprStatement (FieldAccess (NameRef if) . x)) })"
    );
    assert_eq!(tree("{ }"), "(CodeBlock { })");
}

#[test]
fn statement_errors_recover() {
    assert_eq!(
        errors("{ x := 1; y = 2; }"),
        ["4..6: `:=` cannot declare a local variable; use `let`"]
    );
    assert_eq!(errors("{ a b }"), ["4..5: expected `;`"]);
    assert_eq!(errors("{ ) a; }"), ["2..3: expected an expression"]);
    assert_eq!(errors("{ a = ; b }"), ["6..7: expected an expression"]);
    assert_eq!(errors("1 + "), ["4..4: expected an expression"]);
    assert_eq!(
        errors("a b"),
        ["2..3: unexpected input after the expression"]
    );
}

#[test]
fn macros() {
    assert_eq!(
        tree(r#"@tr("ctx" => "{n} item" | "{n} items" % count, name)"#),
        r#"(TrMacro @ tr ( (Literal "ctx") => (Literal "{n} item") | (Literal "{n} items") % (NameRef count) , (NameRef name) ))"#
    );
    assert_eq!(
        tree(r#"@tr("Hello")"#),
        r#"(TrMacro @ tr ( (Literal "Hello") ))"#
    );
    assert_eq!(
        tree(r#"@image-url("a.png", nine-slice(1 2 3 4))"#),
        r#"(ImageUrlMacro @ image-url ( (Literal "a.png") , (NineSlice nine-slice ( 1 2 3 4 )) ))"#
    );
    assert_eq!(
        tree("@linear-gradient(90deg, #3f87a6 0%, red 100%)"),
        "(GradientMacro @ linear-gradient ( (MacroArg (Literal 90deg)) , (MacroArg (Literal #3f87a6) (Literal 0%)) , (MacroArg (NameRef red) (Literal 100%)) ))"
    );
    assert_eq!(
        tree("@radial-gradient(circle 4px at 1px 2px, blue 0%)"),
        "(GradientMacro @ radial-gradient ( (MacroArg (NameRef circle) (Literal 4px) (NameRef at) (Literal 1px) (Literal 2px)) , (MacroArg (NameRef blue) (Literal 0%)) ))"
    );
    assert_eq!(
        tree("@conic-gradient(from 90deg, red 0deg, blue 360deg)"),
        "(GradientMacro @ conic-gradient ( (MacroArg (NameRef from) (Literal 90deg)) , (MacroArg (NameRef red) (Literal 0deg)) , (MacroArg (NameRef blue) (Literal 360deg)) ))"
    );
    assert_eq!(
        tree(r#"@markdown("**a**" "\{b}")"#),
        r#"(MarkdownMacro @ markdown ( (Literal "**a**") (StringTemplate "\{ (NameRef b) }") ))"#
    );
    assert_eq!(
        tree(r#"@keys(Control + Shift? + "€")"#),
        r#"(KeysMacro @ keys ( (KeyPart Control) + (KeyPart Shift ?) + (KeyPart "€") ))"#
    );
}

#[test]
fn macro_errors_recover() {
    assert_eq!(errors("@foo(1, (2))"), ["0..1: unknown macro `@foo`"]);
    assert_eq!(errors("@markdown()"), ["10..11: expected a string literal"]);
    assert_eq!(
        errors("@tr(x)"),
        ["4..5: expected a string literal", "4..5: expected `)`"]
    );
    assert_eq!(
        errors("@image-url(\"a\", 3)"),
        ["16..17: expected `nine-slice(...)`", "16..17: expected `)`"]
    );
}

#[test]
fn types() {
    assert_eq!(
        tree("{ let a: [[int]] = []; let b: { x: length, y: [Foo.Bar], } = c; }"),
        "(CodeBlock { (LetStatement let a : (ArrayType [ (ArrayType [ (NamedType (QualifiedName int)) ]) ]) = (ArrayLiteral [ ]) ;) \
         (LetStatement let b : (ObjectType { (ObjectTypeMember x : (NamedType (QualifiedName length))) , \
         (ObjectTypeMember y : (ArrayType [ (NamedType (QualifiedName Foo . Bar)) ])) , }) = (NameRef c) ;) })"
    );
    assert_eq!(errors("{ let a: = 1; }"), ["9..10: expected a type"]);
}
