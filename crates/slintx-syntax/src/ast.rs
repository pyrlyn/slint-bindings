//! Typed views over the syntax tree: one type per node kind, plus enums for
//! the places where several kinds can stand. A view is a cheap wrapper around
//! a [`SyntaxNode`]; accessors return `None` where the parser recovered from
//! an error, so broken input never panics here either.

use std::marker::PhantomData;

use crate::parser::same_name;
use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

/// The children of a node that cast to `N`, in source order.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: rowan::SyntaxNodeChildren<crate::SlintLanguage>,
    marker: PhantomData<N>,
}

impl<N: AstNode> AstChildren<N> {
    fn new(parent: &SyntaxNode) -> Self {
        AstChildren {
            inner: parent.children(),
            marker: PhantomData,
        }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;
    fn next(&mut self) -> Option<N> {
        self.inner.by_ref().find_map(N::cast)
    }
}

fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    AstChildren::new(parent).next()
}

/// Direct tokens of `kind`; tokens inside child nodes do not count.
fn tokens(parent: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxToken> + use<> {
    parent
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(move |t| t.kind() == kind)
}

fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    tokens(parent, kind).next()
}

/// The first `N` child after the direct token `kind`.
fn after<N: AstNode>(parent: &SyntaxNode, kind: SyntaxKind) -> Option<N> {
    parent
        .children_with_tokens()
        .skip_while(|e| e.as_token().is_none_or(|t| t.kind() != kind))
        .filter_map(rowan::NodeOrToken::into_node)
        .find_map(N::cast)
}

/// The first direct token that is not trivia: an operator, for operator nodes.
fn operator(parent: &SyntaxNode) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
}

fn is_keyword(token: &SyntaxToken, keyword: &str) -> bool {
    same_name(token.text(), keyword)
}

macro_rules! nodes {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct $name(SyntaxNode);

            impl AstNode for $name {
                fn can_cast(kind: SyntaxKind) -> bool {
                    kind == SyntaxKind::$name
                }
                fn cast(node: SyntaxNode) -> Option<Self> {
                    Self::can_cast(node.kind()).then(|| Self(node))
                }
                fn syntax(&self) -> &SyntaxNode {
                    &self.0
                }
            }
        )*

        #[cfg(test)]
        const VIEWS: &[SyntaxKind] = &[$(SyntaxKind::$name),*];
    };
}

macro_rules! unions {
    ($($(#[$doc:meta])* $name:ident { $($variant:ident),* $(,)? })*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($variant)),*
        }

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, $(SyntaxKind::$variant)|*)
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                match node.kind() {
                    $(SyntaxKind::$variant => Some(Self::$variant($variant(node))),)*
                    _ => None,
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $(Self::$variant(node) => &node.0),*
                }
            }
        }
    )*};
}

macro_rules! accessor {
    (child $f:ident $t:ident) => {
        pub fn $f(&self) -> Option<$t> {
            child(&self.0)
        }
    };
    (children $f:ident $t:ident) => {
        pub fn $f(&self) -> AstChildren<$t> {
            AstChildren::new(&self.0)
        }
    };
    (nth $f:ident $t:ident, $n:literal) => {
        pub fn $f(&self) -> Option<$t> {
            AstChildren::new(&self.0).nth($n)
        }
    };
    (token $f:ident $k:ident) => {
        pub fn $f(&self) -> Option<SyntaxToken> {
            token(&self.0, SyntaxKind::$k)
        }
    };
    (ident $f:ident $n:literal) => {
        pub fn $f(&self) -> Option<SyntaxToken> {
            tokens(&self.0, SyntaxKind::Ident).nth($n)
        }
    };
    (after $f:ident $k:ident, $t:ident) => {
        pub fn $f(&self) -> Option<$t> {
            after(&self.0, SyntaxKind::$k)
        }
    };
    (operator $f:ident) => {
        pub fn $f(&self) -> Option<SyntaxToken> {
            operator(&self.0)
        }
    };
}

