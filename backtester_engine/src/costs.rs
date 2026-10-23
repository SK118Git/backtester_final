
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
