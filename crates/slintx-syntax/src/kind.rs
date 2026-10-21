//! Token and node kinds of the Slint syntax tree.

macro_rules! syntax_kinds {
    (tokens { $($token:ident),* $(,)? } nodes { $($node:ident),* $(,)? }) => {
        /// Every token and node kind. Tokens come first, so `is_token` is a
        /// single comparison.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($token,)*
            $($node,)*
        }

        // rowan stores kinds as raw `u16`; this table turns them back
        // without `unsafe` transmutes.
        const ALL: &[SyntaxKind] = &[$(SyntaxKind::$token,)* $(SyntaxKind::$node,)*];
    };
}

syntax_kinds! {
    tokens {
        Whitespace, Comment, Ident, Number, Color,
        // A string with no interpolation, or the pieces around `\{expr}`:
        // `"a\{` is StringStart, `}b\{` StringMiddle, `}c"` StringEnd.
        String, StringStart, StringMiddle, StringEnd,
        LBrace, RBrace, LParen, RParen, LBracket, RBracket,
        Semicolon, Comma, Colon, ColonEqual, DoubleArrow, FatArrow, Arrow, Dot,
        Question, Bang, Equal, PlusEqual, MinusEqual, StarEqual, SlashEqual,
        Plus, Minus, Star, Slash, EqualEqual, NotEqual,
        Less, Greater, LessEqual, GreaterEqual, AndAnd, OrOr,
        Pipe, Percent, At,
        // A character no token starts with.
        Error,
    }
    nodes {
        File, ErrorNode,
        // Items
        Import, ImportSpecifier, Export, ExportSpecifier, Component, Global,
        Struct, StructField, Enum, EnumValue, RustAttr,
        // Element members
        ElementBody, Element, QualifiedName,
        PropertyDeclaration, CallbackDeclaration, CallbackParam,
        FunctionDeclaration, Param, Binding, BindingExpression, TwoWayBinding,
        CallbackConnection, ChangedHandler, Animate, States, State,
        StatePropertyChange, Transition, Repeater, RepeaterIndex, Conditional,
        ChildrenPlaceholder,
        // Types
        NamedType, ArrayType, ObjectType, ObjectTypeMember,
        // Statements
        CodeBlock, LetStatement, ReturnStatement, IfStatement, ExprStatement,
        AssignmentExpr,
        // Expressions
        Literal, StringTemplate, NameRef, ParenExpr, ArrayLiteral,
        ObjectLiteral, ObjectMember, FieldAccess, CallExpr, ArgList, IndexExpr,
        UnaryExpr, BinaryExpr, ConditionalExpr,
        // Macros
        TrMacro, ImageUrlMacro, NineSlice, GradientMacro, MacroArg,
        MarkdownMacro, KeysMacro, KeyPart,
    }
}

impl SyntaxKind {
    pub fn is_token(self) -> bool {
        self <= SyntaxKind::Error
    }

    /// Whitespace and comments: kept in the tree, skipped by the parser.
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::Whitespace | SyntaxKind::Comment)
    }

    pub fn from_raw(raw: u16) -> Option<SyntaxKind> {
        ALL.get(usize::from(raw)).copied()
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// The rowan language tag for Slint trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlintLanguage {}

impl rowan::Language for SlintLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        // Only this crate builds trees, so an unknown raw kind cannot occur;
        // falling back to an error node keeps that path panic-free anyway.
        SyntaxKind::from_raw(raw.0).unwrap_or(SyntaxKind::ErrorNode)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}
