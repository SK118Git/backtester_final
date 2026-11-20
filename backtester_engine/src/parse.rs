use crate::ir::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

#[derive(Debug)]
pub enum ParseError {
    Empty, 
    UnexpectedEoF,
    UnexpectedCloseParen,
    TrailingTokens,
    Lower(String),
    // etc.
}


/// Parsing 


pub fn tokenize(src: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for c in src.chars() {
	match c {
	    '(' | ')' => {
		if !cur.is_empty() {tokens.push(std::mem::take(&mut cur));}
		tokens.push(c.to_string());
	    }
	    _ if c.is_whitespace() => {
		if !cur.is_empty() {tokens.push(std::mem::take(&mut cur));}
	    }
	    _ => cur.push(c),
	}
    }
    if !cur.is_empty() {tokens.push(cur);}
    tokens
}

fn parse_sexp(tokens: &[String], pos: &mut usize) -> Result<Sexp, ParseError> {
    let tok = tokens.get(*pos).ok_or(ParseError::UnexpectedEoF)?;
    *pos += 1;
    match tok.as_str() {
	"(" => {
	    let mut items = Vec::new();
	    loop {
		match tokens.get(*pos).map(String::as_str) {
		    Some(")") => {*pos += 1; return Ok(Sexp::List(items))}
		    None => return Err(ParseError::UnexpectedEoF),
		    Some(_) => items.push(parse_sexp(tokens, pos)?),
		}
	    }
	}
	")" => Err(ParseError::UnexpectedCloseParen),
	_ => Ok(Sexp::Atom(tok.clone())),
    }
}

pub fn parse_sexp_str(src: &str) -> Result<Sexp, ParseError> {
    let tokens = tokenize(src);
    if tokens.is_empty() {return Err(ParseError::Empty);}
    let mut pos = 0;
    let sexp = parse_sexp(&tokens, &mut pos)?;
    if pos != tokens.len() {return Err(ParseError::TrailingTokens);}
    Ok(sexp)
}


#[test]
fn test_parser() {
    let src = "(strategy (rule (when (> (px mid) (sma 20))) (then (exit all))))";
    println!("{:#?}", parse_sexp_str(src));
}

/// Typed lowering 

fn as_list(s: &Sexp) -> Result<&[Sexp], ParseError> {
    match s {
	Sexp::List(v) => Ok(v),
	Sexp::Atom(a) => Err(ParseError::Lower(format!("expected a list, got an atom `{a}`")))
    }
    
}

fn head(items: &[Sexp]) -> Result<&str, ParseError> {
    match items.first() {
	Some(Sexp::Atom(a)) => Ok(a.as_str()),
	_ => Err(ParseError::Lower("list has no head keyword".into()))
    }
}

fn atom(s: &Sexp) -> Result<&str, ParseError> { 
    match s {
	Sexp::Atom(a) => Ok(a.as_str()),
	Sexp::List(_) => Err(ParseError::Lower("expected an atom got a list".into()))
    }
}


fn number(s: &Sexp) -> Result<f64, ParseError> {
    let a = atom(s)?;
    a.parse::<f64>().map_err(|_| ParseError::Lower(format!("expected a number, got `{a}`")))
}

fn arg<'a>(args: &'a [Sexp], n: usize) -> Result<&'a Sexp, ParseError> {
    args.get(n).ok_or_else(|| ParseError::Lower(format!("missing argument {n}")))
}


/// Match to struct

fn quote_field(a:&str) -> Result<QuoteField, ParseError> {
    Ok(match a {
	"bid"=>QuoteField::Bid,
	"ask"=>QuoteField::Ask, 
	"mid"=>QuoteField::Mid, 
	"spread"=>QuoteField::Spread,
	"bid-vol"=>QuoteField::BidVol,
	"ask-vol"=>QuoteField::AskVol,
	_ => return Err(ParseError::Lower(format!("unknown quote field `{a}`")))
    })
}

