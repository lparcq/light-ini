//! # Light Ini file parser.
//!
//! `light-ini` implements an event-driven parser for the [INI file format](https://en.wikipedia.org/wiki/INI_file).
//! The handler must implement `IniHandler`.
//!
//! ```
//! use light_ini::{IniHandler, IniParser, IniHandlerError};
//!
//! struct Handler {}
//!
//! impl IniHandler for Handler {
//!     type Error = IniHandlerError;
//!
//!     fn section(&mut self, name: &str) -> Result<(), Self::Error> {
//!         println!("section {}", name);
//!         Ok(())
//!     }
//!
//!     fn option(&mut self, key: &str, value: &str) -> Result<(), Self::Error> {
//!         println!("option {} is {}", key, value);
//!         Ok(())
//!     }
//!
//!     fn comment(&mut self, comment: &str) -> Result<(), Self::Error> {
//!         println!("comment: {}", comment);
//!         Ok(())
//!     }
//! }
//!
//! let mut handler = Handler{};
//! let mut parser = IniParser::new(&mut handler);
//! parser.parse_file("example.ini");
//! ```

use nom::{
    IResult,
    character::complete::{space0, char, not_line_ending},
    bytes::complete::is_not,
    combinator::all_consuming,
        sequence::{delimited, preceded, terminated, tuple}
};
use std::error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

#[derive(Debug)]
/// Convenient error type for handlers that don't need detailed errors.
pub struct IniHandlerError {}

impl fmt::Display for IniHandlerError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "handler failure")
    }
}

impl error::Error for IniHandlerError {}

#[derive(Debug)]
/// Errors for INI format parsing
pub enum IniError<HandlerError: fmt::Debug + error::Error> {
    InvalidLine(String),
    Handler(HandlerError),
    Io(io::Error),
}

impl<HandlerError: fmt::Debug + error::Error> fmt::Display for IniError<HandlerError> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IniError::InvalidLine(line) => write!(f, "invalid line: {}", line),
            IniError::Handler(err) => write!(f, "handler error: {:?}", err),
            IniError::Io(err) => write!(f, "io error: {:?}", err),
        }
    }
}

impl<HandlerError: fmt::Debug + error::Error> error::Error for IniError<HandlerError> {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            IniError::InvalidLine(_) => None,
            IniError::Handler(err) => err.source(),
            IniError::Io(err) => err.source(),
        }
    }
}

/// Interface for the INI format handler
pub trait IniHandler {
    type Error: fmt::Debug;

    /// Called when a section is found
    fn section(&mut self, name: &str) -> Result<(), Self::Error>;

    /// Called when an option is found
    fn option(&mut self, key: &str, value: &str) -> Result<(), Self::Error>;

    /// Called for each comment
    fn comment(&mut self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

// Parse comments starting with a semi colon.
fn parse_comment(input: &str) -> IResult<&str,&str> {
    let semicolon = char(';');
    let (comment, _) = delimited(space0, semicolon, space0)(input)?;
    Ok(("", comment))
}

// Parse a section between square brackets.
fn parse_section(input: &str) -> IResult<&str,&str> {
    terminated(delimited(char('['), delimited(space0, is_not(" \t\r\n]"), space0), char(']')), space0)(input)
}

// Parse an option as "key = value"
fn parse_option(input: &str) -> IResult<&str,(&str, &str)> {
    let is_key = is_not(" ;=");
    let is_equal = delimited(space0, char('='), space0);
    tuple((is_key, preceded(is_equal, not_line_ending)))(input)
}

// Parse a blank line
fn parse_blank(input: &str) -> IResult<&str,&str> {
    space0(input)
}

// Convert nom errors to crate errors.
macro_rules! map_herror {
    ($res:expr) => {
        $res.map_err(IniError::Handler)
    };
}

/// INI format parser.
pub struct IniParser<'a, Error: fmt::Debug + error::Error> {
    handler: &'a mut dyn IniHandler<Error = Error>,
}

impl<'a, Error: fmt::Debug + error::Error> IniParser<'a, Error> {
    /// Create a parser using the given handler.
    pub fn new(handler: &'a mut dyn IniHandler<Error = Error>) -> IniParser<'a, Error> {
        IniParser { handler }
    }

    /// Parse one line without trailing newline character.
    fn parse_ini_line(&mut self, line: &str) -> Result<(), IniError<Error>> {
        match parse_comment(line) {
            Ok((_, comment)) => map_herror!(self.handler.comment(comment)),
            Err(_) => match all_consuming(parse_section)(line) {
                Ok((_, name)) => map_herror!(self.handler.section(name)),
                Err(_) => match all_consuming(parse_option)(line) {
                    Ok((_, (key, value))) => map_herror!(self.handler.option(key, value.trim_end())),
                    Err(_) => match all_consuming(parse_blank)(line) {
                        Ok(_) => Ok(()),
                        Err(_) => Err(IniError::InvalidLine(line.to_string())),
                    },
                },
            },
        }
    }

    /// Parse input from a buffered reader.
    pub fn parse_buffered<B: BufRead>(&mut self, input: B) -> Result<(), IniError<Error>> {
        for res in input.lines() {
            match res {
                Ok(line) => self.parse_ini_line(line.trim_end())?,
                Err(err) => return Err(IniError::Io(err)),
            }
        }
        Ok(())
    }

    /// Parse input from a reader.
    pub fn parse<R: Read>(&mut self, input: R) -> Result<(), IniError<Error>> {
        let mut reader = BufReader::new(input);
        self.parse_buffered(&mut reader)
    }

    /// Parse a file.
    pub fn parse_file<P>(&mut self, path: P) -> Result<(), IniError<Error>>
    where
        P: AsRef<Path>,
    {
        let file = File::open(path).map_err(IniError::Io)?;
        self.parse(file)
    }
}

#[cfg(test)]
mod tests {

