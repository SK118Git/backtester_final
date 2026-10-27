use crate::ir::{Action, Side, Size, Quote, Strategy};
use crate::state::PositionState;
use crate::fill::{Fill, price_discretionary, resolve_resting};
use crate::costs::CostConfig;
use crate::portfolio::Portfolio;
use crate::engine::{MarketCtx, select_action};
use crate::result::*;


pub fn action_to_fills (action: &Action, position:&PositionState, q:&Quote, cfg:&CostConfig) -> Vec<Fill> {
    let pos = position.position_size;
    
    match action {
	Action::Enter {side, size} => {
	    let lots = match size {
		Size::Lots(n) => *n,
		Size::RiskPct(_) => todo!("RiskPct needs equity + stop distance"),
		Size::Fraction(_) => todo!("Fraction on enter is nonsensical - CL should reject")
	    };
	    let signed = match side {Side::Long => lots, Side::Short => -lots};
	    if pos != 0.0 && signed.signum() != pos.signum() && signed.abs() > pos.abs() {
		// Reversal 
		let close = -pos;
		let open = signed - close;
		vec![price_discretionary(close, q, cfg), price_discretionary(open, q, cfg)]
	    } else {
		vec![price_discretionary(signed, q, cfg)]
	    }
	},
	Action::Exit { size } => {
	    if pos == 0.0 {return vec![];}
	    let mut close = match size {
		Size::Lots(n) => *n,
		Size::Fraction(f) => pos.abs() * f,
		Size::RiskPct(_) => todo!("RiskPct on exit is nonsence - CL should reject")
	    };
	    close = close.min(pos.abs());
	    let dir = if pos > 0.0 {-1.0} else {1.0};
	    vec![price_discretionary(dir * close, q, cfg)]
	}
	Action::SetStop {..} 
	| Action::SetTarget { .. }
	| Action::CancelStop 
	| Action::CancelTarget => vec![],
    }
}


												    

pub fn run_backtest(ticks: &[Quote], strategy: &Strategy, cfg:&RunConfig) -> BacktestResult {
    let mut position = PositionState::flat();
    let mut portfolio = Portfolio::new(cfg.starting_capital);
    let mut current = portfolio.clone();
    let mut trades: Vec<TradeRecord> = Vec::new();
    let mut equity_curve: Vec<EquityPoint> = Vec::new();
    let mut last_eq = portfolio.equity;
    
    if let Some(first) = ticks.first() {
	equity_curve.push(EquityPoint { ts: first.ts, equity: portfolio.equity })
    }
    for q in ticks {
	let ctx = MarketCtx {q, features: &[], portfolio: &current};
	position.update_trailing(&ctx);
	
	if let Some(fill) = resolve_resting(&position, q, &cfg.cost) {
	    apply_and_record(&mut position, &mut portfolio, &fill, &mut trades);
	}

	// release borrow and take ownership  to avoid issues
	if let Some(actions) = select_action(strategy, &ctx, &position).cloned() { 
	   for action in &actions {
	       match action {
		   Action::SetStop {..} | Action::SetTarget {..} | Action::CancelStop | Action::CancelTarget => {
		       position.apply_order(action, &ctx);
		   },
		   Action::Enter {.. } | Action::Exit { .. } => {
		       let fills = action_to_fills(action, &position, q, &cfg.cost);
		       for fill in &fills {
			   apply_and_record(&mut position, &mut portfolio, fill, &mut trades);
		       }
		   }
	       } 
	   }
	}
	
	portfolio.recompute_single(&position, q);
	
	current = portfolio.clone();
	if (portfolio.equity - last_eq).abs() > cfg.equity_epsilon {
	    equity_curve.push(EquityPoint { ts: q.ts, equity: portfolio.equity });
	    last_eq = portfolio.equity
	}
    }
    if let Some(last) = ticks.last() {
	equity_curve.push(EquityPoint { ts: last.ts, equity: portfolio.equity });
    }

    let manifest = RunManifest {
	data_snapshot_id: String::new(), // TODO
	strategy_hash: String::new(),  // TODO 
	config: cfg.clone(), 
	cost_model_version: String::from("unversioned"), // TODO 	
	final_equity: portfolio.equity,
	num_trades: trades.len() as u32,
	num_configs_evaluated: 1,
    };

    BacktestResult { trades, equity_curve, manifest, final_portfolio: portfolio, final_position: position}
}

fn apply_and_record(position:&mut PositionState, portfolio: &mut Portfolio, fill:&Fill, trades: &mut Vec<TradeRecord>) {
    let result = position.apply_fill(fill);
    portfolio.apply_fill(fill, result.realized);
    trades.push(TradeRecord { ts: fill.ts, price: fill.price, lots: fill.lots, cost: fill.cost, realized:result.realized, reason:fill.reason})
}



#[cfg(test)]
mod loop_tests {
    use super::*;
    use crate::test_helpers::*;
    use crate::ir::*;
    use crate::costs::CostConfig;

    fn run_cfg() -> RunConfig {
        RunConfig {
            instrument: "EURUSD".to_string(),
            starting_capital: 100_000.0,
            cost: CostConfig::default(),
            equity_epsilon: 1e-6,
        }
    }

    fn ticks(prices: &[(i64, f64, f64)]) -> Vec<Quote> {
        prices.iter().map(|(ts, bid, ask)| Quote { ts: *ts, bid: *bid, ask: *ask }).collect()
    }

    // ---- empty / no-op cases ----

    #[test]
    fn empty_strategy_never_trades() {
        let strategy = Strategy { rules: vec![] };
        let t = ticks(&[(1, 1.1000, 1.1002), (2, 1.1010, 1.1012), (3, 1.1005, 1.1007)]);
        let result = run_backtest(&t, &strategy, &run_cfg());
        assert!(result.trades.is_empty(), "no rules => no trades");
        assert!(approx(result.final_portfolio.equity, 100_000.0), "equity unchanged");
        assert!(approx(result.final_position.position_size, 0.0));
    }

