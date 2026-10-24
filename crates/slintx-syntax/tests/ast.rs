//! Typed AST views: every fixture can be walked through the views alone, and
//! the views' nodes and tokens cover the source they claim to.

use std::collections::HashSet;

use slintx_syntax::ast::{self, AstNode, Expr, Item, Member, Statement, Type};
use slintx_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, parse, parse_binding_expression};
// The wasm32 runner only sees `#[wasm_bindgen_test]`; natively `#[test]` stays.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

macro_rules! fixtures {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("fixtures/", $name, ".slint")))),*]
    };
}

const VALID: &[(&str, &str)] = fixtures![
    "animate",
    "callbacks",
    "changed",
    "children",
    "component",
    "conditional",
    "empty",
    "enum",
    "exports",
    "expressions",
    "functions",
    "global",
    "imports",
    "lexical",
    "macros",
    "properties",
    "repeater",
    "statements",
    "states",
    "strings",
    "struct",
    "types",
];

/// Records every node reached through typed accessors.
#[derive(Default)]
struct Walk(HashSet<SyntaxNode>);

impl Walk {
    fn see(&mut self, node: &impl AstNode) {
        self.0.insert(node.syntax().clone());
    }

    fn item(&mut self, item: Item) {
        self.see(&item);
        match item {
            Item::Import(n) => n.specifiers().for_each(|s| self.see(&s)),
            Item::Export(n) => {
                n.attrs().for_each(|a| self.see(&a));
                n.specifiers().for_each(|s| self.see(&s));
                n.declaration().into_iter().for_each(|d| self.item(d));
            }
            Item::Component(n) => {
                n.attrs().for_each(|a| self.see(&a));
                n.base().iter().for_each(|b| self.see(b));
                n.body().into_iter().for_each(|b| self.body(b));
            }
            Item::Global(n) => {
                n.attrs().for_each(|a| self.see(&a));
                n.body().into_iter().for_each(|b| self.body(b));
            }
            Item::Struct(n) => {
                n.attrs().for_each(|a| self.see(&a));
                for field in n.fields() {
                    self.see(&field);
                    field.ty().into_iter().for_each(|t| self.ty(t));
                    field.default().into_iter().for_each(|e| self.expr(e));
                }
            }
            Item::Enum(n) => {
                n.attrs().for_each(|a| self.see(&a));
                n.values().for_each(|v| self.see(&v));
            }
        }
    }

    fn body(&mut self, body: ast::ElementBody) {
        self.see(&body);
        body.members().for_each(|m| self.member(m));
    }

    fn element(&mut self, element: ast::Element) {
        self.see(&element);
        element.base().iter().for_each(|b| self.see(b));
        element.body().into_iter().for_each(|b| self.body(b));
    }

    fn member(&mut self, member: Member) {
        self.see(&member);
        match member {
            Member::Element(n) => self.element(n),
            Member::Binding(n) => self.binding(n),
            Member::TwoWayBinding(n) => n.target().into_iter().for_each(|e| self.expr(e)),
            Member::CallbackConnection(n) => {
                n.block().into_iter().for_each(|b| self.block(b));
                n.statement()
                    .into_iter()
                    .for_each(|s| self.stmt(Statement::ExprStatement(s)));
            }
            Member::PropertyDeclaration(n) => {
                n.ty().into_iter().for_each(|t| self.ty(t));
                n.value().into_iter().for_each(|v| self.value(v));
                n.alias().into_iter().for_each(|e| self.expr(e));
            }
            Member::CallbackDeclaration(n) => {
                for param in n.params() {
                    self.see(&param);
                    param.ty().into_iter().for_each(|t| self.ty(t));
                }
                n.return_type().into_iter().for_each(|t| self.ty(t));
                n.alias().into_iter().for_each(|e| self.expr(e));
            }
            Member::FunctionDeclaration(n) => {
                for param in n.params() {
                    self.see(&param);
                    param.ty().into_iter().for_each(|t| self.ty(t));
                }
                n.return_type().into_iter().for_each(|t| self.ty(t));
                n.body().into_iter().for_each(|b| self.block(b));
            }
            Member::ChangedHandler(n) => {
                n.block().into_iter().for_each(|b| self.block(b));
                n.statement()
                    .into_iter()
                    .for_each(|s| self.stmt(Statement::ExprStatement(s)));
            }
            Member::Animate(n) => self.animate(n),
            Member::States(n) => {
                for state in n.states() {
                    self.see(&state);
                    state.condition().into_iter().for_each(|e| self.expr(e));
                    for change in state.changes() {
                        self.see(&change);
                        change.target().iter().for_each(|t| self.see(t));
                        change.value().into_iter().for_each(|v| self.value(v));
                    }
                    for transition in state.transitions() {
                        self.see(&transition);
                        transition.animations().for_each(|a| self.animate(a));
                    }
                }
            }
            Member::Repeater(n) => {
                n.index().iter().for_each(|i| self.see(i));
                n.model().into_iter().for_each(|e| self.expr(e));
                n.element().into_iter().for_each(|e| self.element(e));
            }
            Member::Conditional(n) => {
                n.condition().into_iter().for_each(|e| self.expr(e));
                n.element().into_iter().for_each(|e| self.element(e));
            }
            Member::ChildrenPlaceholder(_) => {}
        }
    }

