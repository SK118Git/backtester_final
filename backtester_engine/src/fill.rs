use crate::state::{PositionState, RestingOrder};
use crate::ir::Quote;
use crate::costs::*;

#[cfg(test)]
use crate::test_helpers::*;


// Did the stop trigger on this tick?
fn stop_triggered(order: &RestingOrder, q:&Quote, is_long:bool) -> bool {
    if is_long { q.bid <= order.level} // long exits at bid, stop is below 
    else { q.ask >= order.level} // short exits at ask, stop is above
}


// Did the target trigger on this tick?
fn target_triggered(order: &RestingOrder, q:&Quote, is_long:bool) -> bool {
    if is_long { q.bid >= order.level} // long exits at bid, target is above  
    else { q.ask <= order.level} // short exits at ask, target is below
   
}


pub enum FillReason {Stop, Target, Discretionary}


pub struct Fill {
    pub price: f64,
    pub lots: f64,
    pub cost: f64,
    pub reason: FillReason
}


enum FillKind {Stop, Target}

// level = the order's price and market is current exit-side price 
fn fill_price(level: f64, market:f64, is_long:bool, kind:FillKind, bias:FillBias)-> f64 {
    match bias {
	FillBias::Favorable => level, // you always get the level 
	FillBias::Mid => (level + market) / 2.0,
	FillBias::Adverse => {
	    // market gapped past level so you eat the gap 
	    match (kind, is_long) {
		(FillKind::Stop, true) => level.min(market),
		(FillKind::Stop, false) => level.max(market),
		(FillKind::Target, true) => level.min(market),
		(FillKind::Target, false) => level.max(market)
	    }
	}
    }
}

pub fn resolve_resting(state:&PositionState, q:&Quote, cfg: &CostConfig) -> Option<Fill> {
    if state.position_size == 0.0 {return None;}
    let is_long = state.position_size > 0.0;
    let market = if is_long {q.bid} else {q.ask};
    let lots = -state.position_size;
    
    // Stop takes priority 
    if let Some(stop) = &state.orders.stop {
	if stop_triggered(stop, q, is_long) {
	    let price = fill_price(stop.level, market, is_long, FillKind::Stop, cfg.fill_bias);
	    let cost = compute_cost(lots, price, cfg);
	    return Some(Fill {price, lots: lots, reason: FillReason::Stop, cost: cost});
	}
    }
    if let Some(tgt) = &state.orders.target {
	if target_triggered(tgt, q, is_long) {
	    let price = fill_price(tgt.level, market, is_long, FillKind::Target, cfg.fill_bias);
	    let cost = compute_cost(lots, price, cfg);
	    return Some(Fill {price, lots: lots, reason: FillReason::Target, cost:cost});
	}
    }
    None 
}


pub fn price_discretionary(signed_lots: f64, q:&Quote, cfg: &CostConfig) -> Fill {
    let price = if signed_lots > 0.0 {q.ask} else {q.bid};
    let cost = compute_cost(signed_lots, price, cfg);
    Fill { price, lots: signed_lots, cost, reason: FillReason::Discretionary }
}


// Tests

#[test]
fn long_stop_triggers_when_bid_falls_to_level() {
    let stop = RestingOrder { level: 1.0980, trailing: None };
    assert!( stop_triggered(&stop, &Quote { ts:1, bid:1.0980, ask:1.0982 }, true), "bid at level triggers");
    assert!( stop_triggered(&stop, &Quote { ts:1, bid:1.0975, ask:1.0977 }, true), "bid below triggers");
    assert!(!stop_triggered(&stop, &Quote { ts:1, bid:1.0985, ask:1.0987 }, true), "bid above does not");
}

