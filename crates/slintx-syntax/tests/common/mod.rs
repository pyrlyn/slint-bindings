#![allow(dead_code)] // Each test binary uses a different subset.

use slintx_syntax::{Parse, SyntaxNode};

/// The tree as an S-expression: nodes as `(Kind ...)`, significant tokens as
/// their text, trivia dropped.
pub fn sexpr(node: &SyntaxNode) -> String {
    let mut out = format!("({:?}", node.kind());
    for child in node.children_with_tokens() {
        match child {
            slintx_syntax::NodeOrToken::Node(n) => out += &format!(" {}", sexpr(&n)),
            slintx_syntax::NodeOrToken::Token(t) if !t.kind().is_trivia() => {
                out += &format!(" {}", t.text())
            }
            slintx_syntax::NodeOrToken::Token(_) => {}
        }
    }
    out + ")"
}

pub fn messages(parse: &Parse) -> Vec<String> {
    parse
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect()
}