    fn animate(&mut self, animate: ast::Animate) {
        self.see(&animate);
        animate.targets().for_each(|t| self.see(&t));
        animate.bindings().for_each(|b| self.binding(b));
    }

    fn binding(&mut self, binding: ast::Binding) {
        self.see(&binding);
        binding.value().into_iter().for_each(|v| self.value(v));
    }

    fn value(&mut self, value: ast::BindingExpression) {
        self.see(&value);
        value.block().into_iter().for_each(|b| self.block(b));
        value.expr().into_iter().for_each(|e| self.expr(e));
    }

    fn ty(&mut self, ty: Type) {
        self.see(&ty);
        match ty {
            Type::NamedType(n) => n.name().iter().for_each(|q| self.see(q)),
            Type::ArrayType(n) => n.element().into_iter().for_each(|t| self.ty(t)),
            Type::ObjectType(n) => {
                for member in n.members() {
                    self.see(&member);
                    member.ty().into_iter().for_each(|t| self.ty(t));
                }
            }
        }
    }

    fn block(&mut self, block: ast::CodeBlock) {
        self.see(&block);
        block.statements().for_each(|s| self.stmt(s));
    }

    fn stmt(&mut self, statement: Statement) {
        self.see(&statement);
        match statement {
            Statement::LetStatement(n) => {
                n.ty().into_iter().for_each(|t| self.ty(t));
                n.value().into_iter().for_each(|e| self.expr(e));
            }
            Statement::ReturnStatement(n) => n.value().into_iter().for_each(|e| self.expr(e)),
            Statement::IfStatement(n) => {
                n.condition().into_iter().for_each(|e| self.expr(e));
                n.then_block().into_iter().for_each(|b| self.block(b));
                n.else_block().into_iter().for_each(|b| self.block(b));
                n.else_if()
                    .into_iter()
                    .for_each(|s| self.stmt(Statement::IfStatement(s)));
            }
            Statement::ExprStatement(n) => n.expr().into_iter().for_each(|e| self.expr(e)),
        }
    }

    fn exprs(&mut self, exprs: impl Iterator<Item = Expr>) {
        exprs.for_each(|e| self.expr(e));
    }

