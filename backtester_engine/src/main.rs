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


fn main() -> Result<(), Box<dyn std::error::Error>>{
    
    //ingest::ingest("eurusd/eurusd-tick-2020-01-01-2021-01-01.csv")?;
    ingest::partition_data("raw_data/eurusd/eurusd-tick-2020-01-01-2021-01-01.csv","eurusd")?;
    Ok(())
}



#[cfg(test)]
mod test_helpers;
