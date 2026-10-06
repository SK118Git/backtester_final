use polars::prelude::*;
use std::collections::HashMap;
use chrono::NaiveDateTime;

// "2020/01/01 00:00:00" -> epoch millis
fn to_millis(time: &str) -> Option<i64> {
    NaiveDateTime::parse_from_str(time, "%Y/%m/%d %H:%M:%S").ok().map(|dt| dt.and_utc().timestamp_millis())
} 


enum DataType{
    Tick,
    Freq,
}


enum SourceName {
    Duckascopy,
    Yfinance,
}

struct TickerData {
    dtype: DataType,
    source: SourceName,
    df: PolarsResult<DataFrame>, 
    ticker: String, 
}

impl TickerData {
    fn resolve_column_names(source:&SourceName) -> HashMap<&'static str, &'static str>{
	match source {
	    SourceName::Duckascopy => HashMap::from([
		("Time", "timestamp"),
		("Ask", "askPrice"),
		("Bid", "bidPrice"),
		("AskVol", "askVolume"),
		("BidVol", "bidVolume"),
	    ]),
	     SourceName::Yfinance => HashMap::from([
                ("Time",   "Date"),
                ("Ask",    "ASK_PX"),
                ("Bid",    "BID_PX"),
                ("AskVol", "VOL_ASK"),
                ("BidVol", "VOL_BID"),
            ]),
	}
    }
    
    fn clean(dtype:&DataType, source:&SourceName, filename:&str) -> PolarsResult<DataFrame>{
	let mapping = Self::resolve_column_names(source);
	// For each (canonical, src col) pick the source column and rename it 
	let selection: Vec<Expr> = mapping.iter().map(|(canonical, src_col)| col(*src_col).alias(*canonical)).collect(); 
	let df = CsvReadOptions::default()
	    .with_has_header(true)
	    .try_into_reader_with_file_path(Some(filename.into()))?
	    .finish()?
	    .lazy()
	    .select(selection)
	    .collect()?;
	Ok(df)
    }


    
    fn new(dtype:DataType, source:SourceName, filename:&str, ticker:String) -> Self {
	let df = Self::clean(&dtype, &source, filename);
	TickerData {dtype, source, df, ticker}
    }
} 


struct MasterData {
    data: HashMap<String, PolarResult<DataFrame>>,
}

impl MasterData {
    fn new(ticker_data:Vec<TickerData>) -> Self {
	let data: HashMap<_, _> = ticker_data
	    .into_iter()
	    .map(|element| (element.ticker, element.df))
	    .collect();
	return MasterData { data } 
    }
}
