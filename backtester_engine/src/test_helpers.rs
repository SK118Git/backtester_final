use crate::ir::*;
use crate::state::*;



pub fn tp_expr() -> Expr {
    Expr::Binary(BinOp::Gt, Box::new(Expr::State(StateField::UnrealizedPnl)), Box::new(Expr::Const(0.0020)))
}

pub fn long_at(p:f64) -> PositionState {
    PositionState { position_size: 1.0, avg_entry_price: p, high_water_mark: p,
                    low_water_mark: p, entry_time: 0, num_entries: 1, num_exits: 0,
		    orders: OrderState { stop: None, target:None}
    }
}

pub fn short_at(p:f64) -> PositionState { PositionState { position_size: -1.0, ..long_at(p)}}


pub fn trailing_long_expr(dist:f64) -> Expr {
    Expr::Binary(BinOp::Sub, 
	Box::new(Expr::State(StateField::HighWaterMark)),
	Box::new(Expr::Const(dist))
    )
}
pub fn trailing_short_expr(dist: f64) -> Expr {
    Expr::Binary(BinOp::Add,
        Box::new(Expr::State(StateField::LowWaterMark)),
        Box::new(Expr::Const(dist)))
}

pub fn approx(a:f64, b:f64) -> bool {
    (a-b).abs() < 1e-9
}

pub fn flat() -> PositionState {
    PositionState { position_size: 0.0, avg_entry_price: 0.0, high_water_mark: 0.0, low_water_mark: 0.0, entry_time: 0, num_entries: 0, num_exits: 0, orders: OrderState::default() }
}