fn state_field(a:&str) -> Result<StateField, ParseError> {
    Ok(match a {
	"position-size"=>StateField::PositionSize,
	"avg-entry-price"=>StateField::AvgEntryPrice,
	"high-water-mark"=>StateField::HighWaterMark,
	"low-water-mark"=>StateField::LowWaterMark,
	"entry-time"=>StateField::EntryTime,
	"num-entries"=>StateField::NumEntries,
	"num-exits"=>StateField::NumExits,
	"unrealized-pnl"=>StateField::UnrealizedPnl,
	"time-in-trade"=>StateField::TimeInTrade,
	"trailing-dist"=>StateField::TrailingDist, 
	_ => return Err(ParseError::Lower(format!("unknown state field `{a}`")))
    })
}

fn account_field(a: &str) -> Result<AccountField, ParseError>{
    Ok(match a {
	"equity" => AccountField::Equity,
	"cash" => AccountField::Cash, 
	"realized-pnl" => AccountField::RealizedPnl, 
	"market-value" => AccountField::MarketValue,
	_ => return Err(ParseError::Lower(format!("unknown account field `{a}`")))
    })
}

fn cmp_op(op: &str) -> BinOp {
    match op {
	">" => BinOp::Gt,
	">=" => BinOp::Ge,
	"=" => BinOp::Eq,
	"<=" => BinOp::Le,
	"<" => BinOp::Lt,
	_ => unreachable!()
    }
}


fn assoc_op(op: &str) -> BinOp {
    match op {
	"and" => BinOp::And,
	"or" => BinOp::Or,
	"+" => BinOp::Add,
	"*" => BinOp::Mul,
	_ => unreachable!()
    }
}


fn lower_variadic(op: BinOp, args: &[Sexp], feats: &mut Features) -> Result<Expr, ParseError> {
    let exprs = args.iter().map(|s| lower_expr(s, feats)).collect::<Result<Vec<_>, _>>()?;
    let mut it = exprs.into_iter();
    let first = it.next().ok_or_else(|| ParseError::Lower("variadic op with no operands".into()))?;
    Ok(it.fold(first, |acc, e| Expr::Binary(op, Box::new(acc), Box::new(e))))
}

fn lower_arg(args: &[Sexp], n:usize, feats: &mut Features) -> Result<Box<Expr>, ParseError> {
    Ok(Box::new(lower_expr(arg(args,n)?, feats)?))
}

fn atom_arg<'a>(args: &'a [Sexp], n:usize) -> Result<&'a str, ParseError> {
    atom(arg(args, n)?)
}

fn lower_expr(s: &Sexp, feats: &mut Features) -> Result<Expr, ParseError> {
    match s {
	Sexp::Atom(a) => match a.as_str() {
	    "true" => Ok(Expr::BoolConst(true)),
	    "false" => Ok(Expr::BoolConst(false)),
	    _ => a.parse::<f64>().map(Expr::Const).map_err(|_| ParseError::Lower(format!("unbound symbol in expression: `{a}`")))
	},
	Sexp::List(items) => {
	    let h = head(items)?;
	    let args = &items[1..];
	    match h {
		"px" => Ok(Expr::Feature(feats.intern(FeatureSpec::Quote(quote_field(atom_arg(args, 0)?)?)))),
		"state" => Ok(Expr::State(state_field(atom_arg(args, 0)?)?)),
		"account" => Ok(Expr::Account(account_field(atom_arg(args, 0)?)?)),
		"signal" => Ok(Expr::Feature(feats.intern(FeatureSpec::Signal(atom_arg(args, 0)?.to_string())))),
		"if" => Ok(Expr::Cond {
		    cond: lower_arg(args, 0, feats)?,
		    then: lower_arg(args, 1, feats)?,
		    otherwise: lower_arg(args, 2, feats)?,
		}),
		"not" => Ok(Expr::Unary(UnOp::Not, lower_arg(args, 0, feats)?)),
		">" | ">=" | "=" | "<=" | "<" => Ok(Expr::Binary(cmp_op(h), lower_arg(args, 0, feats)?, lower_arg(args, 1, feats)?)),
		"and" | "or" | "+" | "*" => lower_variadic(assoc_op(h), args, feats),
		"-" => if args.len() == 1 {
		    Ok(Expr::Unary(UnOp::Neg, lower_arg(args, 0, feats)?))
		} else {
		    Ok(Expr::Binary(BinOp::Sub, lower_arg(args, 0, feats)?, lower_arg(args, 1, feats)?))
		},
		"/" => Ok(Expr::Binary(BinOp::Div, lower_arg(args, 0, feats)?, lower_arg(args, 1, feats)?)),
		name => {
		    let nums = args.iter().map(number).collect::<Result<Vec<f64>, _>>()?;
		    Ok(Expr::Feature(feats.intern(FeatureSpec::Indicator { name: name.to_string(), args: nums })))
		}
			       
	    }
	}
    }
}


