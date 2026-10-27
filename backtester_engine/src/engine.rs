
use crate::state::PositionState;
use crate::ir::*;
use crate::portfolio::Portfolio;


#[cfg(test)]
use crate::test_helpers::*;


pub struct MarketCtx<'a> {
    pub q: &'a Quote, 
    pub features: &'a [f64],
    pub portfolio: &'a Portfolio
}

pub fn eval(expr:&Expr, ctx:&MarketCtx, state:&PositionState) -> Value {
   match expr {
       Expr::Const(x) => Value::Num(*x),
       Expr::BoolConst(b) => Value::Bool(*b),
       Expr::Feature(id) => Value::Num(ctx.features[id.0]),
       Expr::State(f) => Value::Num(state.resolve(*f, ctx.q)),
       
       Expr::Unary(op, a) => {
	   let a = eval(a, ctx, state);
	   match op {
	       UnOp::Neg => Value::Num(-a.num()),
	       UnOp::Not => Value::Bool(!a.boolean())
	   }
       },
       Expr::Binary(op, a, b) => {
	   let a = eval(a, ctx, state);
	   let b = eval(b, ctx, state);
	   match op {
	       BinOp::Add => Value::Num(a.num() + b.num()),
	       BinOp::Sub => Value::Num(a.num() - b.num()),
	       BinOp::Mul => Value::Num(a.num() * b.num()),
	       BinOp::Div => Value::Num(a.num() / b.num()),
	       BinOp::Eq  => Value::Bool(a.num() == b.num()),
	       BinOp::Gt  => Value::Bool(a.num() > b.num()),
	       BinOp::Ge  => Value::Bool(a.num() >= b.num()),
	       BinOp::Lt  => Value::Bool(a.num() < b.num()),
	       BinOp::Le  => Value::Bool(a.num() <= b.num()),
	       BinOp::And => Value::Bool(a.boolean() && b.boolean()),
	       BinOp::Or  => Value::Bool(a.boolean() || b.boolean()),
	   }
       },
       Expr::Cond { cond, then, otherwise} => {
	   if eval(cond, ctx, state).boolean() {
	       eval(then, ctx, state)
	   } else { eval(otherwise, ctx, state)}
       },
       Expr::Account(field) => Value::Num(ctx.portfolio.resolve(*field)),
   } 
    
}




pub fn select_action<'a>(
    strategy: &'a Strategy, 
    ctx: &MarketCtx,
    state:&'a PositionState,
) -> Option<&'a Vec<Action>> {
    strategy.rules.iter()
	.find(|rule| eval(&rule.when, ctx, state).boolean())
	.map(|rule| &rule.then)
}



// Tests
#[test]
fn long_take_profit_fires() {
    let expr = tp_expr();
    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };  // long exits at bid, +25 pips
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
    assert_eq!(eval(&expr, &ctx, &state).boolean(), true);
}

#[test]
fn long_take_profit_does_not_fire() {
    let expr = tp_expr();
    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1010, ask: 1.1012 };  // +10 pips, below 20 threshold
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
    assert_eq!(eval(&expr, &ctx, &state).boolean(), false);
}

#[test]
fn short_in_loss_does_not_fire() {
    let expr = tp_expr();
    let state = short_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };  // short exits at ask, -27 pips
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
    assert_eq!(eval(&expr, &ctx, &state).boolean(), false);
}

#[test]
fn short_in_profit_fires() {
    let expr = tp_expr();
    let state = short_at(1.1000);
    let q = Quote { ts: 100, bid: 1.0973, ask: 1.0975 };  // short exits at ask 1.0975, +25 pips
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
    assert_eq!(eval(&expr, &ctx, &state).boolean(), true);
}

#[test]
fn first_match_wins_exit_before_entry() {
    let strategy = Strategy {
        rules: vec![
            Rule {
                when: Expr::Binary(BinOp::Gt,
                    Box::new(Expr::State(StateField::UnrealizedPnl)),
                    Box::new(Expr::Const(0.0020))),
                then: vec![Action::Exit { size: Size::Fraction(1.0) }],   // ← vec!
            },
            Rule {
                when: Expr::BoolConst(true),
                then: vec![Action::Enter { side: Side::Long, size: Size::Lots(1.0) }],  // ← vec!
            },
        ],
    };

    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };

    // select_action now returns the first matching rule's ACTION LIST.
    let actions = select_action(&strategy, &ctx, &state).expect("a rule should fire");
    // Both rules match; first-match-wins picks rule 0 (the exit list).
    assert_eq!(actions.len(), 1, "rule 0 has one action");
    match &actions[0] {
        Action::Exit { .. } => {}  // correct — exit won by order
        other => panic!("expected Exit to win by order, got {other:?}"),
    }
}

#[test]
fn rule_can_carry_multiple_actions() {
    let strategy = Strategy {
        rules: vec![
            Rule {
                when: Expr::BoolConst(true),
                then: vec![
                    Action::Enter { side: Side::Long, size: Size::Lots(1.0) },
                    Action::SetStop { kind: StopKind::Fixed, level: Box::new(Expr::Const(1.0980)) },
                    Action::SetTarget { level: Box::new(Expr::Const(1.1050)) },
                ],
            },
        ],
    };
    let state = flat();
    let q = Quote { ts: 1, bid: 1.1000, ask: 1.1002 };
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };

    let actions = select_action(&strategy, &ctx, &state).expect("rule fires");
    assert_eq!(actions.len(), 3, "entry + stop + target in one rule");
    assert!(matches!(actions[0], Action::Enter { .. }));
    assert!(matches!(actions[1], Action::SetStop { .. }));
    assert!(matches!(actions[2], Action::SetTarget { .. }));
}
