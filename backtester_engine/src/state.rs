use crate::ir::*;
use crate::engine::{eval, MarketCtx};
use crate::fill::*;


#[cfg(test)]
use crate::test_helpers::*;

#[derive(Debug)]
pub struct PositionState{
    pub position_size:f64,
    pub avg_entry_price:f64,
    pub high_water_mark:f64, 
    pub low_water_mark:f64,
    pub entry_time:i64,
    pub num_entries:u32,  
    pub num_exits:u32,

    // Resting Order part of the engine state but not StateField voc
    pub orders: OrderState,
}

pub struct ApplyResult {
    pub realized: f64,
    pub closed_out:bool
}

impl PositionState{
    fn exit_price(&self, q:&Quote) ->f64 {if self.position_size > 0.0 {q.bid} else {q.ask}}
    pub fn ur_pnl(&self, q:&Quote) -> f64 {
	(self.exit_price(q) - self.avg_entry_price) * self.position_size
    }
    pub fn tit(&self, ts:i64) -> i64 {ts - self.entry_time}
    pub fn trailing_stop_distance(&self, q:&Quote) -> f64 {self.high_water_mark -self.exit_price(q)}
    pub fn resolve(&self, field:StateField, q: &Quote) -> f64{
	match field {
	    StateField::PositionSize  => self.position_size,
	    StateField::AvgEntryPrice => self.avg_entry_price,
	    StateField::HighWaterMark => self.high_water_mark,
	    StateField::LowWaterMark => self.low_water_mark,
	    StateField::EntryTime => self.entry_time as f64,
	    StateField::NumEntries => self.num_entries as f64,
	    StateField::NumExits => self.num_exits as f64,
	    StateField::UnrealizedPnl => self.ur_pnl(q),
	    StateField::TimeInTrade => self.tit(q.ts) as f64,
	    StateField::TrailingDist => self.trailing_stop_distance(q),
	}
    }
    pub fn update_trailing(&mut self, ctx:&MarketCtx) {
	if self.position_size == 0.0 {return;}
	let is_long = self.position_size > 0.0;
	let stop_candidate = self.orders.stop.as_ref() 
	    .and_then(|s| s.trailing.as_ref())
	    .map(|expr| eval(expr, ctx, self).num());
	if let (Some(stop), Some(cand)) = (&mut self.orders.stop, stop_candidate) {
	    stop.level = if is_long {stop.level.max(cand)} else {stop.level.min(cand)};
	}
    }
    

    pub fn apply_fill(&mut self, fill:&Fill) -> ApplyResult {
	let signed = fill.lots;
	let price = fill.price;
	let is_reducing = self.position_size != 0.0 && signed.signum() != self.position_size.signum();
	if is_reducing {
	    let close_lots = signed.abs();
	    debug_assert!(close_lots <= self.position_size.abs() + 1e-9, "apply_fill received a zero-crossing fill; resolver must clamp/decompose");
	    let realized = (price - self.avg_entry_price) * close_lots * self.position_size.signum();
	    self.position_size += signed ; 
	    self.num_exits += 1;
	    let closed_out = self.position_size.abs() < 1e-9;
	    if closed_out {self.close_out()};
	    ApplyResult { realized, closed_out }
	} else {
	    let new_size = self.position_size + signed;
	    if self.position_size == 0.0 {
		self.avg_entry_price = price;
		self.entry_time = fill.ts;
		self.high_water_mark = price;
		self.low_water_mark = price;
	    } else {
		self.avg_entry_price = (self.position_size * self.avg_entry_price + signed * price) / new_size;
	    }
	    self.position_size = new_size;
	    self.num_entries += 1;
	    ApplyResult { realized: 0.0, closed_out: false }
	}
    }
    
    pub fn apply_order(&mut self, action: &Action, ctx: &MarketCtx) {
	match action {
	    Action::SetStop { kind, level } => {
		let lvl = eval(level, ctx, self).num();
		self.orders.stop = Some(RestingOrder { 
		    level: lvl, 
		    trailing: match kind {
			StopKind::Fixed => None,
			StopKind::Trailing => Some(level.clone()),
		    },
		});
	    }
	    Action::SetTarget { level } => {
		let lvl = eval(level, ctx, self).num();
		self.orders.target = Some(RestingOrder { level: lvl, trailing: None });
	    }
	    Action::CancelStop => self.orders.stop = None,
	    Action::CancelTarget => self.orders.target = None, 
	    Action::Enter { .. } | Action::Exit { .. } => {
		debug_assert!(false, "apply_order called with a fill action; should use action_to_fills instead")
	    }
	}
    }
    fn close_out(&mut self) {
	self.position_size = 0.0;
	self.avg_entry_price = 0.0; 
	self.high_water_mark = 0.0;
	self.low_water_mark = 0.0 ;
	self.entry_time = 0;
	self.orders.stop = None;
	self.orders.target = None; 
    }
    pub fn flat() -> Self {
	PositionState { position_size: 0.0, avg_entry_price: 0.0, high_water_mark: 0.0, low_water_mark: 0.0, entry_time: 0, num_entries: 0, num_exits: 0, orders: OrderState::default() }
    }
}


