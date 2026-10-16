use crate::ir::*;
use crate::engine::{eval};


#[cfg(test)]
use crate::test_helpers::*;

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
    pub fn update_trailing(&mut self, q:&Quote, features:&[f64]) {
	if self.position_size == 0.0 {return;}
	let is_long = self.position_size > 0.0;
	let stop_candidate = self.orders.stop.as_ref() 
	    .and_then(|s| s.trailing.as_ref())
	    .map(|expr| eval(expr, q, features, self).num());
	if let (Some(stop), Some(cand)) = (&mut self.orders.stop, stop_candidate) {
	    stop.level = if is_long {stop.level.max(cand)} else {stop.level.min(cand)};
	}
    }
    
    pub fn apply(&mut self, action:&Action, fill_price:f64, q:&Quote, features:&[f64]) {
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
		    self.entry_time = q.ts; 
		    self.high_water_mark = fill_price;
		    self.low_water_mark = fill_price;  
		} else {
		    self.avg_entry_price = (self.position_size * self.avg_entry_price + signed * fill_price) / new_size 
		}
		self.position_size = new_size;
		self.num_entries += 1
	    },
	    Action::Exit{size} => {
		if self.position_size == 0.0 { return;} // you can't exit out of a flat position a priori 
		let mut close_lots = match size {
		    Size::Lots(n) => *n, 
		    Size::RiskPct(_) => todo!("RiskPct on Exit is nonsence - Cl should catch this"),
		    Size::Fraction(f) => self.position_size.abs() * f
		};
		close_lots = close_lots.min(self.position_size.abs()); // disallow over-exit for simplicity 
		let dir = if self.position_size > 0.0 {-1.0} else {1.0}; //current dir 
		self.position_size += dir * close_lots;
		self.num_exits += 1;
		
		// TODO: Realize P&L into account state 

		if self.position_size.abs() < 1e-9 { // garde-fou 
		    self.close_out(); 
		}
	    },
	    Action::SetStop {kind, level} => {
		let lvl = eval(level, q, features, self).num();
		self.orders.stop = Some(RestingOrder { 
		    level: lvl, 
		    trailing: match kind {
			StopKind::Fixed => None,
			StopKind::Trailing => Some(level.clone()),
		    }
		});
	    },
	    Action::SetTarget { level } => {
		let lvl = eval(level, q, features, self).num();
		self.orders.target = Some(RestingOrder {level: lvl, trailing:None});
	    },
	    Action::CancelStop => self.orders.stop = None,
	    Action::CancelTarget => self.orders.target = None, 
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
}



pub struct RestingOrder{
    // The current effective stop price, frozen for a Fixed and moving for Trailing
    pub level: f64, 
    // For trailing: the expr we use to re-evaluate the level
    pub trailing: Option<Box<Expr>>
}

#[derive(Default)]
pub struct OrderState {
    pub stop: Option<RestingOrder>, 
    pub target: Option<RestingOrder>
}





// Tests 

#[test]
fn long_stop_ratchets_up() {
    let mut s = long_at(1.1000);
    s.high_water_mark = 1.1030;
    s.orders.stop = Some(RestingOrder { level: 1.1015, trailing: Some(Box::new(trailing_long_expr(0.0010)))});
    let q = Quote { ts:1, bid:1.1030, ask:1.1032};
    s.update_trailing(&q, &[]);
    assert!(approx(s.orders.stop.unwrap().level, 1.1020), "stop should ratchet up to new candidate");
}

#[test]
fn long_stop_never_drops() {
    let mut s = long_at(1.1000);
    s.high_water_mark = 1.1018;
    s.orders.stop = Some(RestingOrder { level: 1.1015, trailing: Some(Box::new(trailing_long_expr(0.0010)))});
    let q = Quote { ts:1, bid:1.1018, ask:1.1020};
    s.update_trailing(&q, &[]);
    assert!(approx(s.orders.stop.unwrap().level, 1.1015), "stop should ratchet up to new candidate");

}
#[test]
fn short_stop_ratchets_down() {
    let mut s = short_at(1.1000);                     // position_size negative
    s.low_water_mark = 1.0970;                        // candidate = 1.0970 + 0.0010 = 1.0980
    s.orders.stop = Some(RestingOrder {
        level: 1.0985,                                // currently ABOVE the candidate
        trailing: Some(Box::new(trailing_short_expr(0.0010))),
    });
    let q = Quote { ts: 1, bid: 1.0968, ask: 1.0970 };
    s.update_trailing(&q, &[]);
    // short ratchets down: min(1.0985, 1.0980) = 1.0980 — stop tightened
    assert!(approx(s.orders.stop.unwrap().level, 1.0980), "short stop should ratchet down");
}

