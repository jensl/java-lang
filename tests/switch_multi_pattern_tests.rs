//! Multiple case patterns in one switch label (JEP 441).
//!
//! A pattern label may list several alternatives separated by commas, e.g.
//! `case String s, Integer i ->`, and a single guard applies to the whole
//! label: `case String s, Integer i when s.isEmpty() ->`. The guard is part of
//! the label, not of an individual alternative (javac rejects
//! `case String s when s.isEmpty(), Integer i`).

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

/// The patterns of the first label of the first switch-expression arm.
fn switch_expr_patterns(cu: &CompilationUnit) -> &[Pattern] {
    let SwitchArm::Expr(SwitchCase::CasePattern { patterns, .. }, _, _) =
        &first_switch_expr(cu).cases[0]
    else {
        panic!("expected a case pattern arm in the switch expression");
    };
    patterns
}

/// The guard of the first label of the first switch-expression arm.
fn switch_expr_guard(cu: &CompilationUnit) -> Option<&Guard> {
    let SwitchArm::Expr(SwitchCase::CasePattern { guard, .. }, _, _) =
        &first_switch_expr(cu).cases[0]
    else {
        panic!("expected a case pattern arm in the switch expression");
    };
    guard.as_ref().as_ref()
}

fn switch_stmt_patterns(cu: &CompilationUnit) -> &[Pattern] {
    let SwitchCase::CasePattern { patterns, .. } = &first_switch_stmt(cu).cases[0].labels[0] else {
        panic!("expected a case pattern label in the switch statement");
    };
    patterns
}

fn switch_stmt_case(cu: &CompilationUnit) -> &SwitchCase {
    &first_switch_stmt(cu).cases[0].labels[0]
}

/// Assert that `src` parses to a pattern label with the given type-and-binding
/// shape, where each element is `(type source text, binding name or "_")`.
fn assert_switch_expr_patterns(src: &str, expected: &[(&str, &str)]) {
    let cu = parse_method(src);
    let patterns = switch_expr_patterns(&cu);
    assert_eq!(
        patterns.len(),
        expected.len(),
        "unexpected pattern count in {src:?}"
    );
    for (pattern, (expected_type, expected_name)) in patterns.iter().zip(expected) {
        let Pattern::TypePattern(type_pattern) = pattern else {
            panic!("expected a type pattern in {src:?}, got {pattern:?}");
        };
        assert_eq!(
            type_pattern.ty.span().source_text(src),
            *expected_type,
            "unexpected pattern type in {src:?}"
        );
        match (&type_pattern.name, *expected_name) {
            (Some(name), "_") => {
                panic!("expected an unnamed binding for {expected_type}, got {name:?}")
            }
            (Some(name), expected) => assert_eq!(name.name, expected),
            (None, "_") => {}
            (None, expected) => panic!("expected binding {expected} in {src:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Pattern alternatives
// ---------------------------------------------------------------------------

#[test]
fn test_two_type_patterns_in_one_label() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String s, Integer i -> 1; default -> 0; }; } }",
        &[("String", "s"), ("Integer", "i")],
    );
}

#[test]
fn test_three_type_patterns_in_one_label() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String s, Integer i, Long l -> 1; default -> 0; }; } }",
        &[("String", "s"), ("Integer", "i"), ("Long", "l")],
    );
}

#[test]
fn test_unnamed_pattern_alternatives() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String _, Integer _ -> 1; default -> 0; }; } }",
        &[("String", "_"), ("Integer", "_")],
    );
}

#[test]
fn test_unnamed_pattern_alternatives_real_world_shape() {
    // dev/gmx/game/filtering/GameFilteringExpressionParser.java etc. use
    // `case A _, B _, C _`.
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String _, Integer _, Long _ -> 1; default -> 0; }; } }",
        &[("String", "_"), ("Integer", "_"), ("Long", "_")],
    );
}

#[test]
fn test_qualified_pattern_alternatives() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case java.time.Duration d, Outer.Inner i -> 1; default -> 0; }; } }",
        &[("java.time.Duration", "d"), ("Outer.Inner", "i")],
    );
}

#[test]
fn test_parameterized_pattern_alternatives() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case Class<?> c, Map<?, ?> m -> 1; default -> 0; }; } }",
        &[("Class<?>", "c"), ("Map<?, ?>", "m")],
    );
}

#[test]
fn test_record_pattern_alternatives() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case Box(int x), Bag(String s) -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let patterns = switch_expr_patterns(&cu);
    assert_eq!(patterns.len(), 2);
    for (pattern, expected_type) in patterns.iter().zip(["Box", "Bag"]) {
        let Pattern::RecordPattern(record) = pattern else {
            panic!("expected a record pattern, got {pattern:?}");
        };
        assert_eq!(record.record_type.span().source_text(src), expected_type);
        assert_eq!(record.components.len(), 1);
    }
}

#[test]
fn test_record_pattern_and_unnamed_alternative() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case Box(int x), String _ -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let patterns = switch_expr_patterns(&cu);
    assert_eq!(patterns.len(), 2);
    assert!(matches!(patterns[0], Pattern::RecordPattern(_)));
    let Pattern::TypePattern(type_pattern) = &patterns[1] else {
        panic!("expected a type pattern, got {:?}", patterns[1]);
    };
    assert_eq!(type_pattern.ty.span().source_text(src), "String");
    assert!(type_pattern.name.is_none(), "expected an unnamed pattern");
}

