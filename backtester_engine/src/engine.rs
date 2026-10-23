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
) -> Option<&'a Action> {
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
                then: Action::Exit { size: Size::Fraction(1.0) },
            },
            Rule {
                when: Expr::BoolConst(true),
                then: Action::Enter { side: Side::Long, size: Size::Lots(1.0) },
            },
        ],
    };

    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };
    let pf = test_pf();
    let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };

    match select_action(&strategy, &ctx, &state) {
        Some(Action::Exit { .. }) => {}
        other => panic!("expected Exit to win by order, got {other:?}"),
    }
}
