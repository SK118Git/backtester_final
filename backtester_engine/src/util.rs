use polars::prelude::*;

pub fn hive() -> ScanArgsParquet {
	ScanArgsParquet { 
	    hive_options: HiveOptions { enabled: Some(true), ..Default::default() },		     
	    ..Default::default()}
}


pub type DynError = Box<dyn std::error::Error + Send + Sync>; 
pub type Result<T> = std::result::Result<T, DynError>;

