
#[derive(Debug, Clone)]
pub enum Expr {
    Const(f64),
    BoolConst(bool),
    Feature(FeatureId),
    State(StateField), // live: this instrument at this instant 
    Account(AccountField), // frozen snapshot: portfolio-wide, lagged one-instant
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Cond { cond: Box<Expr>, then: Box<Expr>, otherwise: Box<Expr>}
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StateField{
    // stored 
    PositionSize,
    AvgEntryPrice,
    HighWaterMark, 
    LowWaterMark,
    EntryTime,
    NumEntries, 
    NumExits,
    // derived 
    UnrealizedPnl, 
    TimeInTrade,
    TrailingDist
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccountField {
    Equity, 
    Cash, 
    RealizedPnl,
    MarketValue,
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    //Comparison
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    // Logic 
    And,
    Or
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not
}



#[derive(Debug, Clone, Copy)]
pub enum Value {
    Num(f64),
    Bool(bool)
}


impl Value {
    pub fn num(self) -> f64 {
	match self {Value::Num(x) => x, Value::Bool(_) => panic!("Expected a number, not a boolean")}
    }
    pub fn boolean(self) -> bool {
	match self {Value::Bool(b) => b, Value::Num(_) => panic!("Expected a boolean, not a number")}
    }

}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureId(pub usize);

#[derive(Debug, Clone, Copy)]
pub struct Quote {
    pub ts: i64, 
    pub bid: f64, 
    pub ask:f64 
}




#[derive(Debug, Clone)]
pub enum Action {
    Enter { side:Side, size: Size},
    Exit { size:Size },
    SetStop { kind: StopKind, level:Box<Expr>},
    SetTarget {level: Box<Expr>},
    CancelStop,
    CancelTarget
}

#[derive(Debug, Clone, Copy)]
pub enum Side { Long, Short}

#[derive(Debug, Clone, Copy)]
pub enum StopKind {Fixed, Trailing}

#[derive(Debug, Clone, Copy)]
pub enum Size { Lots(f64), Fraction(f64), RiskPct(f64)}

#[derive(Debug, Clone)]
pub struct Rule {
    pub when: Expr, 
    pub then: Vec<Action>
}

#[derive(Debug, Clone)]
pub struct Strategy {
    pub rules: Vec<Rule> // evaluated per tick in list's order, first to match 'when' fires
}
