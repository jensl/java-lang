//! Switch case pattern detection.
//!
//! `switch` labels must accept every legal pattern form, not just the ones whose
//! type happens to be a single simple name. These tests cover qualified type
//! patterns, wildcard-parameterized type patterns, qualified record patterns, and
//! array/annotated/`final` type patterns, plus the constant-label fallback that
//! must keep working alongside them.

use java_lang::{ast::*, parse_str};

fn parse_method(src: &str) -> CompilationUnit {
    parse_str(src).unwrap_or_else(|e| panic!("failed to parse {src:?}: {e}"))
}

fn class_body(cu: &CompilationUnit) -> &ClassBodyDeclList {
    let TypeDecl::Class(class) = &cu.type_decls()[0] else {
        panic!("expected a class declaration");
    };
    &class.body
}

fn method_body(cu: &CompilationUnit) -> &Block {
    class_body(cu)
        .declarations
        .iter()
        .find_map(|decl| match decl {
            ClassBodyDecl::Method(m) => m.body.as_ref(),
            _ => None,
        })
        .expect("expected a method with a body")
}

fn first_switch_expr(cu: &CompilationUnit) -> &SwitchExpr {
    let Stmt::LocalVarDecl(decl) = &method_body(cu).stmts[0] else {
        panic!("expected a local variable declaration");
    };
    let Some(Expr::Switch(switch_expr)) = &decl.declarators[0].initializer else {
        panic!("expected a switch expression initializer");
    };
    switch_expr
}

fn first_switch_stmt(cu: &CompilationUnit) -> &SwitchStmt {
    match &method_body(cu).stmts[0] {
        Stmt::Switch(switch_stmt) => switch_stmt,
        other => panic!("expected a switch statement, got {other:?}"),
    }
}

fn switch_expr_pattern(cu: &CompilationUnit) -> &Pattern {
    let SwitchArm::Expr(SwitchCase::CasePattern { pattern, .. }, _, _) =
        &first_switch_expr(cu).cases[0]
    else {
        panic!("expected a case pattern arm in the switch expression");
    };
    pattern
}

fn switch_stmt_pattern(cu: &CompilationUnit) -> &Pattern {
    let SwitchCase::CasePattern { pattern, .. } = &first_switch_stmt(cu).cases[0].labels[0] else {
        panic!("expected a case pattern label in the switch statement");
    };
    pattern
}

fn switch_expr_type_pattern(cu: &CompilationUnit) -> &TypePattern {
    match switch_expr_pattern(cu) {
        Pattern::TypePattern(pattern) => pattern,
        other => panic!("expected a type pattern, got {other:?}"),
    }
}

fn switch_expr_record_pattern(cu: &CompilationUnit) -> &RecordPattern {
    match switch_expr_pattern(cu) {
        Pattern::RecordPattern(pattern) => pattern,
        other => panic!("expected a record pattern, got {other:?}"),
    }
}

/// Assert that `src` parses and that its first switch-expression label is a type
/// pattern whose type source text is `expected_type` and whose binding is `name`.
fn assert_switch_expr_type_pattern(src: &str, expected_type: &str, name: &str) {
    let cu = parse_method(src);
    let pattern = switch_expr_type_pattern(&cu);
    assert_eq!(
        pattern.ty.span().source_text(src),
        expected_type,
        "unexpected pattern type in {src:?}"
    );
    let ident = pattern
        .name
        .as_ref()
        .unwrap_or_else(|| panic!("expected a named pattern in {src:?}"));
    assert_eq!(ident.name, name);
}

/// Assert that `src` parses and that its first switch-statement label is a type
/// pattern whose type source text is `expected_type`.
fn assert_switch_stmt_type_pattern(src: &str, expected_type: &str) {
    let cu = parse_method(src);
    match switch_stmt_pattern(&cu) {
        Pattern::TypePattern(pattern) => assert_eq!(
            pattern.ty.span().source_text(src),
            expected_type,
            "unexpected pattern type in {src:?}"
        ),
        other => panic!("expected a type pattern in {src:?}, got {other:?}"),
    }
}

/// Assert that the first switch-expression label is a constant label (not a
/// pattern) whose single value has the given source text.
fn assert_switch_expr_value(src: &str, expected_value: &str) {
    let cu = parse_method(src);
    let SwitchArm::Expr(SwitchCase::CaseValues { values, .. }, _, _) =
        &first_switch_expr(&cu).cases[0]
    else {
        panic!("expected a case values arm in {src:?}");
    };
    assert_eq!(values.len(), 1, "unexpected value count in {src:?}");
    assert_eq!(values[0].span().source_text(src), expected_value);
}