/// `Node { name: how(args), ... }` becomes one accessor per entry; see
/// `accessor!` for the forms.
macro_rules! views {
    ($($node:ident { $($f:ident: $how:ident($($arg:tt)*)),* $(,)? })*) => {$(
        impl $node {
            $(accessor!($how $f $($arg)*);)*
        }
    )*};
}

nodes! {
    File, ErrorNode, Import, ImportSpecifier, Export, ExportSpecifier, Component, Global, Struct,
    StructField, Enum, EnumValue, RustAttr, ElementBody, Element, QualifiedName,
    PropertyDeclaration, CallbackDeclaration, CallbackParam, FunctionDeclaration, Param, Binding,
    BindingExpression, TwoWayBinding, CallbackConnection, ChangedHandler, Animate, States, State,
    StatePropertyChange, Transition, Repeater, RepeaterIndex, Conditional, ChildrenPlaceholder,
    NamedType, ArrayType, ObjectType, ObjectTypeMember, CodeBlock, LetStatement, ReturnStatement,
    IfStatement, ExprStatement, AssignmentExpr, Literal, StringTemplate, NameRef, ParenExpr,
    ArrayLiteral, ObjectLiteral, ObjectMember, FieldAccess, CallExpr, ArgList, IndexExpr,
    UnaryExpr, BinaryExpr, ConditionalExpr, TrMacro, ImageUrlMacro, NineSlice, GradientMacro,
    MacroArg, MarkdownMacro, KeysMacro, KeyPart,
}

unions! {
    /// A top-level item. Error nodes between items are skipped.
    Item { Import, Export, Component, Global, Struct, Enum }

    /// A member of an element body.
    Member {
        Element, Binding, TwoWayBinding, CallbackConnection, PropertyDeclaration,
        CallbackDeclaration, FunctionDeclaration, ChangedHandler, Animate, States, Repeater,
        Conditional, ChildrenPlaceholder,
    }

    Type { NamedType, ArrayType, ObjectType }

    Statement { LetStatement, ReturnStatement, IfStatement, ExprStatement }

    Expr {
        Literal, StringTemplate, NameRef, ParenExpr, ArrayLiteral, ObjectLiteral, FieldAccess,
        CallExpr, IndexExpr, UnaryExpr, BinaryExpr, ConditionalExpr, AssignmentExpr, TrMacro,
        ImageUrlMacro, GradientMacro, MarkdownMacro, KeysMacro,
    }
}

