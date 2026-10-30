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


use crate::ir::*;
use crate::costs::CostConfig;
use crate::result::{RunConfig, BacktestResult};
use crate::backtester::run_backtest;
use crate::ingest::load_ticks;
use std::path::Path;


pub fn run_and_save(data_glob: &str, strategy:&Strategy, cfg: &RunConfig, out_dir:&Path) -> Result<BacktestResult, Box<dyn std::error::Error>> {
    let ticks = load_ticks(data_glob)?;
    if ticks.is_empty() {
	return Err(format!("No ticks loaded from {data_glob}").into());
    }
    let result = run_backtest(&ticks, strategy, cfg);
    result.save_parquet(out_dir)?;
    result.save_json(&out_dir.join("manifest.json"))?;
    Ok(result)
}

fn main() -> Result<(), Box<dyn std::error::Error>>{
    
    //ingest::ingest("eurusd/eurusd-tick-2020-01-01-2021-01-01.csv")?;
    //ingest::partition_data("raw_data/eurusd/eurusd-tick-2020-01-01-2021-01-01.csv","eurusd")?;
    //Ok(())
    
    let cfg = RunConfig {
	instrument: "eurusd".to_string(),
	starting_capital: 100_000.0,
	cost: CostConfig::default(),
	equity_epsilon: 1e-6, 
    };
    let strategy = build_demo_strategy();
    let result = run_and_save("store/instrument/year=2020/**/*.parquet", &strategy, &cfg, Path::new("results/run1"))?;
    print_summary(&result);
    Ok(())
   }

fn build_demo_strategy() -> Strategy { Strategy { rules: vec![ /* ... */ ] } }

fn print_summary(result: &BacktestResult) {
    println!("trades: {}", result.trades.len());
    println!("final equity: {:.6}", result.final_portfolio.equity);
    println!("final position: {}", result.final_position.position_size);
}


#[cfg(test)]
mod test_helpers;
