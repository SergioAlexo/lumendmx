//! The Expression node's formulas: numbers, `+ - * / % ^`, brackets, a
//! few functions and the names of its inputs, e.g. `sin(t * 2) * depth + 0.5`.
//! Every name that isn't a function or constant becomes an input socket.

use std::f32::consts::{E, PI, TAU};

/// Names a formula can't use for an input: they'd clash with the node's own
/// params and sockets.
const RESERVED: [&str; 3] = ["formula", "enabled", "exposed"];

/// Most inputs a formula can have.
pub const MAX_VARS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Sin,
    Cos,
    Abs,
    Sqrt,
    Floor,
    Fract,
    Sign,
    Min,
    Max,
    Pow,
    Step,
    Clamp,
    Mix,
    Smoothstep,
}

impl Func {
    fn named(name: &str) -> Option<(Func, usize)> {
        Some(match name {
            "sin" => (Func::Sin, 1),
            "cos" => (Func::Cos, 1),
            "abs" => (Func::Abs, 1),
            "sqrt" => (Func::Sqrt, 1),
            "floor" => (Func::Floor, 1),
            "fract" => (Func::Fract, 1),
            "sign" => (Func::Sign, 1),
            "min" => (Func::Min, 2),
            "max" => (Func::Max, 2),
            "pow" => (Func::Pow, 2),
            "step" => (Func::Step, 2),
            "clamp" => (Func::Clamp, 3),
            "mix" => (Func::Mix, 3),
            "smoothstep" => (Func::Smoothstep, 3),
            _ => return None,
        })
    }
}

fn constant(name: &str) -> Option<f32> {
    match name {
        "pi" => Some(PI),
        "tau" => Some(TAU),
        "e" => Some(E),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f32),
    Var(usize),
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

/// A formula ready to run: the input names in order of first use, and the
/// expression over them.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub vars: Vec<String>,
    root: Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f32),
    Name(String),
    Sym(char),
}