views! {
    File { items: children(Item) }
    Import { specifiers: children(ImportSpecifier), path: token(String) }
    ImportSpecifier { name: ident(0), alias: ident(2) }
    Export {
        attrs: children(RustAttr),
        specifiers: children(ExportSpecifier), star: token(Star), path: token(String),
        declaration: child(Item),
    }
    ExportSpecifier { name: ident(0), alias: ident(2) }
    Component {
        attrs: children(RustAttr), name: ident(1), base: child(QualifiedName),
        body: child(ElementBody),
    }
    Global { attrs: children(RustAttr), name: ident(1), body: child(ElementBody) }
    Struct { attrs: children(RustAttr), name: ident(1), fields: children(StructField) }
    StructField { name: ident(0), ty: child(Type), default: child(Expr) }
    Enum { attrs: children(RustAttr), name: ident(1), values: children(EnumValue) }
    EnumValue { name: ident(0) }
    ElementBody { members: children(Member) }
    Element { base: child(QualifiedName), body: child(ElementBody) }
    PropertyDeclaration {
        ty: child(Type), value: child(BindingExpression), alias: after(DoubleArrow, Expr),
    }
    CallbackDeclaration {
        params: children(CallbackParam), return_type: child(Type),
        alias: after(DoubleArrow, Expr),
    }
    CallbackParam { ty: child(Type) }
    FunctionDeclaration {
        params: children(Param), return_type: child(Type), body: child(CodeBlock),
    }
    Param { name: ident(0), ty: child(Type) }
    Binding { name: ident(0), value: child(BindingExpression) }
    BindingExpression { block: child(CodeBlock), expr: child(Expr) }
    TwoWayBinding { name: ident(0), target: child(Expr) }
    CallbackConnection {
        name: ident(0), block: child(CodeBlock), statement: child(ExprStatement),
    }
    ChangedHandler {
        property: ident(1), block: child(CodeBlock), statement: child(ExprStatement),
    }
    Animate { star: token(Star), targets: children(QualifiedName), bindings: children(Binding) }
    States { states: children(State) }
    State {
        name: ident(0), condition: child(Expr), changes: children(StatePropertyChange),
        transitions: children(Transition),
    }
    StatePropertyChange { target: child(QualifiedName), value: child(BindingExpression) }
    Transition { direction: ident(0), animations: children(Animate) }
    Repeater { index: child(RepeaterIndex), model: child(Expr), element: child(Element) }
    RepeaterIndex { name: ident(0) }
    Conditional { condition: child(Expr), element: child(Element) }
    NamedType { name: child(QualifiedName) }
    ArrayType { element: child(Type) }
    ObjectType { members: children(ObjectTypeMember) }
    ObjectTypeMember { name: ident(0), ty: child(Type) }
    CodeBlock { statements: children(Statement) }
    LetStatement { name: ident(1), ty: child(Type), value: child(Expr) }
    ReturnStatement { value: child(Expr) }
    IfStatement {
        condition: child(Expr), then_block: child(CodeBlock), else_block: nth(CodeBlock, 1),
        else_if: child(IfStatement),
    }
    ExprStatement { expr: child(Expr) }
    AssignmentExpr { target: nth(Expr, 0), op: operator(), value: nth(Expr, 1) }
    StringTemplate { exprs: children(Expr) }
    NameRef { name: ident(0) }
    ParenExpr { expr: child(Expr) }
    ArrayLiteral { elements: children(Expr) }
    ObjectLiteral { members: children(ObjectMember) }
    ObjectMember { name: ident(0), value: child(Expr) }
    FieldAccess { receiver: child(Expr), field: ident(0) }
    CallExpr { callee: child(Expr), args: child(ArgList) }
    ArgList { args: children(Expr) }
    IndexExpr { base: nth(Expr, 0), index: nth(Expr, 1) }
    UnaryExpr { op: operator(), operand: child(Expr) }
    BinaryExpr { lhs: nth(Expr, 0), op: operator(), rhs: nth(Expr, 1) }
    ConditionalExpr {
        condition: nth(Expr, 0), then_branch: nth(Expr, 1), else_branch: nth(Expr, 2),
    }
    TrMacro { count: after(Percent, Expr) }
    ImageUrlMacro { path: child(Literal), nine_slice: child(NineSlice) }
    GradientMacro { name: ident(0), args: children(MacroArg) }
    MacroArg { exprs: children(Expr) }
    MarkdownMacro { strings: children(Expr) }
    KeysMacro { parts: children(KeyPart) }
    KeyPart { optional: token(Question) }
}

impl QualifiedName {
    /// The names between the dots: `a.b.c` has three.
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens(&self.0, SyntaxKind::Ident)
    }
}

impl Element {
    /// The `id` of `id := Base { }`.
    pub fn id(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::ColonEqual)?;
        tokens(&self.0, SyntaxKind::Ident).next()
    }
}

/// The idents of a declaration up to its `keyword`, then the one after it.
fn split_at_keyword(node: &SyntaxNode, keyword: &str) -> (Vec<SyntaxToken>, Option<SyntaxToken>) {
    let mut idents = tokens(node, SyntaxKind::Ident);
    let before = idents
        .by_ref()
        .take_while(|t| !is_keyword(t, keyword))
        .collect();
    (before, idents.next())
}