// ---------------------------------------------------------------------------
// Qualified type patterns
// ---------------------------------------------------------------------------

#[test]
fn test_qualified_type_pattern_with_package() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case java.time.Duration d -> 1; default -> 0; }; } }",
        "java.time.Duration",
        "d",
    );
}

#[test]
fn test_qualified_type_pattern_with_nested_class() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case Outer.Inner i -> 1; default -> 0; }; } }",
        "Outer.Inner",
        "i",
    );
}

#[test]
fn test_qualified_type_pattern_in_switch_statement() {
    assert_switch_stmt_type_pattern(
        "class Foo { void m(Object o) { switch (o) { \
         case java.time.Duration d -> { } default -> { } } } }",
        "java.time.Duration",
    );
}

#[test]
fn test_qualified_type_pattern_in_colon_style_switch_statement() {
    let src = "class Foo { void m(Object o) { switch (o) { \
               case java.time.Duration d: break; default: break; } } }";
    let cu = parse_method(src);
    match switch_stmt_pattern(&cu) {
        Pattern::TypePattern(pattern) => {
            assert_eq!(pattern.ty.span().source_text(src), "java.time.Duration")
        }
        other => panic!("expected a type pattern, got {other:?}"),
    }
}

#[test]
fn test_qualified_type_pattern_as_record_component() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case Box(java.time.Duration d) -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let record = switch_expr_record_pattern(&cu);
    assert_eq!(record.components.len(), 1);
    let Pattern::TypePattern(component) = &record.components[0] else {
        panic!(
            "expected a type pattern component, got {:?}",
            record.components[0]
        );
    };
    assert_eq!(component.ty.span().source_text(src), "java.time.Duration");
}

// ---------------------------------------------------------------------------
// Wildcard-parameterized type patterns
// ---------------------------------------------------------------------------

#[test]
fn test_wildcard_type_pattern() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case Enum<?> e -> 1; default -> 0; }; } }",
        "Enum<?>",
        "e",
    );
}

#[test]
fn test_wildcard_type_pattern_with_bounded_simple_type() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case Class<?> c -> 1; default -> 0; }; } }",
        "Class<?>",
        "c",
    );
}

#[test]
fn test_wildcard_type_pattern_with_multiple_arguments() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case Map<?, ?> m -> 1; default -> 0; }; } }",
        "Map<?, ?>",
        "m",
    );
}

#[test]
fn test_wildcard_type_pattern_on_qualified_type() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case java.util.Map.Entry<?, ?> e -> 1; default -> 0; }; } }",
        "java.util.Map.Entry<?, ?>",
        "e",
    );
}

#[test]
fn test_wildcard_type_pattern_in_switch_statement() {
    assert_switch_stmt_type_pattern(
        "class Foo { void m(Object o) { switch (o) { \
         case Class<?> c -> { } default -> { } } } }",
        "Class<?>",
    );
}

// ---------------------------------------------------------------------------
// Qualified record patterns
// ---------------------------------------------------------------------------

#[test]
fn test_qualified_record_pattern() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case Outer.Inner(int x) -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let record = switch_expr_record_pattern(&cu);
    assert_eq!(record.record_type.span().source_text(src), "Outer.Inner");
    assert_eq!(record.components.len(), 1);
}

#[test]
fn test_qualified_record_pattern_with_nested_pattern_name() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case GraphicItem.ExistingGraphic(var releasableAssetId) -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let record = switch_expr_record_pattern(&cu);
    assert_eq!(
        record.record_type.span().source_text(src),
        "GraphicItem.ExistingGraphic"
    );
    let Pattern::TypePattern(component) = &record.components[0] else {
        panic!(
            "expected a type pattern component, got {:?}",
            record.components[0]
        );
    };
    assert_eq!(
        component
            .name
            .as_ref()
            .expect("expected a named component")
            .name,
        "releasableAssetId"
    );
}

#[test]
fn test_qualified_record_pattern_in_switch_statement() {
    let src = "class Foo { void m(Object o) { switch (o) { \
               case java.awt.Point(int x, int y) -> { } default -> { } } } }";
    let cu = parse_method(src);
    let Pattern::RecordPattern(record) = switch_stmt_pattern(&cu) else {
        panic!("expected a record pattern");
    };
    assert_eq!(record.record_type.span().source_text(src), "java.awt.Point");
    assert_eq!(record.components.len(), 2);
}

// ---------------------------------------------------------------------------
// Array, annotated, and `final` type patterns
// ---------------------------------------------------------------------------

#[test]
fn test_array_type_pattern() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case int[] a -> 1; default -> 0; }; } }",
        "int[]",
        "a",
    );
}

