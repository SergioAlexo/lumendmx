//! The Onyx-style command line: `1 THRU 8 @ 50`, `GROUP 2 @ 0`, `RECORD CUE
//! 1.5`, etc. `GROUP` resolves against real groups (Phase 5, see
//! `engine::groups`) as of this module's `resolve`; cue commands (`RECORD
//! CUE`/`UPDATE`/`DELETE CUE`/`COPY CUE a @ b`/`MOVE CUE a @ b`) act on the
//! current cuelist.
//!
//! Grammar (this project's own interpretation of the examples in
//! BUILD_PLAN.md — there's no full Onyx manual to work from):
//!
//! ```text
//! command      := selection (" @ " intensity)?
//!               | "@ " intensity
//!               | "CLEAR" | "RELEASE" | "ALL" | "NEXT" | "PREV"
//!               | "TIME" number
//!               | "RECORD CUE" number | "UPDATE" | "DELETE CUE" number
//!               | "COPY CUE" number ("@" | "AT") number
//!               | "MOVE CUE" number ("@" | "AT") number
//!               | "BLIND" ("ON" | "OFF")? | "HIGHLIGHT" ("ON" | "OFF")?   -- HIGHLT, HLT also accepted
//!               | "COPY" | "MOVE"   -- parse only; they need CUE a @ b
//! selection    := term (("+" | "-") term)*
//! term         := fixture_number | fixture_number "THRU" fixture_number | "GROUP" fixture_number
//! intensity    := "FULL" | number   -- number is a 0-100 percent
//! ```

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectOp {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionTerm {
    Fixture(u32),
    Range(u32, u32),
    Group(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionExpr {
    pub terms: Vec<(SelectOp, SelectionTerm)>,
}

impl SelectionExpr {
    /// Resolves against `known_fixture_numbers` and `groups` (group id ->
    /// its member fixture numbers, from `engine::groups::GroupStore`).
    pub fn resolve(
        &self,
        known_fixture_numbers: &HashSet<u32>,
        groups: &HashMap<u32, Vec<u32>>,
    ) -> Result<HashSet<u32>, String> {
        let mut result = HashSet::new();
        for (op, term) in &self.terms {
            let matched: HashSet<u32> = match term {
                SelectionTerm::Fixture(n) => [*n].into_iter().collect(),
                SelectionTerm::Range(from, to) => {
                    let (lo, hi) = if from <= to { (*from, *to) } else { (*to, *from) };
                    (lo..=hi).collect()
                }
                SelectionTerm::Group(n) => match groups.get(n) {
                    Some(members) => members.iter().copied().collect(),
                    None => return Err(format!("No group {n}")),
                },
            };
            match op {
                SelectOp::Add => result.extend(matched),
                SelectOp::Remove => {
                    for m in matched {
                        result.remove(&m);
                    }
                }
            }
        }
        result.retain(|n| known_fixture_numbers.contains(n));
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntensityValue {
    Full,
    Percent(f32),
}

impl IntensityValue {
    pub fn as_u8(self) -> u8 {
        match self {
            IntensityValue::Full => 255,
            IntensityValue::Percent(p) => (p.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Select(SelectionExpr),
    SetIntensity(Option<SelectionExpr>, IntensityValue),
    Clear,
    Release,
    SelectAll,
    Next,
    Prev,
    SetTime(f32),
    RecordCue(f32),
    UpdateCue,
    DeleteCue(f32),
    Copy,
    Move,
    /// `COPY CUE from @ to`.
    CopyCue(f32, f32),
    /// `MOVE CUE from @ to`: renumbers.
    MoveCue(f32, f32),
    /// Blind mode on/off (`None` toggles).
    Blind(Option<bool>),
    /// Highlight on/off (`None` toggles).
    Highlight(Option<bool>),
}

pub fn parse(input: &str) -> Result<Command, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Err("Empty command".to_string());
    }
    let upper: Vec<String> = tokens.iter().map(|t| t.to_uppercase()).collect();

    let exact = |n: usize, cmd: Command| {
        if tokens.len() == n {
            Ok(cmd)
        } else {
            Err(format!("'{}' takes no arguments", tokens[0]))
        }
    };

    match upper[0].as_str() {
        "CLEAR" => exact(1, Command::Clear),
        "RELEASE" => exact(1, Command::Release),
        "ALL" => exact(1, Command::SelectAll),
        "NEXT" => exact(1, Command::Next),
        "PREV" => exact(1, Command::Prev),
        "UPDATE" => exact(1, Command::UpdateCue),
        "COPY" | "MOVE" if tokens.len() == 1 => {
            exact(1, if upper[0] == "COPY" { Command::Copy } else { Command::Move })
        }
        "COPY" | "MOVE" => {
            let verb = upper[0].as_str();
            let usage = format!("Expected {verb} CUE <from> @ <to>");
            require(upper.get(1), "CUE", &usage)?;
            let from = parse_number(tokens.get(2), &usage)?;
            if !matches!(upper.get(3).map(String::as_str), Some("@") | Some("AT")) {
                return Err(usage);
            }
            let to = parse_number(tokens.get(4), &usage)?;
            if tokens.len() != 5 {
                return Err(usage);
            }
            Ok(if verb == "COPY" { Command::CopyCue(from, to) } else { Command::MoveCue(from, to) })
        }
        "BLIND" => Ok(Command::Blind(parse_on_off(&upper, "BLIND")?)),
        "HIGHLIGHT" | "HIGHLT" | "HLT" => Ok(Command::Highlight(parse_on_off(&upper, "HIGHLIGHT")?)),
        "TIME" => {
            let n = parse_number(tokens.get(1), "TIME expects a number")?;
            exact(2, Command::SetTime(n))
        }
        "RECORD" => {
            require(upper.get(1), "CUE", "Expected RECORD CUE <number>")?;
            let n = parse_number(tokens.get(2), "RECORD CUE expects a number")?;
            exact(3, Command::RecordCue(n))
        }
        "DELETE" => {
            require(upper.get(1), "CUE", "Expected DELETE CUE <number>")?;
            let n = parse_number(tokens.get(2), "DELETE CUE expects a number")?;
            exact(3, Command::DeleteCue(n))
        }
        "@" => {
            let value = parse_intensity(tokens.get(1))?;
            exact(2, Command::SetIntensity(None, value))
        }
        _ => {
            let (expr, consumed) = parse_selection(&tokens)?;
            if consumed == tokens.len() {
                return Ok(Command::Select(expr));
            }
            if upper.get(consumed).map(String::as_str) == Some("@") {
                let value = parse_intensity(tokens.get(consumed + 1))?;
                if consumed + 2 != tokens.len() {
                    return Err("Unexpected tokens after the intensity value".to_string());
                }
                return Ok(Command::SetIntensity(Some(expr), value));
            }
            Err(format!("Unexpected token '{}'", tokens[consumed]))
        }
    }
}

fn require(actual: Option<&String>, expected: &str, message: &str) -> Result<(), String> {
    if actual.map(String::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

fn parse_on_off(upper: &[String], name: &str) -> Result<Option<bool>, String> {
    match upper.get(1).map(String::as_str) {
        None => Ok(None),
        Some("ON") if upper.len() == 2 => Ok(Some(true)),
        Some("OFF") if upper.len() == 2 => Ok(Some(false)),
        _ => Err(format!("Expected {name}, {name} ON or {name} OFF")),
    }
}

fn parse_number(token: Option<&&str>, message: &str) -> Result<f32, String> {
    token.ok_or_else(|| message.to_string())?.parse::<f32>().map_err(|_| message.to_string())
}

fn parse_fixture_number(token: &str) -> Result<u32, String> {
    token.parse::<u32>().map_err(|_| format!("'{token}' is not a fixture number"))
}

fn parse_intensity(token: Option<&&str>) -> Result<IntensityValue, String> {
    let token = token.ok_or("Expected FULL or a percentage after @")?;
    if token.eq_ignore_ascii_case("FULL") {
        return Ok(IntensityValue::Full);
    }
    token
        .parse::<f32>()
        .map(IntensityValue::Percent)
        .map_err(|_| format!("'{token}' is not FULL or a number"))
}

fn parse_selection(tokens: &[&str]) -> Result<(SelectionExpr, usize), String> {
    let mut terms = Vec::new();
    let mut i = 0;
    let mut op = SelectOp::Add;
    loop {
        let (term, consumed) = parse_term(tokens, i)?;
        terms.push((op, term));
        i += consumed;
        match tokens.get(i) {
            Some(&"+") => {
                op = SelectOp::Add;
                i += 1;
            }
            Some(&"-") => {
                op = SelectOp::Remove;
                i += 1;
            }
            _ => break,
        }
    }
    Ok((SelectionExpr { terms }, i))
}

fn parse_term(tokens: &[&str], i: usize) -> Result<(SelectionTerm, usize), String> {
    let tok = tokens.get(i).ok_or("Expected a fixture number or GROUP")?;
    if tok.eq_ignore_ascii_case("GROUP") {
        let n = parse_fixture_number(tokens.get(i + 1).ok_or("Expected a group number")?)?;
        return Ok((SelectionTerm::Group(n), 2));
    }
    let n = parse_fixture_number(tok)?;
    if tokens.get(i + 1).is_some_and(|t| t.eq_ignore_ascii_case("THRU")) {
        let end = parse_fixture_number(tokens.get(i + 2).ok_or("Expected the end of a THRU range")?)?;
        return Ok((SelectionTerm::Range(n, end), 3));
    }
    Ok((SelectionTerm::Fixture(n), 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures(nums: &[u32]) -> HashSet<u32> {
        nums.iter().copied().collect()
    }

    #[test]
    fn parses_a_single_fixture_selection() {
        assert_eq!(
            parse("5").unwrap(),
            Command::Select(SelectionExpr { terms: vec![(SelectOp::Add, SelectionTerm::Fixture(5))] })
        );
    }

    #[test]
    fn parses_a_thru_range_selection() {
        assert_eq!(
            parse("1 THRU 8").unwrap(),
            Command::Select(SelectionExpr { terms: vec![(SelectOp::Add, SelectionTerm::Range(1, 8))] })
        );
        // Case-insensitive keyword.
        assert_eq!(parse("1 thru 8").unwrap(), parse("1 THRU 8").unwrap());
    }

    #[test]
    fn parses_range_with_intensity() {
        assert_eq!(
            parse("1 THRU 8 @ 50").unwrap(),
            Command::SetIntensity(
                Some(SelectionExpr { terms: vec![(SelectOp::Add, SelectionTerm::Range(1, 8))] }),
                IntensityValue::Percent(50.0)
            )
        );
    }

    #[test]
    fn parses_bare_at_full_against_current_selection() {
        assert_eq!(parse("@ FULL").unwrap(), Command::SetIntensity(None, IntensityValue::Full));
        assert_eq!(parse("@ full").unwrap(), Command::SetIntensity(None, IntensityValue::Full));
    }

    #[test]
    fn parses_plus_and_minus_selection_operators() {
        let no_groups = HashMap::new();
        let cmd = parse("1 THRU 8 + 10").unwrap();
        let Command::Select(expr) = cmd else { panic!("expected Select") };
        assert_eq!(
            expr.resolve(&fixtures(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), &no_groups).unwrap(),
            fixtures(&[1, 2, 3, 4, 5, 6, 7, 8, 10])
        );

        let cmd = parse("1 THRU 8 - 3").unwrap();
        let Command::Select(expr) = cmd else { panic!("expected Select") };
        assert_eq!(
            expr.resolve(&fixtures(&[1, 2, 3, 4, 5, 6, 7, 8]), &no_groups).unwrap(),
            fixtures(&[1, 2, 4, 5, 6, 7, 8])
        );
    }

    #[test]
    fn parses_group_selection_with_intensity() {
        assert_eq!(
            parse("GROUP 2 @ 0").unwrap(),
            Command::SetIntensity(
                Some(SelectionExpr { terms: vec![(SelectOp::Add, SelectionTerm::Group(2))] }),
                IntensityValue::Percent(0.0)
            )
        );
    }

    #[test]
    fn group_selection_fails_to_resolve_when_no_such_group_exists() {
        let Command::Select(expr) = parse("GROUP 2").unwrap() else { panic!("expected Select") };
        let err = expr.resolve(&fixtures(&[1, 2]), &HashMap::new()).unwrap_err();
        assert!(err.contains("group 2"));
    }

    #[test]
    fn group_selection_resolves_to_its_recorded_members() {
        let Command::Select(expr) = parse("GROUP 2").unwrap() else { panic!("expected Select") };
        let mut groups = HashMap::new();
        groups.insert(2, vec![5, 6, 7]);
        let resolved = expr.resolve(&fixtures(&[1, 2, 5, 6, 7]), &groups).unwrap();
        assert_eq!(resolved, fixtures(&[5, 6, 7]));
    }

    #[test]
    fn parses_record_update_delete() {
        assert_eq!(parse("RECORD CUE 1.5").unwrap(), Command::RecordCue(1.5));
        assert_eq!(parse("UPDATE").unwrap(), Command::UpdateCue);
        assert_eq!(parse("DELETE CUE 3").unwrap(), Command::DeleteCue(3.0));
    }

    #[test]
    fn parses_copy_move_next_prev_all() {
        assert_eq!(parse("COPY").unwrap(), Command::Copy);
        assert_eq!(parse("MOVE").unwrap(), Command::Move);
        assert_eq!(parse("NEXT").unwrap(), Command::Next);
        assert_eq!(parse("PREV").unwrap(), Command::Prev);
        assert_eq!(parse("ALL").unwrap(), Command::SelectAll);
    }

    #[test]
    fn parses_copy_and_move_cue() {
        assert_eq!(parse("COPY CUE 1 @ 5").unwrap(), Command::CopyCue(1.0, 5.0));
        assert_eq!(parse("copy cue 1.5 at 7.5").unwrap(), Command::CopyCue(1.5, 7.5));
        assert_eq!(parse("MOVE CUE 2 @ 3").unwrap(), Command::MoveCue(2.0, 3.0));
        assert_eq!(parse("MOVE CUE 2 AT 3").unwrap(), Command::MoveCue(2.0, 3.0));
        // Missing or malformed parts.
        assert!(parse("COPY CUE").is_err());
        assert!(parse("COPY CUE 1").is_err());
        assert!(parse("COPY CUE 1 @").is_err());
        assert!(parse("COPY 1 @ 5").is_err());
        assert!(parse("MOVE CUE 2 @ 3 4").is_err());
        assert!(parse("MOVE CUE x @ 3").is_err());
    }

    #[test]
    fn parses_blind_and_highlight() {
        assert_eq!(parse("BLIND").unwrap(), Command::Blind(None));
        assert_eq!(parse("blind on").unwrap(), Command::Blind(Some(true)));
        assert_eq!(parse("BLIND OFF").unwrap(), Command::Blind(Some(false)));
        assert_eq!(parse("HIGHLIGHT").unwrap(), Command::Highlight(None));
        assert_eq!(parse("HIGHLT").unwrap(), Command::Highlight(None));
        assert_eq!(parse("HLT OFF").unwrap(), Command::Highlight(Some(false)));
        assert!(parse("BLIND MAYBE").is_err());
        assert!(parse("HIGHLIGHT ON NOW").is_err());
    }

    #[test]
    fn parses_time() {
        assert_eq!(parse("TIME 3").unwrap(), Command::SetTime(3.0));
        assert_eq!(parse("TIME 1.5").unwrap(), Command::SetTime(1.5));
    }

    #[test]
    fn parses_clear_and_release() {
        assert_eq!(parse("CLEAR").unwrap(), Command::Clear);
        assert_eq!(parse("RELEASE").unwrap(), Command::Release);
    }

    #[test]
    fn intensity_value_converts_to_u8() {
        assert_eq!(IntensityValue::Full.as_u8(), 255);
        assert_eq!(IntensityValue::Percent(0.0).as_u8(), 0);
        assert_eq!(IntensityValue::Percent(50.0).as_u8(), 128);
        assert_eq!(IntensityValue::Percent(100.0).as_u8(), 255);
        assert_eq!(IntensityValue::Percent(150.0).as_u8(), 255); // clamped
    }

    #[test]
    fn rejects_empty_and_garbage_input() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
        assert!(parse("FROBNICATE").is_err());
        assert!(parse("1 THRU").is_err()); // missing end of range
        assert!(parse("RECORD SCENE 1").is_err()); // must be RECORD CUE
        assert!(parse("@ BANANA").is_err());
        assert!(parse("TIME").is_err());
        assert!(parse("CLEAR NOW").is_err()); // takes no arguments
    }
}