#[test]
fn short_stop_never_rises() {
    let mut s = short_at(1.1000);
    s.low_water_mark = 1.0982;                        // candidate = 1.0982 + 0.0010 = 1.0992
    s.orders.stop = Some(RestingOrder {
        level: 1.0985,                                // currently BELOW the candidate
        trailing: Some(Box::new(trailing_short_expr(0.0010))),
    });
    let q = Quote { ts: 1, bid: 1.0980, ask: 1.0982 };
    s.update_trailing(&q, &[]);
    // min(1.0985, 1.0992) = 1.0985 — must NOT loosen upward
    assert!(approx(s.orders.stop.unwrap().level, 1.0985), "short stop must NOT rise");
}

#[test]
fn flat_position_does_not_ratchet() {
    let mut s = long_at(1.1000);
    s.position_size = 0.0;                            // flat — guard should bail
    s.high_water_mark = 1.1050;                       // would move the stop a lot if unguarded
    s.orders.stop = Some(RestingOrder {
        level: 1.1015,
        trailing: Some(Box::new(trailing_long_expr(0.0010))),
    });
    let q = Quote { ts: 1, bid: 1.1050, ask: 1.1052 };
    s.update_trailing(&q, &[]);
    assert!(approx(s.orders.stop.unwrap().level, 1.1015), "no ratchet while flat");
}



#[test]
fn first_entry_sets_basis_to_fill() {
    let mut s = flat();
    let q = Quote { ts: 500, bid: 1.0998, ask: 1.1000 };
    // Enter long 2 lots; fills.rs would price this at the ask (1.1000).
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(2.0) },
            1.1000, &q, &[]);
    assert!(approx(s.position_size, 2.0));
    assert!(approx(s.avg_entry_price, 1.1000), "first entry basis = fill price");
    assert_eq!(s.num_entries, 1);
    assert_eq!(s.entry_time, 500, "entry_time set on first entry");
    assert!(approx(s.high_water_mark, 1.1000));
}

#[test]
fn scale_in_reaverages_basis() {
    let mut s = flat();
    let q = Quote { ts: 1, bid: 0.0, ask: 0.0 };
    // 2 lots @ 1.1000, then 2 lots @ 1.1020 → avg should be 1.1010.
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(2.0) }, 1.1000, &q, &[]);
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(2.0) }, 1.1020, &q, &[]);
    assert!(approx(s.position_size, 4.0));
    assert!(approx(s.avg_entry_price, 1.1010), "equal-size scale-in averages the two fills");
    assert_eq!(s.num_entries, 2);
    assert_eq!(s.entry_time, 1, "entry_time stays at FIRST entry, not reset on add");
}

#[test]
fn weighted_scale_in() {
    let mut s = flat();
    let q = Quote { ts: 1, bid: 0.0, ask: 0.0 };
    // 1 lot @ 1.1000, then 3 lots @ 1.1040 → avg = (1·1.1000 + 3·1.1040)/4 = 1.1030.
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(1.0) }, 1.1000, &q, &[]);
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(3.0) }, 1.1040, &q, &[]);
    assert!(approx(s.avg_entry_price, 1.1030), "basis is size-weighted, not a plain mean");
}

#[test]
fn partial_exit_reduces_size_keeps_basis() {
    let mut s = flat();
    let q = Quote { ts: 1, bid: 0.0, ask: 0.0 };
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(4.0) }, 1.1000, &q, &[]);
    // Exit half.
    s.apply(&Action::Exit { size: Size::Fraction(0.5) }, 1.1030, &q, &[]);
    assert!(approx(s.position_size, 2.0), "half of 4 lots closed");
    assert!(approx(s.avg_entry_price, 1.1000), "basis unchanged by partial exit");
    assert_eq!(s.num_exits, 1);
    assert!(s.position_size.abs() > 1e-9, "still in a position, not closed out");
}