#[test]
fn test_array_type_pattern_with_reference_component() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String[] s -> 1; default -> 0; }; } }",
        "String[]",
        "s",
    );
}

#[test]
fn test_annotated_type_pattern() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case @Deprecated String s -> 1; default -> 0; }; } }",
        "String",
        "s",
    );
}

#[test]
fn test_final_type_pattern() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case final String s -> 1; default -> 0; }; } }",
        "String",
        "s",
    );
}

#[test]
fn test_final_and_annotated_type_pattern() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case final @Deprecated String s -> 1; default -> 0; }; } }",
        "String",
        "s",
    );
}

// ---------------------------------------------------------------------------
// Constant labels must not be swallowed by pattern speculation
// ---------------------------------------------------------------------------

#[test]
fn test_constant_label_is_not_a_pattern() {
    assert_switch_expr_value(
        "class Foo { void m(int i) { var x = switch (i) { \
         case FOO -> 1; default -> 0; }; } }",
        "FOO",
    );
}

#[test]
fn test_parenthesized_constant_label_is_not_a_pattern() {
    assert_switch_expr_value(
        "class Foo { void m(int i) { var x = switch (i) { \
         case (1 + 2) -> 1; default -> 0; }; } }",
        "(1 + 2)",
    );
}

#[test]
fn test_qualified_constant_label_is_not_a_pattern() {
    assert_switch_expr_value(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case SomeEnum.A -> 1; default -> 0; }; } }",
        "SomeEnum.A",
    );
}

#[test]
fn test_constant_label_before_pattern_label() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case SomeEnum.A -> 1; case Outer.Inner i -> 2; default -> 0; }; } }";
    let cu = parse_method(src);
    let switch_expr = first_switch_expr(&cu);
    assert_eq!(switch_expr.cases.len(), 3);

    let SwitchArm::Expr(SwitchCase::CaseValues { values, .. }, _, _) = &switch_expr.cases[0] else {
        panic!("expected a case values arm for the constant label");
    };
    assert_eq!(values[0].span().source_text(src), "SomeEnum.A");

    let SwitchArm::Expr(SwitchCase::CasePattern { pattern, .. }, _, _) = &switch_expr.cases[1]
    else {
        panic!("expected a case pattern arm for the qualified type pattern");
    };
    let Pattern::TypePattern(pattern) = pattern else {
        panic!("expected a type pattern, got {pattern:?}");
    };
    assert_eq!(pattern.ty.span().source_text(src), "Outer.Inner");
}

#[test]
fn test_multiple_constant_labels_are_not_patterns() {
    let src = "class Foo { void m(int i) { var x = switch (i) { \
               case FOO, BAR -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let SwitchArm::Expr(SwitchCase::CaseValues { values, .. }, _, _) =
        &first_switch_expr(&cu).cases[0]
    else {
        panic!("expected a case values arm");
    };
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].span().source_text(src), "FOO");
    assert_eq!(values[1].span().source_text(src), "BAR");
}

// ---------------------------------------------------------------------------
// Previously supported pattern shapes keep working
// ---------------------------------------------------------------------------

#[test]
fn test_simple_type_pattern_still_works() {
    assert_switch_expr_type_pattern(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String s -> 1; default -> 0; }; } }",
        "String",
        "s",
    );
}

#[test]
fn test_unnamed_type_pattern_still_works() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String _ -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let pattern = switch_expr_type_pattern(&cu);
    assert_eq!(pattern.ty.span().source_text(src), "String");
    assert!(pattern.name.is_none());
}

#[test]
fn test_simple_record_pattern_still_works() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case Box(int x) -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let record = switch_expr_record_pattern(&cu);
    assert_eq!(record.record_type.span().source_text(src), "Box");
    assert_eq!(record.components.len(), 1);
}

#[test]
fn test_bare_unnamed_pattern_still_works() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case _ -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    assert!(matches!(switch_expr_pattern(&cu), Pattern::Unnamed(_)));
}

#[test]
fn test_qualified_type_pattern_with_guard() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case java.time.Duration d when d.isZero() -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let SwitchArm::Expr(SwitchCase::CasePattern { pattern, guard, .. }, _, _) =
        &first_switch_expr(&cu).cases[0]
    else {
        panic!("expected a case pattern arm");
    };
    let Pattern::TypePattern(pattern) = pattern else {
        panic!("expected a type pattern, got {pattern:?}");
    };
    assert_eq!(pattern.ty.span().source_text(src), "java.time.Duration");
    let guard = guard.as_ref().as_ref().expect("expected a guard");
    assert_eq!(guard.expr.span().source_text(src), "d.isZero()");
}
