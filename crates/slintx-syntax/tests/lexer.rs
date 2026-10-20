use slintx_syntax::{SyntaxKind as K, lex, parse};

/// Significant tokens with their text; trivia dropped.
fn tokens(source: &str) -> Vec<(K, &str)> {
    let (tokens, diagnostics) = lex(source);
    assert!(diagnostics.is_empty(), "{source:?}: {diagnostics:?}");
    tokens
        .into_iter()
        .filter(|(kind, _)| !kind.is_trivia())
        .map(|(kind, range)| (kind, &source[range]))
        .collect()
}

fn diagnostics(source: &str) -> Vec<String> {
    lex(source).1.iter().map(ToString::to_string).collect()
}

#[test]
fn bom_is_whitespace() {
    let (tokens, _) = lex("\u{feff}a");
    assert_eq!(tokens[0].0, K::Whitespace);
    assert_eq!(u32::from(tokens[0].1.end()), 3);
    assert_eq!(tokens[1].0, K::Ident);
}

#[test]
fn comments_nest() {
    let source = "/* a /* b */ c */ x // tail\ny /// doc\n/** z */";
    let (all, diagnostics) = lex(source);
    assert!(diagnostics.is_empty());
    let comments: Vec<_> = all
        .iter()
        .filter(|(k, _)| *k == K::Comment)
        .map(|(_, r)| &source[*r])
        .collect();
    assert_eq!(
        comments,
        ["/* a /* b */ c */", "// tail", "/// doc", "/** z */"]
    );
    assert_eq!(tokens(source), [(K::Ident, "x"), (K::Ident, "y")]);
}

#[test]
fn unterminated_block_comment() {
    assert_eq!(
        diagnostics("a /* /* */"),
        ["2..10: unterminated block comment"]
    );
}

#[test]
fn identifiers_take_dashes_and_unicode() {
    assert_eq!(
        tokens("foo-bar _x a-b a - b größe in-out"),
        [
            (K::Ident, "foo-bar"),
            (K::Ident, "_x"),
            (K::Ident, "a-b"),
            (K::Ident, "a"),
            (K::Minus, "-"),
            (K::Ident, "b"),
            (K::Ident, "größe"),
            (K::Ident, "in-out"),
        ]
    );
    // A leading `-` never starts an identifier.
    assert_eq!(tokens("-a"), [(K::Minus, "-"), (K::Ident, "a")]);
}

#[test]
fn numbers_carry_units() {
    assert_eq!(
        tokens("42 3.14 10px 1.5s 50% 0.25turn 10foo 1.abs 1-2"),
        [
            (K::Number, "42"),
            (K::Number, "3.14"),
            (K::Number, "10px"),
            (K::Number, "1.5s"),
            (K::Number, "50%"),
            (K::Number, "0.25turn"),
            (K::Number, "10foo"),
            (K::Number, "1"),
            (K::Dot, "."),
            (K::Ident, "abs"),
            (K::Number, "1"),
            (K::Minus, "-"),
            (K::Number, "2"),
        ]
    );
}

#[test]
fn colors() {
    assert_eq!(tokens("#fff #FFFA #1188ff #1188ffcc").len(), 4);
    assert_eq!(
        diagnostics("#12 #12g45"),
        [
            "0..3: invalid color literal: expected #rgb, #rgba, #rrggbb or #rrggbbaa",
            "4..10: invalid color literal: expected #rgb, #rgba, #rrggbb or #rrggbbaa",
        ]
    );
}

#[test]
fn strings_and_interpolation() {
    assert_eq!(tokens(r#""a\"b\\c\n""#), [(K::String, r#""a\"b\\c\n""#)]);
    assert_eq!(
        tokens(r#""x \{ {a: 1}.a } y \{"in"} z""#),
        [
            (K::StringStart, r#""x \{"#),
            (K::LBrace, "{"),
            (K::Ident, "a"),
            (K::Colon, ":"),
            (K::Number, "1"),
            (K::RBrace, "}"),
            (K::Dot, "."),
            (K::Ident, "a"),
            (K::StringMiddle, r#"} y \{"#),
            (K::String, r#""in""#),
            (K::StringEnd, r#"} z""#),
        ]
    );
    assert_eq!(
        diagnostics(r#"x "abc"#),
        [r#"2..6: unterminated string literal"#]
    );
}

#[test]
fn punctuation() {
    let source =
        "{ } ( ) [ ] ; , : := <=> => -> . ? ! = += -= *= /= + - * / == != < > <= >= && || | % @";
    let kinds: Vec<K> = tokens(source).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        kinds,
        [
            K::LBrace,
            K::RBrace,
            K::LParen,
            K::RParen,
            K::LBracket,
            K::RBracket,
            K::Semicolon,
            K::Comma,
            K::Colon,
            K::ColonEqual,
            K::DoubleArrow,
            K::FatArrow,
            K::Arrow,
            K::Dot,
            K::Question,
            K::Bang,
            K::Equal,
            K::PlusEqual,
            K::MinusEqual,
            K::StarEqual,
            K::SlashEqual,
            K::Plus,
            K::Minus,
            K::Star,
            K::Slash,
            K::EqualEqual,
            K::NotEqual,
            K::Less,
            K::Greater,
            K::LessEqual,
            K::GreaterEqual,
            K::AndAnd,
            K::OrOr,
            K::Pipe,
            K::Percent,
            K::At,
        ]
    );
}

#[test]
fn unknown_characters_are_error_tokens() {
    assert_eq!(
        diagnostics("a $ & ~"),
        [
            "2..3: unexpected character",
            "4..5: unexpected character",
            "6..7: unexpected character",
        ]
    );
}

#[test]
fn every_input_prints_back() {
    for source in [
        "",
        "\u{feff}component A {}",
        "a /* open",
        "\"a \\{ b",
        "$$$ \u{0} \r\n\t",
        "#zz \"\\",
        "}}}{{{",
    ] {
        assert_eq!(parse(source).syntax().to_string(), source);
    }
}
