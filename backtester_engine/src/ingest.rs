use polars::prelude::*;
use serde::Serialize;
use sha2::{Sha256, Digest};


pub fn ingest(filename:&str)-> PolarsResult<LazyFrame>{
    // Declaring and Using the schema 
    let mut schema = Schema::default();
    schema.with_column("timestamp".into(), DataType::Int64);
    schema.with_column("askPrice".into(), DataType::Float64);
    schema.with_column("bidPrice".into(), DataType::Float64);
    schema.with_column("askVolume".into(), DataType::Float64);
    schema.with_column("bidVolume".into(), DataType::Float64);

    let mut lf = LazyCsvReader::new(filename.into())
	.with_has_header(true)
	.with_schema(Some(Arc::new(schema)))
	.finish()?;
    
    // Cleaning up the rows and such 
    lf = lf.select([
	col("timestamp")
	    .cast(DataType::Datetime(TimeUnit::Milliseconds, None))
	    .alias("ts"),
	col("bidPrice").alias("bid"),
	col("askPrice").alias("ask"),
	col("bidVolume").alias("bid_vol"),
	col("askVolume").alias("ask_vol")
    ]);
    
    Ok(lf)
}


pub fn partition_data(filename:&str, ticker:&str) -> Result<(), Box<dyn std::error::Error>> {
    let lf = ingest(filename)?
	.with_columns([
	    col("ts").dt().year().alias("year"),
	    col("ts").dt().month().alias("month"),
	    lit(ticker.to_lowercase()).alias("instrument"),
	]);
    let clr = CleaningReport::new(lf.clone())?;
    debug_assert_eq!(clr.n_crossed + clr.n_not_crossed, clr.n_rows);
    clr.save(ticker)?;
    lf.sink(
	SinkDestination::Partitioned {
	    base_path: "store".into(),
	    file_path_provider:None,
	    partition_strategy: PartitionStrategy::Keyed {
		keys: vec![col("instrument"), col("year"), col("month")],
		include_keys:false, // values live in path
		keys_pre_grouped:true, //ts sorted already 
	    },
	    max_rows_per_file: 10_000_000,
	    approximate_bytes_per_file:0, //no byte cap 
	},
	FileWriteFormat::Parquet(Arc::new(ParquetWriteOptions::default())),
	UnifiedSinkArgs {mkdir: true, maintain_order:true, ..Default::default()},
    )?.collect()?;
    /*
    let df = LazyFrame::scan_parquet(
	"store/**/*.parquet".into(), 
	ScanArgsParquet {hive_options:HiveOptions {enabled:Some(true), ..Default::default()}, ..Default::default()}
    )?.limit(10).collect()?;
    println!("{}", df);
    */
    Ok(())
}


#[derive(Debug, Serialize)]
struct CleaningReport {
    n_rows: u32,
    first:i64, 
    last:i64, 
    median_spread:f64, 
    p95_spread:f64,
    n_crossed:u32,
    n_not_crossed:u32
}

impl CleaningReport {
    fn new(lf:LazyFrame)->Result<CleaningReport, Box<dyn std::error::Error>> {
	let df = lf.select([ 
	    len().alias("n_rows"),
	    col("ts").min().alias("first_ts"),
	    col("ts").max().alias("last_ts"),
	    (col("ask") - col("bid")).median().alias("median_spread"),
	    (col("ask") - col("bid")).quantile(lit(0.95), QuantileMethod::Nearest).alias("p95_spread"),
	    col("bid").gt_eq(col("ask")).sum().alias("n_crossed"),
	    col("bid").lt(col("ask")).sum().alias("n_not_crossed"),
	]).collect()?;
	Ok(CleaningReport { 
	    n_rows: df.column("n_rows")?.u32()?.get(0).expect("Less than one row when creating cleaning report"),
	    first: df.column("first_ts")?.to_physical_repr().i64()?.get(0).expect("Less than one row when creating cleaning report"),
	    last: df.column("last_ts")?.to_physical_repr().i64()?.get(0).expect("Less than one row when creating cleaning report"),
	    median_spread: df.column("median_spread")?.f64()?.get(0).expect("Less than one row when creating cleaning report"),
	    p95_spread: df.column("p95_spread")?.f64()?.get(0).expect("Less than one row when creating cleaning report"),
	    n_crossed: df.column("n_crossed")?.u32()?.get(0).expect("Less than one row when creating cleaning report"),
	    n_not_crossed: df.column("n_not_crossed")?.u32()?.get(0).expect("Less than one row when creating cleaning report"),
	})
    }
    fn save(&self, ticker:&str) -> Result<(), Box<dyn std::error::Error>> {
	let hash = self.snapshot_id(ticker);
	let path = format!("clrep/{ticker}_{hash}.json");
	std::fs::create_dir_all("clrep")?;
	let file = std::fs::File::create(path)?;
	serde_json::to_writer_pretty(file, &self)?;
	Ok(())
    }

    fn snapshot_id(&self, ticker:&str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(ticker.as_bytes());
	hasher.update(&self.n_rows.to_le_bytes());
	hasher.update(&self.first.to_le_bytes());
	hasher.update(&self.last.to_le_bytes());
	hasher.update(&self.median_spread.to_bits().to_le_bytes());
	hasher.update(&self.p95_spread.to_bits().to_le_bytes());
	hasher.update(&self.n_crossed.to_le_bytes());
	// no n_not_crossed because it carries no independent info 
	let digest = hasher.finalize();
	hex::encode(digest)
    }
}
