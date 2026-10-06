use crate::ir::{StateField, Quote};


pub struct PositionState{
    pub position_size:f64,
    pub avg_entry_price:f64,
    pub high_water_mark:f64, 
    pub low_water_mark:f64,
    pub entry_time:i64,
    pub num_entries:u32,  
    pub num_exits:u32 
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
}
