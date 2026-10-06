use crate::state::PositionState;
use crate::ir::*;

pub fn eval(expr:&Expr, q:&Quote, features:&[f64], state: &PositionState) -> Value {
   match expr {
       Expr::Const(x) => Value::Num(*x),
       Expr::BoolConst(b) => Value::Bool(*b),
       Expr::Feature(id) => Value::Num(features[id.0]),
       Expr::State(f) => Value::Num(state.resolve(*f, q)),
       
       Expr::Unary(op, a) => {
	   let a = eval(a, q, features, state);
	   match op {
	       UnOp::Neg => Value::Num(-a.num()),
	       UnOp::Not => Value::Bool(!a.boolean())
	   }
       },
       Expr::Binary(op, a, b) => {
	   let a = eval(a, q, features, state);
	   let b = eval(b, q, features, state);
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
	   if eval(cond, q, features, state).boolean() {
	       eval(then, q, features, state)
	   } else { eval(otherwise, q, features, state)}
       }
   } 
    
}



#[test]
fn long_take_profit_fires() {
    let expr = tp_expr();
    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };  // long exits at bid, +25 pips
    assert_eq!(eval(&expr, &q, &[], &state).boolean(), true);
}

#[test]
fn long_take_profit_does_not_fire() {
    let expr = tp_expr();
    let state = long_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1010, ask: 1.1012 };  // +10 pips, below 20 threshold
    assert_eq!(eval(&expr, &q, &[], &state).boolean(), false);
}

#[test]
fn short_in_loss_does_not_fire() {
    let expr = tp_expr();
    let state = short_at(1.1000);
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 };  // short exits at ask, -27 pips
    assert_eq!(eval(&expr, &q, &[], &state).boolean(), false);
}

#[test]
fn short_in_profit_fires() {
    let expr = tp_expr();
    let state = short_at(1.1000);
    let q = Quote { ts: 100, bid: 1.0973, ask: 1.0975 };  // short exits at ask 1.0975, +25 pips
    assert_eq!(eval(&expr, &q, &[], &state).boolean(), true);
}


fn tp_expr() -> Expr {
    Expr::Binary(BinOp::Gt, Box::new(Expr::State(StateField::UnrealizedPnl)), Box::new(Expr::Const(0.0020)))
}

fn long_at(p:f64) -> PositionState {
    PositionState { position_size: 1.0, avg_entry_price: p, high_water_mark: p,
                    low_water_mark: p, entry_time: 0, num_entries: 1, num_exits: 0 }
}

fn short_at(p:f64) -> PositionState { PositionState { position_size: -1.0, ..long_at(p)}}









pub fn select_action<'a>(
    strategy: &'a Strategy, 
    q: &Quote,
    features:&[f64],
    state: &PositionState,
) -> Option<&'a Action> {
    strategy.rules.iter()
	.find(|rule| eval(&rule.when, q, features, state).boolean())
	.map(|rule| &rule.then)
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

    let state = long_at(1.1000);                       // long 1 lot
    let q = Quote { ts: 100, bid: 1.1025, ask: 1.1027 }; // +25 pips, exit condition true
    let features: &[f64] = &[];

    // Both rules' `when` are true here (up 20 pips AND always-true).
    // First-match-wins must pick rule 0 (Exit), not rule 1 (Enter).
    match select_action(&strategy, &q, features, &state) {
        Some(Action::Exit { .. }) => {}  // correct
        other => panic!("expected Exit to win by order, got {other:?}"),
    }
}