    use super::{
        all_consuming, parse_comment, parse_option, parse_section, parse_blank, IniHandler,
        IniParser,
    };
    use std::error;
    use std::fmt;
    use std::io::{self, Seek, Write};

    #[test]
    fn parse_sections() {
        for line in &["[one]", "[ one ]  "] {
            let (_, name) = all_consuming(parse_section)(line).unwrap();
            assert_eq!("one", name);
        }
        for line in &["[one", "name = value"] {
            let res = all_consuming(parse_section)(line);
            assert!(res.is_err(), "parsing should have failed for: {}", line);
        }
    }

    #[test]
    fn parse_options() {
        let data = [ ("name = test", "name", "test"), ("name=one two three  ", "name", "one two three") ];
        for (input, expected_key, expected_value) in data {
            let (output, (key, value)) = parse_option(input).unwrap();
            assert_eq!(expected_key, key);
            assert_eq!(expected_value, value.trim_end());
            assert!(output.is_empty());
        }
    }

    #[test]
    fn parse_blank_lines() {
        for line in &["", "  \t  "] {
            all_consuming(parse_blank)(line).unwrap();
        }
    }

    #[test]
    fn parse_comments() {
        for line in &["; comment", "  ; comment"] {
            let (output, comment) = parse_comment(line).unwrap();
            assert_eq!("comment", comment);
            assert!(output.is_empty());
        }
    }

    #[derive(Debug)]
    enum ConfigError {
        InvalidSection,
        InvalidOption,
    }

    impl fmt::Display for ConfigError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                ConfigError::InvalidSection => write!(fmt, "invalid section"),
                ConfigError::InvalidOption => write!(fmt, "invalid option"),
            }
        }
    }

    impl error::Error for ConfigError {}

    enum ConfigSection {
        Default,
        Logging,
    }

    struct Config {
        section: ConfigSection,
        pub name: Option<String>,
        pub level: Option<String>,
        pub has_comments: bool,
    }

    impl Config {
        fn new() -> Config {
            Config {
                section: ConfigSection::Default,
                name: None,
                level: None,
                has_comments: false,
            }
        }
    }

    impl IniHandler for Config {
        type Error = ConfigError;

        fn section(&mut self, name: &str) -> Result<(), Self::Error> {
            match name {
                "logging" => {
                    self.section = ConfigSection::Logging;
                    Ok(())
                }
                _ => Err(ConfigError::InvalidSection),
            }
        }

        fn option(&mut self, key: &str, value: &str) -> Result<(), Self::Error> {
            match self.section {
                ConfigSection::Default if key == "name" => self.name = Some(value.to_string()),
                ConfigSection::Logging if key == "level" => self.level = Some(value.to_string()),
                _ => return Err(ConfigError::InvalidOption),
            }
            Ok(())
        }

        fn comment(&mut self, _: &str) -> Result<(), Self::Error> {
            self.has_comments = true;
            Ok(())
        }
    }

    const VALID_INI: &str = "name = test suite

; logging section
[logging]
level = error
";

    #[test]
    fn parse_valid_ini() -> io::Result<()> {
        let mut buf = io::Cursor::new(Vec::<u8>::new());
        writeln!(buf, "{}", VALID_INI)?;
        buf.seek(io::SeekFrom::Start(0))?;
        let mut handler = Config::new();
        let mut parser = IniParser::new(&mut handler);
        parser.parse(buf).unwrap();
        assert_eq!(Some("test suite".to_string()), handler.name);
        assert!(handler.has_comments);
        Ok(())
    }

    const INVALID_SECTION: &str = "name = test suite

[unknown]
level = error
";

    #[test]
    fn parse_invalid_section() -> io::Result<()> {
        let mut buf = io::Cursor::new(Vec::<u8>::new());
        writeln!(buf, "{}", INVALID_SECTION)?;
        buf.seek(io::SeekFrom::Start(0))?;
        let mut handler = Config::new();
        let mut parser = IniParser::new(&mut handler);
        assert!(parser.parse(buf).is_err());
        Ok(())
    }
}