fn side(a: &str) -> Result<Side, ParseError> {
    Ok(match a {
	"long" => Side::Long,
	"short" => Side::Short,
	_ => return Err(ParseError::Lower(format!("bad side `{a}`")))
    })
}

fn stop_kind(a: &str) -> Result<StopKind, ParseError> {
    Ok(match a {
	"fixed"=> StopKind::Fixed,
	"trailing" => StopKind::Trailing,
	_ => return Err(ParseError::Lower(format!("bad stop kind `{a}`")))
    })
}

fn lower_size(s: &Sexp) -> Result<Size, ParseError> {
    let items = as_list(s)?;
    let h = head(items)?;
    let amount = number(arg(&items[1..],0)?)?;
    Ok(match h {
	"units" => Size::Lots(amount),
	"fraction" => Size::Fraction(amount),
	"risk-pct" => Size::RiskPct(amount),
	_ => return Err(ParseError::Lower(format!("unknown size `{h}`")))
    })
}

fn lower_action(s: &Sexp, feats: &mut Features) -> Result<Action, ParseError> {
    let items = as_list(s)?;
    let h = head(items)?;
    let args = &items[1..];
    match h {
	"enter" => Ok(Action::Enter { 
	    side: side(atom_arg(args, 0)?)?, 
	    size: lower_size(arg(args,1)?)? 
	}),
	"exit" => {
	    let what = arg(args, 0)?;
	    match what {
		Sexp::Atom(a) if a == "all" => Ok(Action::Exit { size: Size::Fraction(1.0) }),
		_ => Ok(Action::Exit { size: lower_size(what)? })
	    }
	},
	"set-stop" => Ok(Action::SetStop { 
	    kind: stop_kind(atom_arg(args, 0)?)?, 
	    level: lower_arg(args, 1, feats)? 
	}),
	"set-target" => Ok(Action::SetTarget { level: lower_arg(args, 0, feats)? }),
	"cancel-stop" => Ok(Action::CancelStop),
	"cancel-target" => Ok(Action::CancelTarget),
	_ => Err(ParseError::Lower(format!("unknown action `{h}`")))
    }
}


fn lower_rule(s: &Sexp, feats: &mut Features) -> Result<Rule, ParseError> {
    let items = as_list(s)?;
    let when_clause = as_list(arg(items, 1)?)?;
    let when = lower_expr(arg(when_clause, 1)?, feats)?;
    let then_clause = as_list(arg(items, 2)?)?;
    let then = then_clause[1..].iter().map(|a| lower_action(a, feats)).collect::<Result<Vec<_>, _>>()?;
    Ok(Rule {when, then})
}

fn lower_strategy(s: &Sexp, feats: &mut Features) -> Result<Strategy, ParseError> {
    let items = as_list(s)?;
    let rules = items[1..].iter().map(|r| lower_rule(r, feats)).collect::<Result<Vec<_>,_>>()?;
    Ok(Strategy { rules })
}


pub fn parse(src: &str) -> Result<(Strategy, Vec<FeatureSpec>), ParseError> {
    let sexp = parse_sexp_str(src)?;
    let mut feats = Features::default();
    let strategy = lower_strategy(&sexp, &mut feats)?;
    Ok((strategy, feats.specs))
}


#[test]
fn parses_breakout() {
    let src = "(strategy (rule (when (and (= (state position-size) 0) (> (px mid) (sma 20)))) (then (set-stop trailing (- (px bid) 0.002)) (enter long (risk-pct 0.01)))) (rule (when (and (> (state position-size) 0) (< (px mid) (sma 20)))) (then (exit all))))";
    let (strat, feats) = parse(src).expect("should parse");
    println!("rules: {}", strat.rules.len());
    println!("features: {:#?}", feats);
    assert_eq!(strat.rules.len(), 2);
}
