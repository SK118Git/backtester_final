use polars::prelude::*;
use crate::ir::{FeatureSpec, QuoteField};


fn mid() -> Expr {
    (col("bid") + col("ask")) * lit(0.5)
}

// Right closed to avoid look ahead risk 
fn window_opts(minutes: f64) -> RollingOptionsDynamicWindow {
    let dur = Duration::parse(&format!{"{}m", minutes as i64});
    RollingOptionsDynamicWindow {
	window_size: dur,
	min_periods: 1,
	closed_window: ClosedWindow::Right, 
	fn_params: None
    }
}

fn minutes_arg(name: &str, args: &[f64]) -> Result<f64, String> {
    args.first().copied().ok_or_else(|| format!("indicator `{name}` needs a period (minutes) argument"))
}

pub fn lower_feature(spec: &FeatureSpec) -> Result<Expr, String> {
    Ok(match spec {
	FeatureSpec::Quote(q) => match q {
	    QuoteField::Bid => col("bid"),
	    QuoteField::Ask => col("ask"),
	    QuoteField::Mid => mid(), 
	    QuoteField::Spread => col("ask") - col("bid"),
	    QuoteField::BidVol => col("bid_vol"),
	    QuoteField::AskVol => col("ask_vol")
	},
	FeatureSpec::Indicator { name, args } => match name.as_str() {
	    // Simple Moving Average of mid 
	    "sma" => {
		let m = minutes_arg("sma", args)?;
		mid().rolling_mean_by(col("ts"), window_opts(m))
	    },
	     // Tick based vol 
	    "vol" => {
		let m = minutes_arg("vol", args)?;
		mid().rolling_std_by(col("ts"), window_opts(m))
	    }
	    
	    "rsi" => {
		let m = minutes_arg("rsi", args)?;
		rsi_expr(m)
	    }

	    "ema" => return Err("`ema` not implemented yet".into()), // TODO 
	    other => return Err(format!("unknown indicator `{other}`"))
	},
	FeatureSpec::Signal(name) => {
	    // External point in time series 
	    // TODO 
	    return Err(format!("signal `{name}` not available yet"))
	}
    })
}


fn rsi_expr(minutes: f64) -> Expr {
    let delta = mid() - mid().shift(lit(1));
    let gain = when(delta.clone().gt(lit(0.0))).then(delta.clone()).otherwise(lit(0.0));
    let loss = when(delta.clone().lt(lit(0.0))).then(-delta).otherwise(lit(0.0));
    let avg_gain = gain.rolling_mean_by(col("ts"), window_opts(minutes));
    let avg_loss = loss.rolling_mean_by(col("ts"), window_opts(minutes));
    
    let rs = avg_gain / (avg_loss + lit(1e-12));
    lit(100.0) - lit(100.0)/(lit(1.0) + rs)
}


pub fn lower_features(specs: &[FeatureSpec]) -> Result<Vec<Expr>, String> {
    specs.iter().map(lower_feature).collect()
}