impl PropertyDeclaration {
    /// `in`, `out`, `in-out` or `private`, if written.
    pub fn visibility(&self) -> Option<SyntaxToken> {
        split_at_keyword(&self.0, "property").0.into_iter().next()
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        split_at_keyword(&self.0, "property").1
    }
}

impl CallbackDeclaration {
    /// `pure`, `public` or `protected`, in source order.
    pub fn modifiers(&self) -> Vec<SyntaxToken> {
        split_at_keyword(&self.0, "callback").0
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        split_at_keyword(&self.0, "callback").1
    }
}

impl FunctionDeclaration {
    /// `pure`, `public` or `protected`, in source order.
    pub fn modifiers(&self) -> Vec<SyntaxToken> {
        split_at_keyword(&self.0, "function").0
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        split_at_keyword(&self.0, "function").1
    }
}

impl CallbackParam {
    /// The `name` of `name: type`; parameters may be unnamed.
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Colon)?;
        tokens(&self.0, SyntaxKind::Ident).next()
    }
}

impl CallbackConnection {
    /// The names in `name(a, b) => ...`.
    pub fn params(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens(&self.0, SyntaxKind::Ident).skip(1)
    }
}

impl Repeater {
    /// The `item` of `for item in model`; it may be left out.
    pub fn variable(&self) -> Option<SyntaxToken> {
        tokens(&self.0, SyntaxKind::Ident)
            .nth(1)
            .filter(|t| !is_keyword(t, "in"))
    }
}

impl Literal {
    /// The number, color or string token.
    pub fn token(&self) -> Option<SyntaxToken> {
        self.0.first_token()
    }
}

impl StringTemplate {
    /// The string pieces around the interpolations, quotes and braces kept.
    pub fn pieces(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        self.0
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| {
                use SyntaxKind as K;
                matches!(t.kind(), K::StringStart | K::StringMiddle | K::StringEnd)
            })
    }
}

impl TrMacro {
    fn strings(&self) -> AstChildren<Literal> {
        AstChildren::new(&self.0)
    }

    fn has(&self, kind: SyntaxKind) -> bool {
        token(&self.0, kind).is_some()
    }

    /// The string before `=>`.
    pub fn context(&self) -> Option<Literal> {
        self.strings()
            .next()
            .filter(|_| self.has(SyntaxKind::FatArrow))
    }

    pub fn message(&self) -> Option<Literal> {
        self.strings()
            .nth(usize::from(self.has(SyntaxKind::FatArrow)))
    }

    /// The string after `|`.
    pub fn plural(&self) -> Option<Literal> {
        let index = usize::from(self.has(SyntaxKind::FatArrow)) + 1;
        self.strings()
            .nth(index)
            .filter(|_| self.has(SyntaxKind::Pipe))
    }

    /// The format arguments after the first `,`.
    pub fn args(&self) -> impl Iterator<Item = Expr> + use<> {
        self.0
            .children_with_tokens()
            .skip_while(|e| e.kind() != SyntaxKind::Comma)
            .filter_map(rowan::NodeOrToken::into_node)
            .filter_map(Expr::cast)
    }
}

impl NineSlice {
    pub fn values(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens(&self.0, SyntaxKind::Number)
    }
}

impl KeyPart {
    /// A key name or a string.
    pub fn key(&self) -> Option<SyntaxToken> {
        operator(&self.0).filter(|t| t.kind() != SyntaxKind::Question)
    }
}

#[cfg(test)]
mod tests {
    use super::VIEWS;
    use crate::SyntaxKind;

    #[test]
    fn every_node_kind_has_a_view() {
        let nodes: Vec<SyntaxKind> = (0..)
            .map_while(SyntaxKind::from_raw)
            .filter(|k| !k.is_token())
            .collect();
        assert_eq!(nodes, VIEWS);
    }
}