    fn expr(&mut self, expr: Expr) {
        self.see(&expr);
        match expr {
            Expr::Literal(_) | Expr::NameRef(_) => {}
            Expr::StringTemplate(n) => self.exprs(n.exprs()),
            Expr::ParenExpr(n) => self.exprs(n.expr().into_iter()),
            Expr::ArrayLiteral(n) => self.exprs(n.elements()),
            Expr::ObjectLiteral(n) => {
                for member in n.members() {
                    self.see(&member);
                    self.exprs(member.value().into_iter());
                }
            }
            Expr::FieldAccess(n) => self.exprs(n.receiver().into_iter()),
            Expr::CallExpr(n) => {
                self.exprs(n.callee().into_iter());
                if let Some(args) = n.args() {
                    self.see(&args);
                    self.exprs(args.args());
                }
            }
            Expr::IndexExpr(n) => self.exprs(n.base().into_iter().chain(n.index())),
            Expr::UnaryExpr(n) => self.exprs(n.operand().into_iter()),
            Expr::BinaryExpr(n) => self.exprs(n.lhs().into_iter().chain(n.rhs())),
            Expr::ConditionalExpr(n) => {
                let branches = n.condition().into_iter().chain(n.then_branch());
                self.exprs(branches.chain(n.else_branch()));
            }
            Expr::AssignmentExpr(n) => self.exprs(n.target().into_iter().chain(n.value())),
            Expr::TrMacro(n) => {
                let strings = n.context().into_iter().chain(n.message()).chain(n.plural());
                self.exprs(strings.map(Expr::Literal));
                self.exprs(n.count().into_iter().chain(n.args()));
            }
            Expr::ImageUrlMacro(n) => {
                n.path().iter().for_each(|p| self.see(p));
                n.nine_slice().iter().for_each(|s| self.see(s));
            }
            Expr::GradientMacro(n) => {
                for arg in n.args() {
                    self.see(&arg);
                    self.exprs(arg.exprs());
                }
            }
            Expr::MarkdownMacro(n) => self.exprs(n.strings()),
            Expr::KeysMacro(n) => n.parts().for_each(|p| self.see(&p)),
        }
    }
}

#[test]
fn every_fixture_walks_through_the_views() {
    for (name, source) in VALID {
        let parse = parse(source);
        let Some(file) = parse.tree::<ast::File>() else {
            unreachable!("{name}: the root is not a file");
        };
        let mut walk = Walk::default();
        walk.see(&file);
        file.items().for_each(|i| walk.item(i));
        let missed: Vec<String> = parse
            .syntax()
            .descendants()
            .filter(|n| !walk.0.contains(n))
            .map(|n| format!("{:?} {:?}", n.kind(), n.text_range()))
            .collect();
        assert!(missed.is_empty(), "{name}: views miss {missed:#?}");
    }
}

fn text(token: Option<SyntaxToken>) -> String {
    token.map(|t| t.text().to_owned()).unwrap_or_default()
}

fn first<N: AstNode>(root: &SyntaxNode) -> N {
    root.descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| unreachable!("no such node"))
}

