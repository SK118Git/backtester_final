// mod data;
// mod strategy;
// mod backtester;
mod ingest;
mod ir;
mod state;
mod engine;
mod costs;
mod fill;
mod portfolio;
mod result;
mod backtester;
mod parse;
mod indicator;
mod features;
mod util;



use crate::ir::*;
use crate::costs::CostConfig;
use crate::result::{RunConfig, BacktestResult};
use crate::backtester::run_backtest;
use std::path::Path;
use crate::util::Result;


pub fn run_and_save(strategy:&Strategy, specs: &[FeatureSpec], cfg: &RunConfig, out_dir:&Path) -> Result<BacktestResult> {
    let pair_glob = format!("store/instrument={}/**/*.parquet", cfg.instrument);
    let snapshot_id = ingest::data_snapshot_id(&cfg.instrument, &pair_glob)?;
    let data   = features::load_instrument(&cfg.instrument, &pair_glob, &snapshot_id, specs, cfg.period)?;
    if data.quotes.is_empty() {
	return Err(format!("no ticks for {} in requested period", cfg.instrument).into())
   }
    let result = run_backtest(&data, strategy, cfg, &snapshot_id);         
    result.save_parquet(out_dir)?;
    result.save_json(&out_dir.join("manifest.json"))?;
    Ok(result)
}

const DEMO_STRATEGY: &str = "(strategy (rule (when (and (= (state position-size) 0) (> (px mid) (sma 20)))) (then (set-stop trailing (- (px bid) 0.002)) (enter long (risk-pct 0.01)))) (rule (when (and (> (state position-size) 0) (< (px mid) (sma 20)))) (then (exit all))))";

/*
fn main() -> Result<()>{
    let cfg = RunConfig {
	instrument: "eurusd".to_string(),
	starting_capital: 100_000.0,
	cost: CostConfig::default(),
	equity_epsilon: 1e-6, 
	period: None
    };
    
    let (strategy, specs) = parse::parse(DEMO_STRATEGY).map_err(|e| format!("failed to parse demo strategy: {e:?}"))?;
    
    let result = run_and_save(&strategy, &specs, &cfg, Path::new("results/run1"))?;
    print_summary(&result);
    Ok(())
   }
*/

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
	Some("ingest") => {
	    let csv = args.get(2).ok_or("ingest needs a csv path")?;
	    let ticker = args.get(3).map(String::as_str).unwrap_or("eurusd");
	    ingest::partition_data(csv, ticker)?;
	    println!("ingested {csv} -> store/instrument={ticker}");
	    Ok(())
	}
	Some("run") | None => {
	    let ir_path = args.get(2).map(String::as_str).unwrap_or("strat.ir");
	    let instrument = args.get(3).cloned().unwrap_or_else(|| "eurusd".to_string());
	    let src = std::fs::read_to_string(ir_path)?;
	    let (strategy, specs) = parse::parse(&src).map_err(|e| format!("parse {ir_path}: {e:?}"))?;
	    let cfg = RunConfig{
		instrument, 
		starting_capital: 100_000.0,
		cost: CostConfig::default(),
		equity_epsilon: 1e-6, 
		period: None
	    };
	    let result = run_and_save(&strategy, &specs, &cfg, Path::new("results/run1"))?;
	    print_summary(&result);
	    Ok(())
	}
	Some(other) => Err(format!("unknown subcommand  `{other}`; use `ingest` or `run`").into()),
    }
}

fn build_demo_strategy() -> Strategy { Strategy { rules: vec![ /* ... */ ] } }

fn print_summary(result: &BacktestResult) {
    println!("trades: {}", result.trades.len());
    println!("final equity: {:.6}", result.final_portfolio.equity);
    println!("final position: {}", result.final_position.position_size);
}


#[cfg(test)]
mod test_helpers;
