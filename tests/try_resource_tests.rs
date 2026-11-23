//! `try` resources that are variable accesses rather than declarations.
//!
//! JLS §14.20.3 defines `Resource = LocalVariableDeclaration | VariableAccess`
//! with `VariableAccess = ExpressionName | FieldAccess`. A resource may
//! therefore be a name or a field access (`h.stream`, `this.h`, `a.b.c`), not
//! just a bare local variable as the parser previously assumed.

use java_lang::{ast::*, parse_str};

fn parse_method(src: &str) -> CompilationUnit {
    parse_str(src).unwrap_or_else(|e| panic!("failed to parse {src:?}: {e}"))
}

fn method_body(cu: &CompilationUnit) -> &Block {
    let TypeDecl::Class(class) = &cu.type_decls()[0] else {
        panic!("expected a class declaration");
    };
    class
        .body
        .declarations
        .iter()
        .find_map(|decl| match decl {
            ClassBodyDecl::Method(m) => m.body.as_ref(),
            _ => None,
        })
        .expect("expected a method with a body")
}

fn resources(cu: &CompilationUnit) -> &[TryResource] {
    match &method_body(cu).stmts[0] {
        Stmt::Try(TryStmt::TryWithResources { resources, .. }) => resources,
        other => panic!("expected a try-with-resources statement, got {other:?}"),
    }
}

fn only_var_access(cu: &CompilationUnit) -> &Expr {
    let resources = resources(cu);
    assert_eq!(resources.len(), 1, "expected a single resource");
    match &resources[0] {
        TryResource::VarAccess(expr) => expr,
        other => panic!("expected a variable-access resource, got {other:?}"),
    }
}

fn field_access(expr: &Expr) -> &FieldAccessExpr {
    match expr {
        Expr::FieldAccess(field) => field,
        other => panic!("expected a field access, got {other:?}"),
    }
}

#[test]
fn test_bare_variable_resource() {
    let src = "class Foo { void m() { try (br) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    let Expr::Ident(ident) = expr else {
        panic!("expected an identifier expression, got {expr:?}");
    };
    assert_eq!(ident.name, "br");
}

#[test]
fn test_qualified_field_access_resource() {
    let src = "class Foo { void m() { try (h.stream) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "h.stream");
    let field = field_access(expr);
    assert_eq!(field.field.name, "stream");
    assert_eq!(field.dot_span.source_text(src), ".");
    let Expr::Ident(target) = field.target.as_ref() else {
        panic!("expected a name target, got {:?}", field.target);
    };
    assert_eq!(target.name, "h");
}

#[test]
fn test_this_field_access_resource() {
    let src = "class Foo { void m() { try (this.h) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "this.h");
    let field = field_access(expr);
    assert_eq!(field.field.name, "h");
    assert!(matches!(field.target.as_ref(), Expr::This(_)));
}

#[test]
fn test_chained_field_access_resource() {
    let src = "class Foo { void m() { try (a.b.c) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "a.b.c");
    let outer = field_access(expr);
    assert_eq!(outer.field.name, "c");
    assert_eq!(outer.target.span().source_text(src), "a.b");
    let inner = field_access(&outer.target);
    assert_eq!(inner.field.name, "b");
    let Expr::Ident(a) = inner.target.as_ref() else {
        panic!("expected a name target, got {:?}", inner.target);
    };
    assert_eq!(a.name, "a");
}

#[test]
fn test_qualified_field_access_is_qualified_from_outer_class() {
    let src = "class Foo { void m() { try (Foo.this.h) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "Foo.this.h");
    let outer = field_access(expr);
    assert_eq!(outer.field.name, "h");
    let inner = field_access(&outer.target);
    assert_eq!(inner.field.name, "this");
}

#[test]
fn test_mixed_declared_and_variable_access_resources() {
    let src = "class Foo { void m() { try (var a = open(); h.stream; br) { } } }";
    let cu = parse_method(src);
    let resources = resources(&cu);
    assert_eq!(resources.len(), 3, "expected three resources");
    let TryResource::Decl(decl) = &resources[0] else {
        panic!("expected a declared resource, got {:?}", resources[0]);
    };
    assert!(matches!(decl.ty, LocalVarType::Var(_)));
    assert_eq!(decl.declarators[0].name.as_ref().unwrap().name, "a");
    assert_eq!(
        only_var_access_at(&resources[1]).span().source_text(src),
        "h.stream"
    );
    assert_eq!(
        only_var_access_at(&resources[2]).span().source_text(src),
        "br"
    );
}

fn only_var_access_at(resource: &TryResource) -> &Expr {
    match resource {
        TryResource::VarAccess(expr) => expr,
        other => panic!("expected a variable-access resource, got {other:?}"),
    }
}

#[test]
fn test_variable_access_resource_with_trailing_semicolon() {
    let src = "class Foo { void m() { try (h.stream;) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "h.stream");
}

#[test]
fn test_declared_resources_keep_their_shape() {
    let src = "class Foo { void m() { try (AutoCloseable c = open(); var _ = open()) { } } }";
    let cu = parse_method(src);
    let resources = resources(&cu);
    let [TryResource::Decl(explicit), TryResource::Decl(inferred)] = resources else {
        panic!("expected two declared resources, got {resources:?}");
    };
    assert!(matches!(explicit.ty, LocalVarType::Type(_)));
    assert_eq!(explicit.declarators[0].name.as_ref().unwrap().name, "c");
    assert!(matches!(inferred.ty, LocalVarType::Var(_)));
    assert!(inferred.declarators[0].name.is_none());
}

#[test]
fn test_variable_access_resource_in_statement_after_other_statements() {
    let src = "class Foo { void m() { int x = 0; try (this.h) { } } }";
    let cu = parse_method(src);
    let Stmt::Try(TryStmt::TryWithResources { resources, .. }) = &method_body(&cu).stmts[1] else {
        panic!("expected a try-with-resources statement");
    };
    assert_eq!(
        only_var_access_at(&resources[0]).span().source_text(src),
        "this.h"
    );
}

#[test]
fn test_field_access_on_primary_resource() {
    // `FieldAccess = Primary . Identifier`, so the target need not be a name.
    // javac 25 compiles `try (new H().stream) { }` as written.
    let src = "class Foo { void m() { try (new H().stream) { } } }";
    let cu = parse_method(src);
    let expr = only_var_access(&cu);
    assert_eq!(expr.span().source_text(src), "new H().stream");
    let field = field_access(expr);
    assert_eq!(field.field.name, "stream");
    assert!(matches!(field.target.as_ref(), Expr::NewClass(_)));
}

#[test]
fn test_variable_access_resource_with_catch_and_finally() {
    let src = "class Foo {\n\
        void m() {\n\
            try (h.stream) {\n\
            } catch (Exception e) {\n\
            } finally {\n\
            }\n\
        }\n\
    }";
    let cu = parse_method(src);
    let Stmt::Try(TryStmt::TryWithResources {
        resources,
        catches,
        finally_block,
        ..
    }) = &method_body(&cu).stmts[0]
    else {
        panic!("expected a try-with-resources statement");
    };
    assert!(matches!(&resources[0], TryResource::VarAccess(_)));
    assert_eq!(catches.len(), 1);
    assert!(finally_block.is_some());
}