#[test]
fn declaration_accessors_point_at_their_tokens() {
    let source = "export component App inherits Window {\n    in-out property <int> count: 0;\n    \
                  pure public callback clicked(x: int, string) -> bool;\n    \
                  public pure function f(a: int) -> int { a }\n    \
                  for item[i] in model: Text {}\n    title := Text {}\n}\n";
    let parse = parse(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    let root = parse.syntax();
    let component: ast::Component = first(&root);
    assert_eq!(text(component.name()), "App");
    let base = component.base().map(|b| b.syntax().to_string());
    assert_eq!(base.as_deref(), Some("Window"));
    let property: ast::PropertyDeclaration = first(&root);
    assert_eq!(text(property.visibility()), "in-out");
    assert_eq!(text(property.name()), "count");
    let ty = property.ty().map(|t| t.syntax().to_string());
    assert_eq!(ty.as_deref(), Some("int"));
    let callback: ast::CallbackDeclaration = first(&root);
    let modifiers: Vec<String> = callback.modifiers().iter().map(|t| t.to_string()).collect();
    assert_eq!(modifiers, ["pure", "public"]);
    assert_eq!(text(callback.name()), "clicked");
    let params: Vec<String> = callback.params().map(|p| text(p.name())).collect();
    assert_eq!(params, ["x", ""]);
    let function: ast::FunctionDeclaration = first(&root);
    assert_eq!(text(function.name()), "f");
    assert_eq!(function.modifiers().len(), 2);
    let repeater: ast::Repeater = first(&root);
    assert_eq!(text(repeater.variable()), "item");
    assert_eq!(text(repeater.index().and_then(|i| i.name())), "i");
    let ids: Vec<String> = root
        .descendants()
        .filter_map(ast::Element::cast)
        .map(|e| text(e.id()))
        .collect();
    assert_eq!(ids, ["", "title"]);
}

#[test]
fn expression_accessors_keep_structure_and_spans() {
    let source = "a.b[1] + -c * (d ? e : f)";
    let parse = parse_binding_expression(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    let Some(Expr::BinaryExpr(sum)) = parse
        .tree::<ast::BindingExpression>()
        .and_then(|b| b.expr())
    else {
        unreachable!("not a sum");
    };
    assert_eq!(text(sum.op()), "+");
    let span = |n: &dyn Fn() -> Option<Expr>| n().map(|e| &source[e.syntax().text_range()]);
    assert_eq!(span(&|| sum.lhs()), Some("a.b[1]"));
    assert_eq!(span(&|| sum.rhs()), Some("-c * (d ? e : f)"));
    let index: ast::IndexExpr = first(&parse.syntax());
    let field = index
        .base()
        .and_then(|b| ast::FieldAccess::cast(b.syntax().clone()));
    assert_eq!(text(field.and_then(|f| f.field())), "b");
    let ternary: ast::ConditionalExpr = first(&parse.syntax());
    assert_eq!(span(&|| ternary.else_branch()), Some("f"));
}

#[test]
fn macro_accessors_split_their_arguments() {
    let source = r#"@tr("ctx" => "one {n}" | "{n} items" % n, extra)"#;
    let parse = parse_binding_expression(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    let tr: ast::TrMacro = first(&parse.syntax());
    let string = |l: Option<ast::Literal>| text(l.and_then(|l| l.token()));
    assert_eq!(string(tr.context()), r#""ctx""#);
    assert_eq!(string(tr.message()), r#""one {n}""#);
    assert_eq!(string(tr.plural()), r#""{n} items""#);
    assert_eq!(
        tr.count().map(|e| e.syntax().to_string()).as_deref(),
        Some("n")
    );
    let args: Vec<String> = tr.args().map(|e| e.syntax().to_string()).collect();
    assert_eq!(args, ["extra"]);

    let plain = parse_binding_expression(r#"@tr("hi")"#);
    let tr: ast::TrMacro = first(&plain.syntax());
    assert!(tr.context().is_none() && tr.plural().is_none());
    assert_eq!(string(tr.message()), r#""hi""#);

    let keys = parse_binding_expression(r#"@keys(Control + Shift? + "z")"#);
    let parts: Vec<(String, bool)> = first::<ast::KeysMacro>(&keys.syntax())
        .parts()
        .map(|p| (text(p.key()), p.optional().is_some()))
        .collect();
    let expected = [("Control", false), ("Shift", true), ("\"z\"", false)];
    assert_eq!(parts, expected.map(|(k, o)| (k.to_owned(), o)));
}

#[test]
fn views_reject_other_kinds_and_skip_errors() {
    let parse = parse("oops\ncomponent A {}\n");
    let root = parse.syntax();
    assert!(ast::Component::cast(root.clone()).is_none());
    assert!(parse.tree::<ast::BindingExpression>().is_none());
    let Some(file) = parse.tree::<ast::File>() else {
        unreachable!("the root is a file");
    };
    let items: Vec<SyntaxKind> = file.items().map(|i| i.syntax().kind()).collect();
    assert_eq!(items, [SyntaxKind::Component]);
}

#[test]
fn legacy_component_names_the_ident_before_colon_equal() {
    let root = parse("Main := Window { Text {} }\ncomponent Other inherits Main {}\n").syntax();
    let names: Vec<String> = root
        .children()
        .filter_map(ast::Component::cast)
        .map(|c| text(c.name()))
        .collect();
    assert_eq!(names, ["Main", "Other"]);
    let legacy: ast::Component = first(&root);
    assert_eq!(
        text(legacy.base().and_then(|b| b.segments().next())),
        "Window"
    );
}
