/* This file is part of moulars.
 *
 * moulars is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * moulars is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with moulars.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::io::{BufRead, Result};
use std::str::FromStr;

use crate::general_error;
use crate::plasma::UnifiedTime;
use crate::plasma::color::{Color32, ColorRGBA};
use crate::plasma::geometry::{Vector3, Quaternion};
use super::{VarType, VarDefault, VarDescriptor, StateDescriptor};

#[derive(Eq, PartialEq, Debug)]
enum Token {
    Number(String),         // Will be parsed to a specific type in the consumer
    Identifier(String),     // Also used for keywords, since they are context-sensitive
    TypeReference(String),
    StringLiteral(String),
    CharToken(char),        // Parens, braces, etc
    Invalid(char),
    IncompleteString,
}

#[derive(Eq, PartialEq, Debug)]
struct Location {
    line: usize,
    column: usize,
}

impl Display for Location {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "line {}, column {}", self.line, self.column)
    }
}

pub struct Parser<S: BufRead> {
    stream: S,
    tok_buffer: VecDeque<(Token, Location)>,
    line_no: usize,
}

// Main keywords
const KW_STATEDESC: &str    = "STATEDESC";
const KW_VAR: &str          = "VAR";
const KW_VERSION: &str      = "VERSION";

// Variable context-sensitive keywords
const KW_DEFAULT: &str          = "DEFAULT";
const KW_DEFAULTOPTION: &str    = "DEFAULTOPTION";
const KW_DISPLAYOPTION: &str    = "DISPLAYOPTION";

impl<S: BufRead> Parser<S> {
    pub fn new(stream: S) -> Self {
        Self { stream, tok_buffer: VecDeque::new(), line_no: 0 }
    }

    fn next_token(&mut self) -> Result<Option<(Token, Location)>> {
        while self.tok_buffer.is_empty() {
            if !self.next_line()? {
                return Ok(None);
            }
        }

        Ok(self.tok_buffer.pop_front())
    }

    fn push_token(&mut self, token: Token, start: usize) {
        let location = Location { line: self.line_no, column: start + 1 };
        self.tok_buffer.push_back((token, location));
    }

    fn next_line(&mut self) -> Result<bool> {
        // NOTE: Not using read_line since that assumes the input is valid UTF-8.
        let mut line_buf = Vec::new();
        let num_read = self.stream.read_until(b'\n', &mut line_buf)?;
        if num_read == 0 {
            // Reached end of file
            return Ok(false)
        }
        self.line_no += 1;

        let mut start = 0;
        while start < line_buf.len() {
            // SAFETY: Range is already checked by the loop condition
            let start_ch = unsafe { line_buf.get_unchecked(start) };
            match start_ch {
                // Stop at comment markers
                b'#' => break,

                // Skip whitespace
                b' ' | b'\t' | b'\r' | b'\n' => start += 1,

                // Numeric
                b'0'..=b'9' | b'-' => {
                    let mut end = start + 1;
                    while end < line_buf.len() {
                        // SAFETY: Range is already checked by the loop condition
                        let end_ch = unsafe { line_buf.get_unchecked(end) };
                        match end_ch {
                            b'0'..=b'9' | b'.' => end += 1,
                            _ => break,
                        }
                    }
                    // SAFETY: The characters in this range are already validated to be ASCII
                    let num_value = unsafe { std::str::from_utf8_unchecked(&line_buf[start..end]) };
                    self.push_token(Token::Number(num_value.to_string()), start);
                    start = end;
                }

                // Identifier, keyword, or statedesc reference
                b'A'..=b'Z' | b'a'..=b'z' | b'_' | b'$' => {
                    let mut end = start + 1;
                    while end < line_buf.len() {
                        // SAFETY: Range is already checked by the loop condition
                        let end_ch = unsafe { line_buf.get_unchecked(end) };
                        match end_ch {
                            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => end += 1,
                            _ => break,
                        }
                    }
                    if line_buf[start] == b'$' {
                        // SAFETY: The characters in this range are already validated to be ASCII
                        let statedesc = unsafe { std::str::from_utf8_unchecked(&line_buf[start+1..end]) };
                        if statedesc.is_empty() {
                            // A '$' with no identifier behind it is invalid
                            self.push_token(Token::Invalid(line_buf[start] as char), start);
                        } else {
                            self.push_token(Token::TypeReference(statedesc.to_string()), start);
                        }
                    } else {
                        // SAFETY: The characters in this range are already validated to be ASCII
                        let ident = unsafe { std::str::from_utf8_unchecked(&line_buf[start..end]) };
                        self.push_token(Token::Identifier(ident.to_string()), start);
                    }
                    start = end;
                }

                // Single-char tokens
                b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'=' | b',' | b';' => {
                    self.push_token(Token::CharToken(line_buf[start] as char), start);
                    start += 1;
                }

                // String literal
                b'"' => {
                    // NOTE: We assume no escape capability...
                    let mut end = start + 1;
                    while end < line_buf.len() && line_buf[end] != b'"' {
                        end += 1;
                    }
                    let literal = String::from_utf8_lossy(&line_buf[start+1..end]);
                    if end < line_buf.len() && line_buf[end] == b'"' {
                        self.push_token(Token::StringLiteral(literal.to_string()), start);
                    } else {
                        self.push_token(Token::IncompleteString, start);
                        break;
                    }
                    start = end + 1;
                }

                _ => {
                    self.push_token(Token::Invalid(line_buf[start] as char), start);
                    start += 1;
                }
            }
        }
        Ok(true)
    }

    pub fn parse(&mut self) -> Result<Vec<StateDescriptor>> {
        let mut descriptors = Vec::new();
        while let Some((token, location)) = self.next_token()? {
            match &token {
                Token::Identifier(ident) => match ident.as_ref() {
                    KW_STATEDESC => descriptors.push(self.parse_statedesc()?),
                    _ => return Err(general_error!("Unexpected {:?} at {}", token, location))
                }
                _ => return Err(general_error!("Unexpected {:?} at {}", token, location))
            }
        }
        Ok(descriptors)
    }

    fn parse_statedesc(&mut self) -> Result<StateDescriptor> {
        let name = self.expect_identifier(KW_STATEDESC)?;
        self.expect_token(&Token::CharToken('{'), KW_STATEDESC)?;
        let start_line = self.line_no;

        let mut opt_version = None;
        let mut vars = Vec::new();
        while let Some((token, location)) = self.next_token()? {
            match &token {
                Token::Identifier(ident) => match ident.as_ref() {
                    KW_VERSION => {
                        opt_version = Some(self.expect_number::<u32>(false, KW_STATEDESC)?);
                    },
                    KW_VAR => vars.push(self.parse_var()?),
                    _ => return Err(general_error!("Unexpected {:?} at {}", token, location))
                }
                Token::CharToken('}') => {
                    let version = match opt_version {
                        Some(version) => version,
                        None => {
                            return Err(general_error!(
                                "Missing version for state descriptor {} on line {}",
                                name, start_line));
                        }
                    };
                    return Ok(StateDescriptor::new(name, version, vars));
                }
                _ => return Err(general_error!("Unexpected {:?} at {}", token, location))
            }
        }

        Err(general_error!("Unexpected EOF while parsing STATEDESC"))
    }

    fn parse_var(&mut self) -> Result<VarDescriptor> {
        let var_type = match self.next_token()? {
            Some((Token::Identifier(ident), location)) => {
                match ident.to_ascii_lowercase().as_ref() {
                    "agetimeofday" => VarType::AgeTimeOfDay,
                    "bool" => VarType::Bool,
                    "byte" => VarType::Byte,
                    "creatable" | "message" => VarType::Creatable,
                    "double" => VarType::Double,
                    "float" => VarType::Float,
                    "int" => VarType::Int,
                    "plkey" => VarType::Key,
                    "point3" => VarType::Point3,
                    "quat" | "quaternion" => VarType::Quat,
                    "rgb" => VarType::Rgb,
                    "rgb8" => VarType::Rgb8,
                    "rgba" => VarType::Rgba,
                    "rgba8" => VarType::Rgba8,
                    "short" => VarType::Short,
                    "string32" => VarType::String32,
                    "time" => VarType::Time,
                    "vector3" => VarType::Vector3,
                    _ => return Err(general_error!("Unknown type {} at {}", ident, location)),
                }
            }
            Some((Token::TypeReference(ident), _)) => VarType::StateDesc(ident),
            Some((token, location)) => {
                return Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => return Err(general_error!("Unexpected EOF while parsing VAR"))
        };
        let var_name = self.expect_identifier(KW_VAR)?;
        self.expect_token(&Token::CharToken('['), KW_VAR)?;
        let var_count = match self.next_token()? {
            Some((Token::Number(value), location)) => {
                let count = value.parse::<usize>().map_err(|err| {
                    general_error!("Invalid var count '{}' at {}: {}",
                                   value, location, err)
                })?;
                self.expect_token(&Token::CharToken(']'), KW_VAR)?;
                Some(count)
            }
            Some((Token::CharToken(']'), _)) => None,
            Some((token, location)) => {
                return Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => return Err(general_error!("Unexpected EOF while parsing VAR"))
        };

        let mut default = None;

        // Parse any optional fields
        while let Some((token, location)) = self.next_token()? {
            match &token {
                Token::Identifier(ident) => match ident.as_ref() {
                    KW_DEFAULT => {
                        default = self.parse_default(&var_type)?;
                    }
                    KW_DEFAULTOPTION => {
                        // Ignored for now
                        self.expect_token(&Token::CharToken('='), KW_DEFAULTOPTION)?;
                        let _ = self.expect_identifier(KW_DEFAULTOPTION)?;
                    }
                    KW_DISPLAYOPTION => {
                        // Ignored for now
                        self.expect_token(&Token::CharToken('='), KW_DISPLAYOPTION)?;
                        let _ = self.expect_identifier(KW_DISPLAYOPTION)?;
                    }
                    _ => {
                        self.tok_buffer.push_front((token, location));
                        return Ok(VarDescriptor::new(var_name, var_type, var_count, default))
                    }
                }
                // At least one SDL file has a stray ; at the end of a line...
                // We just ignore it here.
                Token::CharToken(';') => (),
                _ => {
                    self.tok_buffer.push_front((token, location));
                    return Ok(VarDescriptor::new(var_name, var_type, var_count, default))
                }
            }
        }

        Err(general_error!("Unexpected EOF while parsing VAR"))
    }

    fn parse_default(&mut self, var_type: &VarType) -> Result<Option<VarDefault>> {
        self.expect_token(&Token::CharToken('='), KW_DEFAULT)?;
        match var_type {
            VarType::Bool => Ok(Some(VarDefault::Bool(self.expect_bool(true, KW_DEFAULT)?))),
            VarType::Byte => Ok(Some(VarDefault::Byte(self.expect_number::<u8>(true, KW_DEFAULT)?))),
            VarType::Double => Ok(Some(VarDefault::Double(self.expect_number::<f64>(true, KW_DEFAULT)?))),
            VarType::Float => Ok(Some(VarDefault::Float(self.expect_number::<f32>(true, KW_DEFAULT)?))),
            VarType::Int => Ok(Some(VarDefault::Int(self.expect_number::<i32>(true, KW_DEFAULT)?))),
            VarType::Short => Ok(Some(VarDefault::Short(self.expect_number::<i16>(true, KW_DEFAULT)?))),
            VarType::String32 => Ok(Some(VarDefault::String32(self.expect_string_literal(KW_DEFAULT)?))),
            VarType::Time => {
                let secs = self.expect_number::<u32>(true, KW_DEFAULT)?;
                Ok(Some(VarDefault::Time(UnifiedTime::from_secs(secs))))
            }
            VarType::Key => {
                // "nil" is the only supported default value for keys
                match self.next_token()? {
                    Some((Token::Identifier(ident), location)) => {
                        match ident.as_ref() {
                            "nil" => Ok(None),
                            _ => Err(general_error!("Unexpected plKey value '{}' at {}", ident, location))
                        }
                    }
                    Some((token, location)) => {
                        Err(general_error!("Unexpected {:?} at {}", token, location))
                    }
                    None => Err(general_error!("Unexpected EOF while parsing DEFAULT"))
                }
            }
            VarType::Point3 | VarType::Vector3 => {
                let (values, location) = self.expect_sequence::<f32>(KW_DEFAULT)?;
                if values.len() != 3 {
                    return Err(general_error!("Incorrect number of elements for Point3 at {}",
                               location));
                }
                let vector = Vector3 { x: values[0], y: values[1], z: values[2] };
                Ok(Some(VarDefault::Vector3(vector)))
            }
            VarType::Quat => {
                let (values, location) = self.expect_sequence::<f32>(KW_DEFAULT)?;
                if values.len() != 4 {
                    return Err(general_error!("Incorrect number of elements for Quaternion at {}",
                               location));
                }
                let quat = Quaternion { x: values[0], y: values[1], z: values[2], w: values[3] };
                Ok(Some(VarDefault::Quat(quat)))
            }
            VarType::Rgb => {
                let (values, location) = self.expect_sequence::<f32>(KW_DEFAULT)?;
                if values.len() != 3 {
                    return Err(general_error!("Incorrect number of elements for RGB at {}",
                               location));
                }
                let color = ColorRGBA { r: values[0], g: values[1], b: values[2], a: 1.0 };
                Ok(Some(VarDefault::Rgba(color)))
            }
            VarType::Rgb8 => {
                let (values, location) = self.expect_sequence::<u8>(KW_DEFAULT)?;
                if values.len() != 3 {
                    return Err(general_error!("Incorrect number of elements for RGB8 at {}",
                               location));
                }
                let color = Color32 { r: values[0], g: values[1], b: values[2], a: 255 };
                Ok(Some(VarDefault::Rgba8(color)))
            }
            VarType::Rgba => {
                let (values, location) = self.expect_sequence::<f32>(KW_DEFAULT)?;
                if values.len() != 4 {
                    return Err(general_error!("Incorrect number of elements for RGBA at {}",
                               location));
                }
                let color = ColorRGBA { r: values[0], g: values[1], b: values[2], a: values[3] };
                Ok(Some(VarDefault::Rgba(color)))
            }
            VarType::Rgba8 => {
                let (values, location) = self.expect_sequence::<u8>(KW_DEFAULT)?;
                if values.len() != 4 {
                    return Err(general_error!("Incorrect number of elements for RGBA8 at {}",
                               location));
                }
                let color = Color32 { r: values[0], g: values[1], b: values[2], a: values[3] };
                Ok(Some(VarDefault::Rgba8(color)))
            }
            VarType::AgeTimeOfDay => {
                Err(general_error!("AgeTimeOfDay variables cannot have a default"))
            }
            VarType::Creatable => {
                Err(general_error!("Creatable variables cannot have a default"))
            }
            VarType::StateDesc(_) => {
                Err(general_error!("StateDesc variables cannot have a default"))
            }
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<String> {
        match self.next_token()? {
            Some((Token::Identifier(ident), _)) => Ok(ident),
            Some((token, location)) => {
                Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => Err(general_error!("Unexpected EOF while parsing {}", context))
        }
    }

    fn expect_number<T>(&mut self, seq_ok: bool, context: &str) -> Result<T>
        where T: FromStr, <T as FromStr>::Err: Display
    {
        match self.next_token()? {
            Some((Token::Number(value), location)) => {
                value.parse::<T>().map_err(|err| {
                    general_error!("Invalid numeric literal '{}' at {}: {}",
                                   value, location, err)
                })
            }
            Some((Token::CharToken('('), _)) if seq_ok => {
                let inner = self.expect_number::<T>(false, context)?;
                self.expect_token(&Token::CharToken(')'), context)?;
                Ok(inner)
            }
            Some((token, location)) => {
                Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => Err(general_error!("Unexpected EOF while parsing {}", context))
        }
    }

    fn expect_bool(&mut self, seq_ok: bool, context: &str) -> Result<bool> {
        match self.next_token()? {
            Some((Token::Identifier(value), location)) => {
                match value.to_ascii_lowercase().as_ref() {
                    "false" => Ok(false),
                    "true" => Ok(true),
                    _ => Err(general_error!("Invalid boolean literal '{}' at {}", value, location))
                }
            }
            Some((Token::Number(value), location)) => {
                match value.as_ref() {
                    "0" => Ok(false),
                    "1" => Ok(true),
                    _ => Err(general_error!("Invalid boolean literal '{}' at {}", value, location))
                }
            }
            Some((Token::CharToken('('), _)) if seq_ok => {
                let inner = self.expect_bool(false, context)?;
                self.expect_token(&Token::CharToken(')'), context)?;
                Ok(inner)
            }
            Some((token, location)) => {
                Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => Err(general_error!("Unexpected EOF while parsing {}", context))
        }
    }

    fn expect_sequence<T>(&mut self, context: &str) -> Result<(Vec<T>, Location)>
        where T: FromStr, <T as FromStr>::Err: Display
    {
        let mut result = Vec::new();
        let start = self.expect_token(&Token::CharToken('('), context)?;
        loop {
            result.push(self.expect_number::<T>(false, context)?);
            match self.next_token()? {
                Some((Token::CharToken(','), _)) => (),
                Some((Token::CharToken(')'), _)) => return Ok((result, start)),
                Some((token, location)) => {
                    return Err(general_error!("Unexpected {:?} at {}", token, location));
                }
                None => return Err(general_error!("Unexpected EOF while parsing {}", context))
            }
        }
    }

    fn expect_token(&mut self, expected: &Token, context: &str) -> Result<Location> {
        match self.next_token()? {
            Some((token, location)) => {
                if &token == expected {
                    Ok(location)
                } else {
                    Err(general_error!("Unexpected {:?} at {}", token, location))
                }
            }
            None => Err(general_error!("Unexpected EOF while parsing {}", context))
        }
    }

    fn expect_string_literal(&mut self, context: &str) -> Result<String> {
        match self.next_token()? {
            // String literal or single word value
            Some((Token::StringLiteral(value) | Token::Identifier(value), _)) => Ok(value),
            Some((token, location)) => {
                Err(general_error!("Unexpected {:?} at {}", token, location))
            }
            None => Err(general_error!("Unexpected EOF while parsing {}", context))
        }
    }
}

// For simplifying the tokenizer tests
#[cfg(test)]
macro_rules! check_token {
    ($parser:ident, $tok_type:expr, @($line:literal, $column:literal)) => {
        assert_eq!($parser.next_token().unwrap(),
                   Some(($tok_type, Location { line: $line, column: $column })));
    };
    ($parser:ident, None) => {
        assert_eq!($parser.next_token().unwrap(), None);
    };
}

#[test]
fn test_tokenizer() {
    use std::io::Cursor;

    {
        let empty = b"";
        let mut parser = Parser::new(Cursor::new(empty));
        check_token!(parser, None);
    }

    {
        let numerics = b"0 3.14 -25 -99.999";
        let mut parser = Parser::new(Cursor::new(numerics));
        check_token!(parser, Token::Number("0".to_string()), @(1, 1));
        check_token!(parser, Token::Number("3.14".to_string()), @(1, 3));
        check_token!(parser, Token::Number("-25".to_string()), @(1, 8));
        check_token!(parser, Token::Number("-99.999".to_string()), @(1, 12));
        check_token!(parser, None);
    }

    {
        let idents = b"VAR keyword_123 X";
        let mut parser = Parser::new(Cursor::new(idents));
        check_token!(parser, Token::Identifier("VAR".to_string()), @(1, 1));
        check_token!(parser, Token::Identifier("keyword_123".to_string()), @(1, 5));
        check_token!(parser, Token::Identifier("X".to_string()), @(1, 17));
        check_token!(parser, None);
    }

    {
        let sd_refs = b"$f $statedesc1";
        let mut parser = Parser::new(Cursor::new(sd_refs));
        check_token!(parser, Token::TypeReference("f".to_string()), @(1, 1));
        check_token!(parser, Token::TypeReference("statedesc1".to_string()), @(1, 4));
        check_token!(parser, None);
    }

    {
        let misc_chars = b"[({=,;})]";
        let mut parser = Parser::new(Cursor::new(misc_chars));
        check_token!(parser, Token::CharToken('['), @(1, 1));
        check_token!(parser, Token::CharToken('('), @(1, 2));
        check_token!(parser, Token::CharToken('{'), @(1, 3));
        check_token!(parser, Token::CharToken('='), @(1, 4));
        check_token!(parser, Token::CharToken(','), @(1, 5));
        check_token!(parser, Token::CharToken(';'), @(1, 6));
        check_token!(parser, Token::CharToken('}'), @(1, 7));
        check_token!(parser, Token::CharToken(')'), @(1, 8));
        check_token!(parser, Token::CharToken(']'), @(1, 9));
        check_token!(parser, None);
    }

    {
        let strings = b"\"Test 1..2..3..\"  \"Second\"\"Contains # other *$! chars\"";
        let mut parser = Parser::new(Cursor::new(strings));
        check_token!(parser, Token::StringLiteral("Test 1..2..3..".to_string()), @(1, 1));
        check_token!(parser, Token::StringLiteral("Second".to_string()), @(1, 19));
        check_token!(parser, Token::StringLiteral("Contains # other *$! chars".to_string()), @(1, 27));
        check_token!(parser, None);
    }

    {
        let comments = b"# This is a comment\ntest  # So is this\n";
        let mut parser = Parser::new(Cursor::new(comments));
        check_token!(parser, Token::Identifier("test".to_string()), @(2, 1));
        check_token!(parser, None);
    }

    {
        let errors = b"*+$. # Tokens after here are ok *+$\n$$";
        let mut parser = Parser::new(Cursor::new(errors));
        check_token!(parser, Token::Invalid('*'), @(1, 1));
        check_token!(parser, Token::Invalid('+'), @(1, 2));
        check_token!(parser, Token::Invalid('$'), @(1, 3));
        check_token!(parser, Token::Invalid('.'), @(1, 4));
        check_token!(parser, Token::Invalid('$'), @(2, 1));
        check_token!(parser, Token::Invalid('$'), @(2, 2));
        check_token!(parser, None);
    }

    {
        let incomplete_strings = b"\"One\" \"Two\n\"\n\"Three";
        let mut parser = Parser::new(Cursor::new(incomplete_strings));
        check_token!(parser, Token::StringLiteral("One".to_string()), @(1, 1));
        check_token!(parser, Token::IncompleteString, @(1, 7));
        check_token!(parser, Token::IncompleteString, @(2, 1));
        check_token!(parser, Token::IncompleteString, @(3, 1));
        check_token!(parser, None);
    }
}

#[test]
fn test_parser() {
    use std::io::Cursor;

    {
        let empty = b"";
        let result = Parser::new(Cursor::new(empty)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 0);
    }

    {
        let bad_top_level = b"VAR x";
        let result = Parser::new(Cursor::new(bad_top_level)).parse();
        assert!(result.is_err());
    }

    {
        let empty_desc = b"STATEDESC empty { VERSION 1 }";
        let result = Parser::new(Cursor::new(empty_desc)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].name().as_str(), "empty");
        assert_eq!(descs[0].version(), 1);
        assert_eq!(descs[0].vars().len(), 0);
    }

    {
        let missing_version = b"STATEDESC missing_version { }";
        let result = Parser::new(Cursor::new(missing_version)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing version"));
    }

    {
        let incomplete = b"STATEDESC incomplete { VERSION 3";
        let result = Parser::new(Cursor::new(incomplete)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected EOF"));
    }

    {
        let basic_var = b"STATEDESC basic_var { VERSION 1 VAR BOOL foobar[1] }";
        let result = Parser::new(Cursor::new(basic_var)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name().as_str(), "foobar");
        assert_eq!(vars[0].var_type(), &VarType::Bool);
        assert_eq!(vars[0].count(), Some(1));
        assert!(vars[0].default().is_none());
    }

    {
        let sd_var = b"STATEDESC sd_var { VERSION 1 VAR $subtype foobar[1] }";
        let result = Parser::new(Cursor::new(sd_var)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name().as_str(), "foobar");
        assert_eq!(vars[0].var_type(), &VarType::StateDesc("subtype".to_string()));
        assert_eq!(vars[0].count(), Some(1));
    }

    {
        let no_size = b"STATEDESC no_size { VERSION 1 VAR INT foobar[] }";
        let result = Parser::new(Cursor::new(no_size)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::Int);
        assert_eq!(vars[0].count(), None);
    }

    {
        let incomplete_var = b"STATEDESC incomplete_var { VERSION 1 VAR }";
        let result = Parser::new(Cursor::new(incomplete_var)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected CharToken('}')"));
    }

    {
        let incomplete_var = b"STATEDESC incomplete_var { VERSION 1 VAR name }";
        let result = Parser::new(Cursor::new(incomplete_var)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown type"));
    }

    {
        let incomplete_var = b"STATEDESC incomplete_var { VERSION 1 VAR BOOL name }";
        let result = Parser::new(Cursor::new(incomplete_var)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected CharToken('}')"));
    }

    {
        let incomplete_var = b"STATEDESC incomplete_var { VERSION 1 VAR BOOL name[1 }";
        let result = Parser::new(Cursor::new(incomplete_var)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected CharToken('}')"));
    }

    {
        let with_defaults = b"STATEDESC with_defaults { VERSION 1 VAR INT answer[1] DEFAULT=42 }";
        let result = Parser::new(Cursor::new(with_defaults)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::Int);
        assert_eq!(vars[0].default(), Some(&VarDefault::Int(42)));
    }

    {
        let with_defaults = b"STATEDESC with_defaults { VERSION 1 VAR Rgb8 color[1] DEFAULT=(255, 127, 7) }";
        let result = Parser::new(Cursor::new(with_defaults)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::Rgb8);
        assert_eq!(vars[0].default(), Some(&VarDefault::Rgba8(Color32 { r: 255, g: 127, b: 7, a: 255 })));
    }

    {
        let with_defaults = b"STATEDESC with_defaults { VERSION 1 VAR BOOL foobar[1] DEFAULT=true }";
        let result = Parser::new(Cursor::new(with_defaults)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::Bool);
        assert_eq!(vars[0].default(), Some(&VarDefault::Bool(true)));
    }

    {
        let with_defaults = b"STATEDESC with_defaults { VERSION 1 VAR BOOL foobar[1] DEFAULT=(0) }";
        let result = Parser::new(Cursor::new(with_defaults)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::Bool);
        assert_eq!(vars[0].default(), Some(&VarDefault::Bool(false)));
    }

    {
        let incomplete_default = b"STATEDESC incomplete_default { VERSION 1 VAR BOOL foobar[1] DEFAULT= }";
        let result = Parser::new(Cursor::new(incomplete_default)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected CharToken('}')"));
    }

    {
        let bad_bool = b"STATEDESC bad_bool { VERSION 1 VAR BOOL foobar[1] DEFAULT=asdf }";
        let result = Parser::new(Cursor::new(bad_bool)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid boolean literal"));
    }

    {
        let incomplete_default = b"STATEDESC incomplete_default { VERSION 1 VAR Vector3 foobar[1] DEFAULT=(1, 2, 3 }";
        let result = Parser::new(Cursor::new(incomplete_default)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected CharToken('}')"));
    }

    {
        let bad_default = b"STATEDESC bad_default { VERSION 1 VAR Vector3 foobar[1] DEFAULT=42 }";
        let result = Parser::new(Cursor::new(bad_default)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected Number(\"42\")"));
    }

    {
        let string_var = b"STATEDESC bad_default { VERSION 1 VAR STRING32 foobar[1] DEFAULT=\"String Value\" }";
        let result = Parser::new(Cursor::new(string_var)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::String32);
        let expect_default = VarDefault::String32("String Value".to_string());
        assert_eq!(vars[0].default(), Some(&expect_default));
    }

    {
        let string_var = b"STATEDESC bad_default { VERSION 1 VAR STRING32 foobar[1] DEFAULT=empty }";
        let result = Parser::new(Cursor::new(string_var)).parse();
        assert!(result.is_ok());
        let descs = result.unwrap();
        assert_eq!(descs.len(), 1);
        let vars = descs[0].vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].var_type(), &VarType::String32);
        let expect_default = VarDefault::String32("empty".to_string());
        assert_eq!(vars[0].default(), Some(&expect_default));
    }

    {
        let string_var = b"STATEDESC bad_default { VERSION 1 VAR STRING32 foobar[1] DEFAULT=\"String }";
        let result = Parser::new(Cursor::new(string_var)).parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected IncompleteString"));
    }
}
