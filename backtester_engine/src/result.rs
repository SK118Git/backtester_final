use crate::fill::FillReason;
use crate::portfolio::Portfolio;
use crate::costs::CostConfig;
use crate::state::PositionState;
use serde::Serialize;

use polars::prelude::{DataFrame, NamedFrom, ParquetWriter, Column};
use std::fs::File;
use std::path::Path; 


#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub ts: i64,
    pub price: f64, 
    pub lots: f64, 
    pub cost: f64, 
    pub realized: f64, 
    pub reason: FillReason,
}


#[derive(Debug, Clone, Copy)]
pub struct EquityPoint {
    pub ts: i64, 
    pub equity: f64, 
}

#[derive(Debug, Clone, Serialize)]
pub struct RunManifest {
    // Versioned Inputs
    pub data_snapshot_id: String, 
    pub strategy_hash: String, // TODO 
    pub config: RunConfig,
    pub cost_model_version: String, // TODO

    // Summary at the end of run 
    pub final_equity: f64,
    pub num_trades: u32, 
    pub num_configs_evaluated: u32, 
}


#[derive(Debug, Clone, Serialize)]
pub struct RunConfig {
    pub instrument: String,
    pub starting_capital: f64, 
    pub cost: CostConfig, 
    pub equity_epsilon: f64,
    // TODO: date range?
}

#[derive(Debug)]
pub struct BacktestResult {
    pub trades:Vec<TradeRecord>,
    pub equity_curve: Vec<EquityPoint>,
    pub manifest: RunManifest,
    pub final_portfolio: Portfolio,
    pub final_position: PositionState, 
}

impl BacktestResult {
    pub fn save_parquet(&self, dir: &std::path::Path) ->Result<(), Box<dyn std::error::Error>> {
	std::fs::create_dir_all(dir)?;
	let mut trades = DataFrame::new_infer_height(vec![
	    Column::new("ts".into(), self.trades.iter().map(|t| t.ts).collect::<Vec<_>>()),
	    Column::new("price".into(), self.trades.iter().map(|t| t.price).collect::<Vec<_>>()),
	    Column::new("lots".into(), self.trades.iter().map(|t| t.lots).collect::<Vec<_>>()),
	    Column::new("cost".into(), self.trades.iter().map(|t| t.cost).collect::<Vec<_>>()),
	    Column::new("realized".into(), self.trades.iter().map(|t| t.realized).collect::<Vec<_>>()),
	    Column::new("reason".into(), self.trades.iter().map(|t| format!("{:?}", t.reason)).collect::<Vec<_>>()),
	])?;
	
	ParquetWriter::new(File::create(dir.join("trades.parquet"))?).finish(&mut trades)?;
	
	let mut equity = DataFrame::new_infer_height(vec![
	    Column::new("ts".into(), self.equity_curve.iter().map(|e| e.ts).collect::<Vec<_>>()),
	    Column::new("equity".into(), self.equity_curve.iter().map(|e| e.equity).collect::<Vec<_>>()),
	])?;
	
	ParquetWriter::new(File::create(dir.join("equity.parquet"))?).finish(&mut equity)?;

	Ok(())
    }
    

    pub fn save_json(&self, path:&Path) -> Result<(), Box<dyn std::error::Error>> {
	let f = File::create(path)?;
	serde_json::to_writer_pretty(f, &self.manifest)?;
	Ok(())
    }
}