#[test]
fn full_exit_closes_and_resets() {
    let mut s = flat();
    let q = Quote { ts: 500, bid: 0.0, ask: 0.0 };
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(2.0) }, 1.1000, &q, &[]);
    s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None }); // a resting stop exists
    // Exit the full 2 lots.
    s.apply(&Action::Exit { size: Size::Fraction(1.0) }, 1.1050, &q, &[]);
    assert!(approx(s.position_size, 0.0), "flat after full exit");
    assert!(approx(s.avg_entry_price, 0.0), "basis reset");
    assert!(approx(s.high_water_mark, 0.0), "water marks reset");
    assert_eq!(s.entry_time, 0, "entry_time reset");
    assert!(s.orders.stop.is_none(), "resting orders cleared on close");
}

#[test]
fn exit_while_flat_is_noop() {
    let mut s = flat();
    s.apply(&Action::Exit { size: Size::Fraction(1.0) }, 1.1000,
            &Quote { ts: 1, bid: 0.0, ask: 0.0 }, &[]);
    assert!(approx(s.position_size, 0.0), "exit on flat does nothing");
    assert_eq!(s.num_exits, 0, "guard returns before incrementing");
}

#[test]
fn short_entry_is_negative_size() {
    let mut s = flat();
    let q = Quote { ts: 1, bid: 1.1000, ask: 1.1002 };
    // Short 2 lots; fills.rs prices at the bid (1.1000) — passed as fill_price.
    s.apply(&Action::Enter { side: Side::Short, size: Size::Lots(2.0) }, 1.1000, &q, &[]);
    assert!(approx(s.position_size, -2.0), "short => negative size");
    assert!(approx(s.avg_entry_price, 1.1000));
}

#[test]
fn set_fixed_stop_evaluates_once() {
    let mut s = long_at(1.1000);
    s.high_water_mark = 1.1030;
    // Fixed stop at a literal 1.0980.
    s.apply(&Action::SetStop { kind: StopKind::Fixed, level: Box::new(Expr::Const(1.0980)) },
            0.0, &Quote { ts: 1, bid: 1.1025, ask: 1.1027 }, &[]);
    let stop = s.orders.stop.as_ref().unwrap();
    assert!(approx(stop.level, 1.0980), "fixed stop level evaluated now");
    assert!(stop.trailing.is_none(), "fixed => no stored expr, won't ratchet");
}

#[test]
fn set_trailing_stop_stores_expr() {
    let mut s = long_at(1.1000);
    s.high_water_mark = 1.1030;
    // Trailing stop: high_water - 10 pips = 1.1020 initially.
    s.apply(&Action::SetStop {
                kind: StopKind::Trailing,
                level: Box::new(trailing_long_expr(0.0010)),
            },
            0.0, &Quote { ts: 1, bid: 1.1030, ask: 1.1032 }, &[]);
    let stop = s.orders.stop.as_ref().unwrap();
    assert!(approx(stop.level, 1.1020), "initial level evaluated at registration");
    assert!(stop.trailing.is_some(), "trailing => expr stored for re-eval");
}

#[test]
fn cancel_clears_order() {
    let mut s = long_at(1.1000);
    s.orders.stop = Some(RestingOrder { level: 1.0980, trailing: None });
    s.apply(&Action::CancelStop, 0.0, &Quote { ts: 1, bid: 0.0, ask: 0.0 }, &[]);
    assert!(s.orders.stop.is_none());
}

// Documents CURRENT behavior — over-exit dis-allowed 
#[test]
fn over_exit_clamps_flat() {
    let mut s = flat();
    let q = Quote { ts: 1, bid: 0.0, ask: 0.0 };
    s.apply(&Action::Enter { side: Side::Long, size: Size::Lots(2.0) }, 1.1000, &q, &[]);
    s.apply(&Action::Exit { size: Size::Lots(5.0) }, 1.1000, &q, &[]);
    // 2 - 5 = -3 < 0 Currently clamped. 
    assert!(approx(s.position_size, 0.0), "CURRENT behavior: no over-exit.");
    assert!(approx(s.avg_entry_price, 0.0), "CURRENT behavior: no over-exit.");
}