#[test]
fn short_stop_triggers_when_ask_rises_to_level() {
    let stop = RestingOrder { level: 1.1020, trailing: None };
    assert!( stop_triggered(&stop, &Quote { ts:1, bid:1.1018, ask:1.1020 }, false), "ask at level triggers");
    assert!( stop_triggered(&stop, &Quote { ts:1, bid:1.1023, ask:1.1025 }, false), "ask above triggers");
    assert!(!stop_triggered(&stop, &Quote { ts:1, bid:1.1015, ask:1.1017 }, false), "ask below does not");
}

#[test]
fn long_target_triggers_when_bid_rises_to_level() {
    let tgt = RestingOrder { level: 1.1050, trailing: None };
    assert!( target_triggered(&tgt, &Quote { ts:1, bid:1.1050, ask:1.1052 }, true));
    assert!(!target_triggered(&tgt, &Quote { ts:1, bid:1.1045, ask:1.1047 }, true));
}

#[test]
fn short_target_triggers_when_ask_falls_to_level() {
    let tgt = RestingOrder { level: 1.0950, trailing: None };
    assert!( target_triggered(&tgt, &Quote { ts:1, bid:1.0948, ask:1.0950 }, false));
    assert!(!target_triggered(&tgt, &Quote { ts:1, bid:1.0953, ask:1.0955 }, false));
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RestingOrder;
    use crate::ir::Quote;
    // approx(a, b) from test_helpers

    // --- Adverse pricing: at the level (no gap) fills AT the level ---

    #[test]
    fn long_stop_at_level_fills_at_level() {
        // bid exactly at stop 1.0980, no gap
        let p = fill_price(1.0980, 1.0980, true, FillKind::Stop, FillBias::Adverse);
        assert!(approx(p, 1.0980), "no gap => fill at level");
    }

    #[test]
    fn long_stop_gap_through_eats_the_gap() {
        // stop 1.0980, but bid gapped down to 1.0970 — you fill at the worse price
        let p = fill_price(1.0980, 1.0970, true, FillKind::Stop, FillBias::Adverse);
        assert!(approx(p, 1.0970), "adverse: gapped past stop => fill at market, not level");
    }

    #[test]
    fn short_stop_gap_through_eats_the_gap() {
        // short stop 1.1020 (above), ask gapped up to 1.1035 — fill worse (higher)
        let p = fill_price(1.1020, 1.1035, false, FillKind::Stop, FillBias::Adverse);
        assert!(approx(p, 1.1035), "adverse short stop: gapped up => fill at higher market");
    }

    #[test]
    fn long_target_adverse_never_better_than_level() {
        // target 1.1050, bid gapped up to 1.1060 — you do NOT get the extra, capped at... market or level?
        // Adverse = level.min(market) = min(1.1050, 1.1060) = 1.1050. You get the level, not the gap bonus.
        let p = fill_price(1.1050, 1.1060, true, FillKind::Target, FillBias::Adverse);
        assert!(approx(p, 1.1050), "adverse: no favorable gap bonus on target");
    }

    // --- Favorable: always the level, gap ignored ---

    #[test]
    fn favorable_always_fills_at_level() {
        let p = fill_price(1.0980, 1.0970, true, FillKind::Stop, FillBias::Favorable);
        assert!(approx(p, 1.0980), "favorable: ignore the gap, fill at level");
    }

    // --- Mid: halfway ---

    #[test]
    fn mid_averages_level_and_market() {
        let p = fill_price(1.0980, 1.0970, true, FillKind::Stop, FillBias::Mid);
        assert!(approx(p, 1.0975), "mid: (level + market) / 2");
    }

    // --- resolve_resting: triggers produce the right Fill ---

    #[test]
    fn resolve_long_stop_produces_closing_fill() {
        let mut s = long_at(1.1000);               // +2 lots? long_at sets +1 — check helper
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
        let q = Quote { ts: 1, bid: 1.0975, ask: 1.0977 };   // bid below stop => triggers, gapped
        let fill = resolve_resting(&s, &q, &CostConfig::default()).expect("stop should fire");
        assert!(approx(fill.price, 1.0975), "adverse gap-through price");
        assert!(approx(fill.lots, -s.position_size), "fill closes the whole position");
        assert!(matches!(fill.reason, FillReason::Stop));
    }

    #[test]
    fn resolve_returns_none_when_nothing_triggers() {
        let mut s = long_at(1.1000);
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
        let q = Quote { ts: 1, bid: 1.1010, ask: 1.1012 };   // well above stop
        assert!(resolve_resting(&s, &q, &CostConfig::default()).is_none());
    }

    #[test]
    fn resolve_flat_position_never_fills() {
        let mut s = flat();
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None }); // shouldn't exist, but guard it
        let q = Quote { ts: 1, bid: 1.0000, ask: 1.0002 };
        assert!(resolve_resting(&s, &q, &CostConfig::default()).is_none(), "flat => no fills");
    }
    // --- compute_cost: the cost model ---

    #[test]
    fn no_slippage_is_zero_cost() {
        let cfg = CostConfig { slippage: SlippageModel::None, fill_bias: FillBias::Adverse };
        assert!(approx(compute_cost(5.0, 1.1000, &cfg), 0.0), "None => no cost");
        assert!(approx(compute_cost(-5.0, 1.1000, &cfg), 0.0), "None => no cost, any sign");
    }

    #[test]
    fn fixed_pips_cost_scales_with_size_not_sign() {
        let cfg = CostConfig { slippage: SlippageModel::FixedPips(0.0001), fill_bias: FillBias::Adverse };
        // cost = pips × |lots| = 0.0001 × 3 = 0.0003
        assert!(approx(compute_cost(3.0, 1.1000, &cfg), 0.0003), "cost scales with size");
        // sign of lots must not change cost — buying and selling cost the same
        assert!(approx(compute_cost(-3.0, 1.1000, &cfg), 0.0003), "cost uses |lots|, sign-independent");
    }

    // --- price_discretionary: honest side selection for commanded fills ---

    #[test]
    fn discretionary_buy_lifts_the_ask() {
        let cfg = CostConfig::default();
        let q = Quote { ts: 1, bid: 1.1000, ask: 1.1002 };
        let fill = price_discretionary(2.0, &q, &cfg);   // +2 = buying
        assert!(approx(fill.price, 1.1002), "buyer pays the ask");
        assert!(approx(fill.lots, 2.0));
        assert!(matches!(fill.reason, FillReason::Discretionary));
    }

    #[test]
    fn discretionary_sell_hits_the_bid() {
        let cfg = CostConfig::default();
        let q = Quote { ts: 1, bid: 1.1000, ask: 1.1002 };
        let fill = price_discretionary(-2.0, &q, &cfg);  // -2 = selling
        assert!(approx(fill.price, 1.1000), "seller receives the bid");
        assert!(approx(fill.lots, -2.0));
    }

    #[test]
    fn discretionary_cost_applied() {
        let cfg = CostConfig { slippage: SlippageModel::FixedPips(0.0001), fill_bias: FillBias::Adverse };
        let q = Quote { ts: 1, bid: 1.1000, ask: 1.1002 };
        let fill = price_discretionary(4.0, &q, &cfg);
        assert!(approx(fill.price, 1.1002), "buy at ask");
        assert!(approx(fill.cost, 0.0004), "cost = 0.0001 × 4 lots");
    }

    // --- cost flows through resolve_resting too ---

    #[test]
    fn resting_fill_carries_cost() {
        let mut s = long_at(1.1000);
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
        let q = Quote { ts: 1, bid: 1.0975, ask: 1.0977 };
        let cfg = CostConfig { slippage: SlippageModel::FixedPips(0.0001), fill_bias: FillBias::Adverse };
        let fill = resolve_resting(&s, &q, &cfg).expect("stop fires");
        // cost = 0.0001 × |lots| where lots = -position_size
        assert!(approx(fill.cost, 0.0001 * s.position_size.abs()), "resting fill priced with cost");
    }
}
