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
fn precedence() {
    assert_eq!(
        tree("(a || b) && c"),
        "(BinaryExpr (ParenExpr ( (BinaryExpr (NameRef a) || (NameRef b)) )) && (NameRef c))"
    );
    assert_eq!(
        tree("1 + 2 * -x"),
        "(BinaryExpr (Literal 1) + (BinaryExpr (Literal 2) * (UnaryExpr - (NameRef x))))"
    );
    assert_eq!(
        tree("a - b - c"),
        "(BinaryExpr (BinaryExpr (NameRef a) - (NameRef b)) - (NameRef c))"
    );
    assert_eq!(
        tree("a + 1 == b && !c"),
        "(BinaryExpr (BinaryExpr (BinaryExpr (NameRef a) + (Literal 1)) == (NameRef b)) && (UnaryExpr ! (NameRef c)))"
    );
    assert_eq!(
        tree("(a + b) * c"),
        "(BinaryExpr (ParenExpr ( (BinaryExpr (NameRef a) + (NameRef b)) )) * (NameRef c))"
    );
}

#[test]
fn ternary_is_right_associative() {
    assert_eq!(
        tree("a ? b : c ? d : e"),
        "(ConditionalExpr (NameRef a) ? (NameRef b) : (ConditionalExpr (NameRef c) ? (NameRef d) : (NameRef e)))"
    );
}

#[test]
fn same_logical_operator_chains() {
    assert_eq!(
        tree("a && b && c"),
        "(BinaryExpr (BinaryExpr (NameRef a) && (NameRef b)) && (NameRef c))"
    );
}

#[test]
fn non_associative_operators() {
    assert_eq!(
        errors("a == b == c"),
        ["7..9: comparison operators cannot be chained; add parentheses"]
    );
    assert_eq!(
        errors("a < b < c"),
        ["6..7: comparison operators cannot be chained; add parentheses"]
    );
    assert_eq!(
        errors("a && b || c"),
        ["7..9: `&&` and `||` cannot be mixed without parentheses"]
    );
    assert!(errors("(a == b) == c").is_empty());
}

#[test]
fn postfix() {
    assert_eq!(
        tree("root.items[i].name.to-upper()"),
        "(CallExpr (FieldAccess (FieldAccess (IndexExpr (FieldAccess (NameRef root) . items) [ (NameRef i) ]) . name) . to-upper) (ArgList ( )))"
    );
    assert_eq!(
        tree("(-10).abs()"),
        "(CallExpr (FieldAccess (ParenExpr ( (UnaryExpr - (Literal 10)) )) . abs) (ArgList ( )))"
    );
    assert_eq!(
        tree("Math.max(1px, 2px,)"),
        "(CallExpr (FieldAccess (NameRef Math) . max) (ArgList ( (Literal 1px) , (Literal 2px) , )))"
    );
}

#[test]
fn literals() {
    assert_eq!(tree("#ff0000"), "(Literal #ff0000)");
    assert_eq!(tree("50%"), "(Literal 50%)");
    assert_eq!(tree(r#""a\"b\u{1F600}""#), r#"(Literal "a\"b\u{1F600}")"#);
    assert_eq!(
        tree("[1, 2, 3,]"),
        "(ArrayLiteral [ (Literal 1) , (Literal 2) , (Literal 3) , ])"
    );
    assert_eq!(tree("[]"), "(ArrayLiteral [ ])");
    assert_eq!(
        tree("{ x: 1px, y: [{a: true}] }"),
        "(ObjectLiteral { (ObjectMember x : (Literal 1px)) , (ObjectMember y : (ArrayLiteral [ (ObjectLiteral { (ObjectMember a : (NameRef true)) }) ])) })"
    );
}

#[test]
fn deep_nesting_is_an_error_not_a_crash() {
    let source = "(".repeat(100_000) + "1";
    let parse = parse_binding_expression(&source);
    assert_eq!(parse.syntax().to_string(), source);
    assert!(
        messages(&parse)
            .iter()
            .any(|m| m.ends_with("nesting is too deep"))
    );
    let chain = vec!["a"; 10_000].join(" + ");
    assert!(
        messages(&parse_binding_expression(&chain))
            .iter()
            .any(|m| m.ends_with("nesting is too deep"))
    );
}