#[derive(Debug)]
pub struct RestingOrder{
    // The current effective stop price, frozen for a Fixed and moving for Trailing
    pub level: f64, 
    // For trailing: the expr we use to re-evaluate the level
    pub trailing: Option<Box<Expr>>
}

#[derive(Default, Debug)]
pub struct OrderState {
    pub stop: Option<RestingOrder>, 
    pub target: Option<RestingOrder>
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;   // flat, long_at, short_at, approx, test_pf, trailing_long_expr, etc.

    // Helper: build a Fill for apply_fill tests.
    fn fill(ts: i64, price: f64, lots: f64) -> Fill {
        Fill { ts, price, lots, cost: 0.0, reason: FillReason::Discretionary }
    }

    // ---------- apply_fill: opening / adding ----------

    #[test]
    fn open_from_flat_sets_basis() {
        let mut s = flat();
        let r = s.apply_fill(&fill(500, 1.1000, 2.0));   // buy 2
        assert!(approx(s.position_size, 2.0));
        assert!(approx(s.avg_entry_price, 1.1000), "basis = fill price on open");
        assert_eq!(s.entry_time, 500, "entry_time = fill ts");
        assert_eq!(s.num_entries, 1);
        assert!(approx(r.realized, 0.0), "opening realizes nothing");
        assert!(!r.closed_out);
    }

