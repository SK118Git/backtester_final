use polars::prelude::*;
use polars_io::prelude::*;

fn run(start:str, end:str) -> PolarsResult<DataFrame>{
    
}

fn save(mut df: DataFrame, filetype: &str, filename: &str, sep: u8) -> PolarsResult<()> {
    match filetype {
        "csv" => {
            let mut file = std::fs::File::create(format!("{filename}.csv"))?;
            CsvWriter::new(&mut file)
                .with_separator(sep)
                .finish(&mut df)?;
        }
        "parquet" => {
            let mut file = std::fs::File::create(format!("{filename}.parquet"))?;
            ParquetWriter::new(&mut file).finish(&mut df)?;
        }
        _ => {}
    }

    Ok(())
}
