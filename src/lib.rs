//! # Light Ini file parser.
//!
//! `light-ini` implements an event-driven parser for the [INI file format](https://en.wikipedia.org/wiki/INI_file).
//! The handler must implement `IniHandler`.
//!
//! ```
//!
//! ```

use nom::{
    call, char,
    character::complete::{multispace0, space0},
    combinator::all_consuming,
    do_parse, is_not, named, opt,
};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

#[derive(Debug)]
/// Errors for INI format parsing
pub enum IniError<HandlerError> {
    InvalidLine(String),
    Handler(HandlerError),
    Io(io::Error),
}

/// Interface for the INI format handler
pub trait IniHandler {
    type Error;

    /// Called when a section is found
    fn section(&mut self, name: &str) -> Result<(), Self::Error>;

    /// Called when an option is found
    fn option(&mut self, key: &str, value: &str) -> Result<(), Self::Error>;

    /// Called for each comment
    fn comment(&mut self, comment: &str) -> Result<(), Self::Error>;
}

// Parse comments starting with a semi colon.
named!(parse_comment<&str,&str>,
  do_parse!(
      space0
      >> char!(';')
      >> space0
      >> ("")
  )
);

// Parse a section between square brackets.
named!(parse_section<&str, &str>,
  do_parse!(
     char!('[')
     >> opt!(space0)
     >> name: is_not!(" \t\r\n]")
     >> opt!(space0)
     >> char!(']')
     >> multispace0
     >> (name)
  )
);

// Parse an option as "key = value"
named!(parse_option<&str, &str>,
  do_parse!(
     key: is_not!(" ;=")
     >> opt!(space0)
     >> char!('=')
     >> opt!(space0)
     >> (key)
  )
);

// Parse a blank line
named!(parse_blank<&str,&str>,
   call!(multispace0)
);

// Convert nom errors to crate errors.
macro_rules! map_herror {
    ($res:expr) => {
        $res.map_err(|err| IniError::Handler(err))
    };
}

/// INI format parser.
pub struct IniParser<'a, Error> {
    handler: &'a mut dyn IniHandler<Error = Error>,
}

impl<'a, Error> IniParser<'a, Error> {
    /// Create a parser using the given handler.
    pub fn new(handler: &'a mut dyn IniHandler<Error = Error>) -> IniParser<'a, Error> {
        IniParser { handler }
    }

    /// Parse one line without trailing newline character.
    fn parse_ini_line(&mut self, line: &str) -> Result<(), IniError<Error>> {
        match parse_comment(line) {
            Ok((comment, _)) => map_herror!(self.handler.comment(comment)),
            Err(_) => match all_consuming(parse_section)(line) {
                Ok((_, name)) => map_herror!(self.handler.section(name)),
                Err(_) => match parse_option(line) {
                    Ok((value, key)) => map_herror!(self.handler.option(key, value)),
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
        let file = File::open(path).map_err(|err| IniError::Io(err))?;
        self.parse(file)
    }
}

#[cfg(test)]
mod tests {

    use super::{
        all_consuming, parse_blank, parse_comment, parse_option, parse_section, IniHandler,
        IniParser,
    };
    use std::io::{self, Seek, Write};

    #[test]
    fn parse_sections() {
        for line in &["[one]\n", "[ one ]  "] {
            let (_, name) = all_consuming(parse_section)(line).unwrap();
            assert_eq!("one", name);
        }
        for line in &["[one\n", "name = value"] {
            let res = all_consuming(parse_section)(line);
            assert!(res.is_err(), "parsing should have failed for: {}", line);
        }
    }

    #[test]
    fn parse_options() {
        for line in &["name = test", "name = one two three  "] {
            let (value, key) = parse_option(line).unwrap();
            assert_eq!("name", key);
            assert!(value.len() > 0);
        }
    }

    #[test]
    fn parse_blank_lines() {
        for line in &["\n", "  \t  \n"] {
            all_consuming(parse_blank)(line).unwrap();
        }
    }

    #[test]
    fn parse_comments() {
        for line in &["; comment", "  ; comment"] {
            let (comment, _) = parse_comment(line).unwrap();
            assert_eq!("comment", comment);
        }
    }

    #[derive(Debug)]
    enum ConfigError {
        InvalidSection,
        InvalidOption,
    }

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