// ---------------------------------------------------------------------------
// Guards apply to the whole label
// ---------------------------------------------------------------------------

#[test]
fn test_guard_after_pattern_list() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String s, Integer i when s.isEmpty() -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let patterns = switch_expr_patterns(&cu);
    assert_eq!(patterns.len(), 2);
    let guard = switch_expr_guard(&cu).expect("expected a label-level guard");
    assert_eq!(guard.expr.span().source_text(src), "s.isEmpty()");
}

#[test]
fn test_guard_after_unnamed_pattern_list() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String _, Integer _ when o != null -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    assert_eq!(switch_expr_patterns(&cu).len(), 2);
    let guard = switch_expr_guard(&cu).expect("expected a label-level guard");
    assert_eq!(guard.expr.span().source_text(src), "o != null");
}

#[test]
fn test_single_pattern_with_guard_still_works() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String s when s.isEmpty() -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    assert_eq!(switch_expr_patterns(&cu).len(), 1);
    let guard = switch_expr_guard(&cu).expect("expected a label-level guard");
    assert_eq!(guard.expr.span().source_text(src), "s.isEmpty()");
}

#[test]
fn test_pattern_label_span_covers_alternatives_and_guard() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String s, Integer i when s.isEmpty() -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let SwitchArm::Expr(label, _, _) = &first_switch_expr(&cu).cases[0] else {
        panic!("expected an expression arm");
    };
    assert_eq!(
        label.span().source_text(src),
        "case String s, Integer i when s.isEmpty()"
    );
}

#[test]
fn test_pattern_label_span_without_guard() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case String s, Integer i -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let SwitchArm::Expr(label, _, _) = &first_switch_expr(&cu).cases[0] else {
        panic!("expected an expression arm");
    };
    assert_eq!(label.span().source_text(src), "case String s, Integer i");
}

// ---------------------------------------------------------------------------
// Switch statements
// ---------------------------------------------------------------------------

#[test]
fn test_pattern_alternatives_in_switch_statement() {
    let src = "class Foo { void m(Object o) { switch (o) { \
               case String s, Integer i -> { } default -> { } } } }";
    let cu = parse_method(src);
    let patterns = switch_stmt_patterns(&cu);
    assert_eq!(patterns.len(), 2);
    assert_eq!(patterns[0].span().source_text(src), "String s");
    assert_eq!(patterns[1].span().source_text(src), "Integer i");
}

#[test]
fn test_pattern_alternatives_in_colon_style_switch_statement() {
    let src = "class Foo { void m(Object o) { switch (o) { \
               case String s, Integer i: break; default: break; } } }";
    let cu = parse_method(src);
    let patterns = switch_stmt_patterns(&cu);
    assert_eq!(patterns.len(), 2);
    assert!(
        matches!(switch_stmt_case(&cu), SwitchCase::CasePattern { .. }),
        "expected a pattern label"
    );
}

#[test]
fn test_pattern_alternatives_with_guard_in_switch_statement() {
    let src = "class Foo { void m(Object o) { switch (o) { \
               case String s, Integer i when o != null: break; default: break; } } }";
    let cu = parse_method(src);
    let SwitchCase::CasePattern {
        patterns, guard, ..
    } = switch_stmt_case(&cu)
    else {
        panic!("expected a pattern label");
    };
    assert_eq!(patterns.len(), 2);
    assert_eq!(
        guard
            .as_ref()
            .as_ref()
            .expect("expected a label-level guard")
            .expr
            .span()
            .source_text(src),
        "o != null"
    );
}

#[test]
fn test_pattern_alternative_before_constant_alternative() {
    // The pattern label ends at the arrow; the next label may still be a
    // constant one.
    let src = "class Foo { void m(Object o) { switch (o) { \
               case String s, Integer i: break; case 1: break; default: break; } } }";
    let cu = parse_method(src);
    let switch_stmt = first_switch_stmt(&cu);
    assert_eq!(switch_stmt.cases.len(), 3);
    let SwitchCase::CaseValues { values, .. } = &switch_stmt.cases[1].labels[0] else {
        panic!("expected a constant label");
    };
    assert_eq!(values[0].span().source_text(src), "1");
}

// ---------------------------------------------------------------------------
// Existing label shapes keep working
// ---------------------------------------------------------------------------

#[test]
fn test_single_pattern_label_still_works() {
    assert_switch_expr_patterns(
        "class Foo { void m(Object o) { var x = switch (o) { \
         case String s -> 1; default -> 0; }; } }",
        &[("String", "s")],
    );
}

#[test]
fn test_constant_alternatives_still_work() {
    let src = "class Foo { void m(int i) { var x = switch (i) { \
               case 1, 2 -> 1; default -> 0; }; } }";
    let cu = parse_method(src);
    let SwitchArm::Expr(SwitchCase::CaseValues { values, .. }, _, _) =
        &first_switch_expr(&cu).cases[0]
    else {
        panic!("expected a case values arm");
    };
    assert_eq!(values.len(), 2);
}

#[test]
fn test_null_default_still_works() {
    let src = "class Foo { void m(Object o) { var x = switch (o) { \
               case null, default -> 1; }; } }";
    let cu = parse_method(src);
    assert!(matches!(
        &first_switch_expr(&cu).cases[0],
        SwitchArm::Expr(SwitchCase::CaseNullDefault { .. }, _, _)
    ));
}