    #[test]
    fn no_ticks_returns_starting_state() {
        let strategy = Strategy { rules: vec![] };
        let result = run_backtest(&[], &strategy, &run_cfg());
        assert!(approx(result.final_portfolio.equity, 100_000.0));
        assert!(result.trades.is_empty());
    }

    // ---- entry + stop atomically in one tick (the gap-closer) ----

    #[test]
    fn entry_with_stop_in_one_tick_is_protected() {
        // When flat: enter long AND set a stop, same tick (multi-action rule).
        // Guard entry on num_entries == 0 so it doesn't re-enter after the stop,
        // isolating the "stop was set atomically with entry" behavior.
        let strategy = Strategy {
            rules: vec![
                Rule {
                    when: Expr::Binary(BinOp::And,
                        Box::new(Expr::Binary(BinOp::Eq,
                            Box::new(Expr::State(StateField::PositionSize)),
                            Box::new(Expr::Const(0.0)))),
                        Box::new(Expr::Binary(BinOp::Eq,
                            Box::new(Expr::State(StateField::NumEntries)),
                            Box::new(Expr::Const(0.0))))),
                    then: vec![
                        Action::Enter { side: Side::Long, size: Size::Lots(1.0) },
                        Action::SetStop { kind: StopKind::Fixed, level: Box::new(Expr::Const(1.0980)) },
                    ],
                },
            ],
        };
        // tick1: enter @ ask 1.1002 + set stop at 1.0980 (same tick).
        // tick2: bid gaps down to 1.0975 (< 1.0980) → stop fires. Protected from tick 1.
        let t = ticks(&[(1, 1.1000, 1.1002), (2, 1.0975, 1.0977)]);
        let result = run_backtest(&t, &strategy, &run_cfg());

        // The stop existed from tick 1, so the tick-2 gap fires it.
        let stop_exits: Vec<_> = result.trades.iter()
            .filter(|tr| matches!(tr.reason, crate::fill::FillReason::Stop)).collect();
        assert_eq!(stop_exits.len(), 1, "stop fired on tick 2 — it existed from tick 1");

        // trades[0] = entry (discretionary, at ask); trades[1] = stop exit.
        assert!(approx(result.trades[0].price, 1.1002), "entered at ask");
        assert!(approx(result.trades[0].lots, 1.0));
        assert!(matches!(result.trades[0].reason, crate::fill::FillReason::Discretionary));

        // Stop exit: adverse gap-through fills at the bid 1.0975 (below the 1.0980 level).
        assert!(approx(stop_exits[0].price, 1.0975), "adverse fill at gapped bid, not the level");
        assert!(approx(stop_exits[0].lots, -1.0), "closes the long");
        // realized = (1.0975 - 1.1002) * 1 = -0.0027 (a loss)
        assert!(approx(stop_exits[0].realized, -0.0027), "stopped out at a loss");

        // flat at end (num_entries guard prevents re-entry), equity down by the loss.
        assert!(approx(result.final_position.position_size, 0.0), "flat after stop");
        assert!(approx(result.final_portfolio.equity, 100_000.0 - 0.0027));
    }

    // ---- stop set the tick AFTER entry (two-rule version, three ticks) ----

    #[test]
    fn stop_fires_and_closes() {
        // Rule 0: if flat and never entered, enter long.
        // Rule 1: if in a position, ensure a stop is set.
        let strategy = Strategy {
            rules: vec![
                Rule {
                    when: Expr::Binary(BinOp::And,
                        Box::new(Expr::Binary(BinOp::Eq,
                            Box::new(Expr::State(StateField::PositionSize)),
                            Box::new(Expr::Const(0.0)))),
                        Box::new(Expr::Binary(BinOp::Eq,
                            Box::new(Expr::State(StateField::NumEntries)),
                            Box::new(Expr::Const(0.0))))),
                    then: vec![Action::Enter { side: Side::Long, size: Size::Lots(1.0) }],
                },
                Rule {
                    when: Expr::Binary(BinOp::Gt,
                        Box::new(Expr::State(StateField::PositionSize)),
                        Box::new(Expr::Const(0.0))),
                    then: vec![Action::SetStop { kind: StopKind::Fixed, level: Box::new(Expr::Const(1.0980)) }],
                },
            ],
        };
        // tick1 enter; tick2 (still up) stop gets set; tick3 price drops, stop fires.
        let t = ticks(&[(1, 1.1000, 1.1002), (2, 1.1010, 1.1012), (3, 1.0975, 1.0977)]);
        let result = run_backtest(&t, &strategy, &run_cfg());

        let exits: Vec<_> = result.trades.iter()
            .filter(|tr| matches!(tr.reason, crate::fill::FillReason::Stop)).collect();
        assert_eq!(exits.len(), 1, "stop produced one closing fill");
        assert!(approx(result.final_position.position_size, 0.0), "flat after stop");
        assert!(exits[0].realized < 0.0, "stop realized a loss");
    }

    // ---- equity curve sampling ----

    #[test]
    fn equity_curve_has_endpoints() {
        let strategy = Strategy { rules: vec![] };
        let t = ticks(&[(1, 1.1000, 1.1002), (5, 1.1010, 1.1012)]);
        let result = run_backtest(&t, &strategy, &run_cfg());
        assert!(result.equity_curve.len() >= 2, "at least first and last point");
        assert_eq!(result.equity_curve.first().unwrap().ts, 1);
        assert_eq!(result.equity_curve.last().unwrap().ts, 5);
    }
}
