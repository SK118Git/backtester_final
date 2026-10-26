
#[derive(Debug, Clone, Copy)]
pub enum SlippageModel {
   None, 
   FixedPips(f64),
   // TODO add more 
}


#[derive(Debug, Clone, Copy)]
pub enum FillBias { 
    Adverse, //pessimistic: assume the worst fill 
    Favorable, //optimistic 
    Mid, 
}

#[derive(Debug, Clone, Copy)]
pub struct CostConfig {
    pub slippage: SlippageModel,
    pub fill_bias: FillBias,
    // TODO add more
}

impl Default for CostConfig {
    fn default() -> Self {
	CostConfig { slippage: SlippageModel::None, fill_bias: FillBias::Adverse}
    }
}

pub fn compute_cost(lots: f64, price:f64, cfg:&CostConfig) -> f64 {
    match cfg.slippage {
	SlippageModel::None => 0.0,
	SlippageModel::FixedPips(pips) => pips * lots.abs()
    }
}