fn tokenize(text: &str) -> Result<Vec<(usize, Token)>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let literal: String = chars[start..i].iter().collect();
            let value = literal.parse::<f32>().map_err(|_| format!("\"{literal}\" isn't a number"))?;
            tokens.push((start, Token::Num(value)));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push((start, Token::Name(chars[start..i].iter().collect())));
        } else if "+-*/%^(),".contains(c) {
            tokens.push((i, Token::Sym(c)));
            i += 1;
        } else {
            return Err(format!("Unexpected \"{c}\" at {}", i + 1));
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    at: usize,
    vars: Vec<String>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(_, t)| t)
    }

    fn here(&self) -> String {
        match self.tokens.get(self.at) {
            Some((pos, _)) => format!("at {}", pos + 1),
            None => "at the end".to_string(),
        }
    }

    fn eat(&mut self, sym: char) -> bool {
        if self.peek() == Some(&Token::Sym(sym)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, sym: char) -> Result<(), String> {
        if self.eat(sym) {
            Ok(())
        } else {
            Err(format!("Expected \"{sym}\" {}", self.here()))
        }
    }

    fn expression(&mut self) -> Result<Node, String> {
        let mut left = self.term()?;
        loop {
            let op = if self.eat('+') {
                Op::Add
            } else if self.eat('-') {
                Op::Sub
            } else {
                return Ok(left);
            };
            left = Node::Bin(op, Box::new(left), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Result<Node, String> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat('*') {
                Op::Mul
            } else if self.eat('/') {
                Op::Div
            } else if self.eat('%') {
                Op::Rem
            } else {
                return Ok(left);
            };
            left = Node::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    /// Minus binds looser than `^`: -2^2 is -4.
    fn unary(&mut self) -> Result<Node, String> {
        if self.eat('-') {
            return Ok(Node::Neg(Box::new(self.unary()?)));
        }
        self.power()
    }

    fn power(&mut self) -> Result<Node, String> {
        let base = self.atom()?;
        if self.eat('^') {
            return Ok(Node::Bin(Op::Pow, Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Node, String> {
        let here = self.here();
        match self.tokens.get(self.at).map(|(_, t)| t.clone()) {
            Some(Token::Num(v)) => {
                self.at += 1;
                Ok(Node::Num(v))
            }
            Some(Token::Sym('(')) => {
                self.at += 1;
                let inner = self.expression()?;
                self.expect(')')?;
                Ok(inner)
            }
            Some(Token::Name(name)) => {
                self.at += 1;
                if self.peek() == Some(&Token::Sym('(')) {
                    return self.call(&name);
                }
                if let Some(v) = constant(&name) {
                    return Ok(Node::Num(v));
                }
                if Func::named(&name).is_some() {
                    return Err(format!("\"{name}\" is a function: write {name}(…)"));
                }
                if RESERVED.contains(&name.as_str()) {
                    return Err(format!("\"{name}\" can't be an input name"));
                }
                let index = match self.vars.iter().position(|v| *v == name) {
                    Some(i) => i,
                    None if self.vars.len() >= MAX_VARS => return Err(format!("At most {MAX_VARS} inputs")),
                    None => {
                        self.vars.push(name);
                        self.vars.len() - 1
                    }
                };
                Ok(Node::Var(index))
            }
            Some(Token::Sym(c)) => Err(format!("Unexpected \"{c}\" {here}")),
            None => Err("The formula ends too soon".to_string()),
        }
    }

    fn call(&mut self, name: &str) -> Result<Node, String> {
        let Some((func, arity)) = Func::named(name) else {
            return Err(format!("Unknown function \"{name}\""));
        };
        self.expect('(')?;
        let mut args = Vec::new();
        if !self.eat(')') {
            loop {
                args.push(self.expression()?);
                if self.eat(')') {
                    break;
                }
                self.expect(',')?;
            }
        }
        if args.len() != arity {
            return Err(format!("{name} takes {arity} value{}, not {}", if arity == 1 { "" } else { "s" }, args.len()));
        }
        Ok(Node::Call(func, args))
    }
}

impl Expr {
    pub fn parse(text: &str) -> Result<Expr, String> {
        let tokens = tokenize(text)?;
        if tokens.is_empty() {
            return Err("Write a formula, like a * b + 0.5".to_string());
        }
        let mut parser = Parser { tokens, at: 0, vars: Vec::new() };
        let root = parser.expression()?;
        if parser.at < parser.tokens.len() {
            return Err(format!("Unexpected input {}", parser.here()));
        }
        Ok(Expr { vars: parser.vars, root })
    }

    /// The value with each input name's current value in `vars` (same order as `self.vars`).
    pub fn eval(&self, vars: &[f32]) -> f32 {
        let v = run(&self.root, vars);
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }
}

fn run(node: &Node, vars: &[f32]) -> f32 {
    match node {
        Node::Num(v) => *v,
        Node::Var(i) => vars.get(*i).copied().unwrap_or(0.0),
        Node::Neg(a) => -run(a, vars),
        Node::Bin(op, a, b) => {
            let (a, b) = (run(a, vars), run(b, vars));
            match op {
                Op::Add => a + b,
                Op::Sub => a - b,
                Op::Mul => a * b,
                Op::Div => if b == 0.0 { 0.0 } else { a / b },
                Op::Rem => if b == 0.0 { 0.0 } else { a.rem_euclid(b) },
                Op::Pow => a.powf(b),
            }
        }
        Node::Call(func, args) => {
            let a: Vec<f32> = args.iter().map(|n| run(n, vars)).collect();
            match func {
                Func::Sin => a[0].sin(),
                Func::Cos => a[0].cos(),
                Func::Abs => a[0].abs(),
                Func::Sqrt => a[0].max(0.0).sqrt(),
                Func::Floor => a[0].floor(),
                Func::Fract => a[0] - a[0].floor(),
                Func::Sign => if a[0] == 0.0 { 0.0 } else { a[0].signum() },
                Func::Min => a[0].min(a[1]),
                Func::Max => a[0].max(a[1]),
                Func::Pow => a[0].powf(a[1]),
                Func::Step => if a[1] >= a[0] { 1.0 } else { 0.0 },
                Func::Clamp => a[0].max(a[1]).min(a[2]),
                Func::Mix => a[0] + (a[1] - a[0]) * a[2],
                Func::Smoothstep => {
                    let span = a[1] - a[0];
                    if span == 0.0 {
                        if a[2] >= a[0] { 1.0 } else { 0.0 }
                    } else {
                        let t = ((a[2] - a[0]) / span).clamp(0.0, 1.0);
                        t * t * (3.0 - 2.0 * t)
                    }
                }
            }
        }
    }
}

/// The input names a formula mentions, in order of first use, even if the
/// formula doesn't parse: sockets (and links to them) stay put while it is
/// being typed.
pub fn input_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut names: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' {
            // A number, including a stray letter after it (1e3).
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            let is_call = chars[i..].iter().find(|c| !c.is_whitespace()) == Some(&'(');
            let known = is_call || constant(&name).is_some() || Func::named(&name).is_some() || RESERVED.contains(&name.as_str());
            if !known && !names.contains(&name) && names.len() < MAX_VARS {
                names.push(name);
            }
        } else {
            i += 1;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, vars: &[f32]) -> f32 {
        Expr::parse(text).unwrap().eval(vars)
    }

    #[test]
    fn arithmetic_follows_the_usual_precedence() {
        assert_eq!(run("1 + 2 * 3", &[]), 7.0);
        assert_eq!(run("(1 + 2) * 3", &[]), 9.0);
        assert_eq!(run("10 - 4 - 3", &[]), 3.0);
        assert_eq!(run("2 ^ 3 ^ 2", &[]), 512.0); // right to left
        assert_eq!(run("-2 ^ 2", &[]), -4.0);
        assert_eq!(run("2 ^ -1", &[]), 0.5);
        assert_eq!(run("7 % 3", &[]), 1.0);
        assert_eq!(run("-1 % 3", &[]), 2.0);
    }

    #[test]
    fn names_become_inputs_in_order_of_first_use() {
        let e = Expr::parse("sin(t * 2) * depth + t").unwrap();
        assert_eq!(e.vars, vec!["t", "depth"]);
        assert!((e.eval(&[0.25, 0.5]) - (0.5f32.sin() * 0.5 + 0.25)).abs() < 1e-6);
        assert_eq!(input_names("sin(t * 2) * depth + pi"), vec!["t", "depth"]);
        assert_eq!(input_names("min(a, b) * (c"), vec!["a", "b", "c"]); // while it is half typed
        assert_eq!(input_names("a $ b > 2e3 + sin (x)"), vec!["a", "b", "x"]); // stray characters don't drop sockets
    }

    #[test]
    fn functions_and_constants_work() {
        assert_eq!(run("clamp(5, 0, 1)", &[]), 1.0);
        assert_eq!(run("mix(10, 20, 0.25)", &[]), 12.5);
        assert_eq!(run("fract(2.75)", &[]), 0.75);
        assert_eq!(run("step(0.5, 0.7) + step(0.5, 0.2)", &[]), 1.0);
        assert_eq!(run("smoothstep(0, 1, 0.5)", &[]), 0.5);
        assert_eq!(run("max(min(3, 2), abs(-1))", &[]), 2.0);
        assert!((run("sin(pi / 2)", &[]) - 1.0).abs() < 1e-6);
        assert!((run("tau", &[]) - TAU).abs() < 1e-6);
    }

    #[test]
    fn dividing_by_zero_or_going_out_of_range_gives_zero() {
        assert_eq!(run("1 / 0", &[]), 0.0);
        assert_eq!(run("5 % 0", &[]), 0.0);
        assert_eq!(run("sqrt(-4)", &[]), 0.0);
        assert_eq!(run("(0 - 1) ^ 0.5", &[]), 0.0); // NaN
    }

    #[test]
    fn mistakes_say_what_is_wrong() {
        assert!(Expr::parse("").unwrap_err().contains("Write a formula"));
        assert!(Expr::parse("1 +").unwrap_err().contains("ends too soon"));
        assert!(Expr::parse("(1 + 2").unwrap_err().contains("Expected \")\""));
        assert!(Expr::parse("foo(1)").unwrap_err().contains("Unknown function"));
        assert!(Expr::parse("min(1)").unwrap_err().contains("takes 2 values"));
        assert!(Expr::parse("sin * 2").unwrap_err().contains("is a function"));
        assert!(Expr::parse("1 $ 2").unwrap_err().contains("Unexpected"));
        assert!(Expr::parse("1 2").unwrap_err().contains("Unexpected input"));
        assert!(Expr::parse("enabled + 1").unwrap_err().contains("can't be an input name"));
        assert!(Expr::parse("a > b").unwrap_err().contains("Unexpected"));
    }
}