    #[test]
    fn short_open_is_negative() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, -2.0));   // sell 2 from flat = short
        assert!(approx(s.position_size, -2.0));
        assert!(approx(s.avg_entry_price, 1.1000));
    }

    #[test]
    fn add_reaverages_basis() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, 2.0));
        s.apply_fill(&fill(2, 1.1020, 2.0));    // same direction = add
	let got = s.avg_entry_price;
	assert!(approx(got, 1.1010), "expected 1.1010, got {got}");
        assert!(approx(s.position_size, 4.0));
        assert!(approx(s.avg_entry_price, 1.1010), "equal-size add averages");
        assert_eq!(s.entry_time, 1, "entry_time stays at first entry");
        assert_eq!(s.num_entries, 2);
    }

    #[test]
    fn weighted_add() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, 1.0));
        s.apply_fill(&fill(2, 1.1040, 3.0));    // (1·1.1000 + 3·1.1040)/4 = 1.1030
        assert!(approx(s.avg_entry_price, 1.1030), "size-weighted basis");
    }

    // ---------- apply_fill: reducing / closing ----------

    #[test]
    fn partial_reduce_keeps_basis() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, 4.0));         // long 4
        let r = s.apply_fill(&fill(2, 1.1030, -2.0)); // sell 2
        assert!(approx(s.position_size, 2.0), "reduced to 2");
        assert!(approx(s.avg_entry_price, 1.1000), "basis unchanged on reduce");
        assert_eq!(s.num_exits, 1);
        assert!(approx(r.realized, 0.006), "(1.1030-1.1000)*2*(+1)");
        assert!(!r.closed_out, "still holding 2");
    }

    #[test]
    fn full_close_resets_and_realizes() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, 2.0));
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
        let r = s.apply_fill(&fill(2, 1.1050, -2.0));  // sell all 2
        assert!(approx(s.position_size, 0.0), "flat");
        assert!(approx(s.avg_entry_price, 0.0), "basis reset");
        assert!(approx(s.high_water_mark, 0.0), "water marks reset");
        assert_eq!(s.entry_time, 0, "entry_time reset");
        assert!(s.orders.stop.is_none(), "resting orders cleared on close");
        assert!(approx(r.realized, 0.010), "(1.1050-1.1000)*2*(+1)");
        assert!(r.closed_out);
    }

    #[test]
    fn short_close_realizes_positive_when_covered_lower() {
        let mut s = flat();
        s.apply_fill(&fill(1, 1.1000, -2.0));          // short 2 @ 1.1000
        let r = s.apply_fill(&fill(2, 1.0950, 2.0));   // buy 2 to cover @ 1.0950
        assert!(approx(s.position_size, 0.0));
        // realized = (1.0950-1.1000)*2*(-1) = +0.010 — shorted high, covered low
        assert!(approx(r.realized, 0.010), "short profit on lower cover");
        assert!(r.closed_out);
    }

    // ---------- apply_order: stop/target registration ----------

    #[test]
    fn set_fixed_stop_evaluates_once() {
        let mut s = long_at(1.1000);
        s.high_water_mark = 1.1030;
        let q = Quote { ts: 1, bid: 1.1025, ask: 1.1027 };
        let pf = test_pf();
        let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
        s.apply_order(&Action::SetStop { kind: StopKind::Fixed, level: Box::new(Expr::Const(1.0980)) }, &ctx);
        let stop = s.orders.stop.as_ref().unwrap();
        assert!(approx(stop.level, 1.0980));
        assert!(stop.trailing.is_none(), "fixed => no stored expr");
    }

    #[test]
    fn set_trailing_stop_stores_expr() {
        let mut s = long_at(1.1000);
        s.high_water_mark = 1.1030;
        let q = Quote { ts: 1, bid: 1.1030, ask: 1.1032 };
        let pf = test_pf();
        let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
        s.apply_order(&Action::SetStop { kind: StopKind::Trailing, level: Box::new(trailing_long_expr(0.0010)) }, &ctx);
        let stop = s.orders.stop.as_ref().unwrap();
        assert!(approx(stop.level, 1.1020), "initial level evaluated now");
        assert!(stop.trailing.is_some());
    }

    #[test]
    fn cancel_clears_stop() {
        let mut s = long_at(1.1000);
        s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
        let q = Quote { ts: 1, bid: 0.0, ask: 0.0 };
        let pf = test_pf();
        let ctx = MarketCtx { q: &q, features: &[], portfolio: &pf };
        s.apply_order(&Action::CancelStop, &ctx);
        assert!(s.orders.stop.is_none());
    }

}



   /* 
    pub fn apply(&mut self, action:&Action, fill_price:f64, ctx: &MarketCtx)-> ApplyResult {
	match action {
	    Action::Enter {side, size} => {
		let lots = match size {
		    Size::Lots(n) => *n,
		    Size::RiskPct(_) => todo!("RiskPct needs account equity and stop distance"),
		    Size::Fraction(_) => todo!("Fraction on enter is nonsence: CL should catch it")
		};
		let signed = match side {Side::Long => lots, Side::Short => -lots};
		
		// Re-average entry basis
		let new_size = self.position_size + signed ;
		if self.position_size == 0.0 {
		    self.avg_entry_price = fill_price;
		    self.entry_time = ctx.q.ts; 
		    self.high_water_mark = fill_price;
		    self.low_water_mark = fill_price;  
		} else {
		    self.avg_entry_price = (self.position_size * self.avg_entry_price + signed * fill_price) / new_size 
		}
		self.position_size = new_size;
		self.num_entries += 1;
		ApplyResult {realized: 0.0, closed_out: false}
	    },
	    Action::Exit{size} => {
		if self.position_size == 0.0 { return ApplyResult {realized: 0.0, closed_out: false} } // you can't exit out of a flat position a priori 
		let mut close_lots = match size {
		    Size::Lots(n) => *n, 
		    Size::RiskPct(_) => todo!("RiskPct on Exit is nonsence - Cl should catch this"),
		    Size::Fraction(f) => self.position_size.abs() * f
		};
		close_lots = close_lots.min(self.position_size.abs()); // disallow over-exit for simplicity 

		let realized = (fill_price - self.avg_entry_price) * close_lots *self.position_size.signum();
		let dir = if self.position_size > 0.0 {-1.0} else {1.0}; //current dir 
		self.position_size += dir * close_lots;
		self.num_exits += 1;
		
		// TODO: Realize P&L into account state 
		let closed_out = self.position_size.abs() < 1e-9;
		if closed_out { // garde-fou 
		    self.close_out(); 
		}
		ApplyResult {realized: realized, closed_out: closed_out} 
	    },
	    Action::SetStop {kind, level} => {
		let lvl = eval(level, ctx, self).num();
		self.orders.stop = Some(RestingOrder { 
		    level: lvl, 
		    trailing: match kind {
			StopKind::Fixed => None,
			StopKind::Trailing => Some(level.clone()),
		    }
		});
		ApplyResult {realized: 0.0, closed_out: false}
	    },
	    Action::SetTarget { level } => {
		let lvl = eval(level, ctx, self).num();
		self.orders.target = Some(RestingOrder {level: lvl, trailing:None});
		ApplyResult {realized: 0.0, closed_out: false}
	    },
	    Action::CancelStop => {self.orders.stop = None; ApplyResult {realized: 0.0, closed_out: false}},
	    Action::CancelTarget => {self.orders.target = None; ApplyResult {realized: 0.0, closed_out: false}},
	}
    }
    */

