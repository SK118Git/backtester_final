use crate::ir::{Quote, AccountField};
use crate::state::PositionState;
use crate::fill::Fill;
use std::collections::HashMap;

pub type InstrumentId = String;

#[derive(Debug, Clone)]
pub struct Portfolio {
    // Flows are accumulated additively during the instant for each ticker (needs to be commutative operation)
    // Flows - The source of truth 
    pub cash: f64,

    // Flows = Report Only 
    pub realized_pnl: f64,
    pub costs_paid: f64, 
    pub num_trades: u32, 
    
    // States are recomputed at the end of every instant for all tickers combined 
    // States 
    pub market_value: f64, 
    // equity = cash + market_value
    pub equity: f64
}


impl Portfolio {
    pub fn new(starting_capital:f64) -> Self {
	Portfolio {
	    cash: starting_capital,
	    realized_pnl: 0.0,
	    costs_paid: 0.0,
	    num_trades: 0,
	    market_value: 0.0,
	    equity: starting_capital,
	}
    }
    

    // Update the flow based fields, called once per fill for each ticker:
    // We compute the cash consequence using the Fill
    // buying (+ lots) reduces cash, selling ( - lots) increases cash => cash -= lots * price 
    // comission and extra cists always reduce cash => cash -= cost 
    // realized is the reporting gain for a closing fill, computed by the function's caller because it requires entry basis 
    // (which) lives in POsitionState), and is 0.0 for pure open/add.
    pub fn apply_fill(&mut self, fill:&Fill, realized:f64) {
	self.cash -= fill.lots * fill.price;
	self.cash -= fill.cost;
	self.costs_paid += fill.cost;
	self.realized_pnl += realized;
	self.num_trades += 1;
    }

    // Recompute the State: called Once at the end of the instant, after all fills of the instant have been applied 
    // Rebuild everything from scratch for safety 
    pub fn recompute_states(&mut self, positions: &HashMap<InstrumentId, PositionState>, marks: &HashMap<InstrumentId, Quote>){
	self.market_value = positions
	    .iter() 
	    .filter(|(_, p)| p.position_size != 0.0)
	    .map(|(id, p)| {
		let q = &marks[id]; // every open position must have at last known quote 
		let mark = if p.position_size > 0.0 {q.bid} else {q.ask};
		p.position_size * mark
	    }).sum(); 
	self.equity = self.cash + self.market_value;
    }
    
    pub fn recompute_single(&mut self, pos:&PositionState, q:&Quote) {
	self.market_value = if pos.position_size == 0.0 {0.0} else {
	    let mark = if pos.position_size > 0.0 {q.bid} else {q.ask};
	    pos.position_size * mark 
	};
	self.equity = self.cash + self.market_value;
    }
    
    pub fn resolve(&self, field:AccountField) -> f64 {
	match field {
	    AccountField::Equity => self.equity, 
	    AccountField::Cash => self.cash, 
	    AccountField::RealizedPnl => self.realized_pnl,
	    AccountField::MarketValue => self.market_value
	}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fill::{Fill, FillReason};
    use crate::test_helpers::{approx, flat};
 
    /// Builds a minimal long PositionState for marking tests.
    fn long_pos(size: f64, entry: f64) -> PositionState {
        PositionState { position_size: size, avg_entry_price: entry, ..flat() }
    }
 
    /// The worked example: start $100, buy 10 @ 10, sell 5 @ 15, hold 5.
    /// Hand calc: cash 75, open value 5×15 = 75, equity = 150 (up 50). Costs 0.
    #[test]
    fn round_trip_conserves_equity() {
        let mut p = Portfolio::new(100.0);
 
        // Buy 10 @ 10: lots +10, price 10 → cash −100. No realized (opening).
        p.apply_fill(
            &Fill { price: 10.0, lots: 10.0, cost: 0.0, reason: FillReason::Discretionary, ts: 0},
            0.0,
        );
        assert!(approx(p.cash, 0.0), "spent 100 on the buy");
 
        // Sell 5 @ 15: lots −5, price 15 → cash +75. Realized +25 (5 × $5 gain), reporting.
        p.apply_fill(
            &Fill { price: 15.0, lots: -5.0, cost: 0.0, reason: FillReason::Discretionary, ts: 0},
            25.0,
        );
        assert!(approx(p.cash, 75.0), "received 75 on the sell");
        assert!(approx(p.realized_pnl, 25.0), "reporting figure only");
 
        // 5 shares still held; mark at bid 15.
        let mut positions = HashMap::new();
        positions.insert("STOCK".to_string(), long_pos(5.0, 10.0));
        let mut marks = HashMap::new();
        marks.insert("STOCK".to_string(), Quote { ts: 1, bid: 15.0, ask: 15.02 });
 
        p.recompute_states(&positions, &marks);
 
        // equity = cash(75) + market_value(5 × 15 = 75) = 150. Matches hand calc.
        assert!(approx(p.market_value, 75.0));
        assert!(approx(p.equity, 150.0), "started 100, up 50");
 
        // DOUBLE-COUNT GUARD: equity must NOT be cash + market_value + realized (= 175).
        assert!(!approx(p.equity, 175.0), "realized must not be re-added into equity");
    }
 
    #[test]
    fn new_portfolio_equity_is_capital() {
        let p = Portfolio::new(1000.0);
        assert!(approx(p.equity, 1000.0));
        assert!(approx(p.cash, 1000.0));
        assert!(approx(p.market_value, 0.0));
    }
 
    #[test]
    fn cost_reduces_cash_and_accumulates() {
        let mut p = Portfolio::new(100.0);
        // Buy 1 @ 10 with 0.50 commission: cash −10 − 0.50 = 89.50.
        p.apply_fill(
            &Fill { price: 10.0, lots: 1.0, cost: 0.50, reason: FillReason::Discretionary, ts:0 },
            0.0,
        );
        assert!(approx(p.cash, 89.50));
        assert!(approx(p.costs_paid, 0.50));
    }
 
    #[test]
    fn short_position_marks_at_ask() {
        // Short 2 @ 1.1000: sold 2, cash += 2×1.1000 = 2.2000 over start.
        let mut p = Portfolio::new(100.0);
        p.apply_fill(
            &Fill { price: 1.1000, lots: -2.0, cost: 0.0, reason: FillReason::Discretionary, ts:0 },
            0.0,
        );
        assert!(approx(p.cash, 102.2000), "shorting banks the proceeds");
 
        // Open short of −2; mark at ASK (what you'd pay to buy back).
        let mut positions = HashMap::new();
        positions.insert("FX".to_string(), long_pos(-2.0, 1.1000)); // negative size = short
        let mut marks = HashMap::new();
        marks.insert("FX".to_string(), Quote { ts: 1, bid: 1.0998, ask: 1.1002 });
 
        p.recompute_states(&positions, &marks);
        // market_value = −2 × 1.1002 (ask) = −2.2004.
        assert!(approx(p.market_value, -2.2004), "short marked at ask, negative value");
        // equity = 102.2000 + (−2.2004) = 99.9996 — slightly under 100: the half-spread cost
        // of being in the trade, honestly reflected.
        assert!(approx(p.equity, 99.9996));
    }
}
