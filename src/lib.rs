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

use std::{
    convert::From,
    error, fmt,
    fs::File,
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

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
    InvalidLine(usize),
    Handler(HandlerError),
    Io(io::Error),
}

impl<HandlerError: fmt::Debug + error::Error> fmt::Display for IniError<HandlerError> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IniError::InvalidLine(line) => write!(f, "invalid line: {}", line),
            IniError::Handler(err) => write!(f, "handler error: {:?}", err),
            IniError::Io(err) => write!(f, "input/output error: {:?}", err),
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

impl<HandlerError: fmt::Debug + error::Error> From<HandlerError> for IniError<HandlerError> {
    fn from(err: HandlerError) -> Self {
        Self::Handler(err)
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

/// INI format parser.
pub struct IniParser<'a, Error: fmt::Debug + error::Error> {
    handler: &'a mut dyn IniHandler<Error = Error>,
    start_comment: String,
}

impl<'a, Error: fmt::Debug + error::Error> IniParser<'a, Error> {
    /// Create a parser using the given handler.
    pub fn new(handler: &'a mut dyn IniHandler<Error = Error>) -> IniParser<'a, Error> {
        Self::with_start_comment(handler, ';')
    }

    /// Create a parser using the given character as start of comment.
    pub fn with_start_comment(
        handler: &'a mut dyn IniHandler<Error = Error>,
        start_comment: char,
    ) -> IniParser<'a, Error> {
        let start_comment = format!("{}", start_comment);
        Self {
            handler,
            start_comment,
        }
    }

    /// Parse one line without trailing newline character.
    fn parse_ini_line(&mut self, line: &str, lineno: usize) -> Result<(), IniError<Error>> {
        let line = line.trim_start();
        if line.is_empty() {
            Ok(())
        } else if !line.is_char_boundary(0) {
            Err(IniError::InvalidLine(lineno))
        } else {
            let (prefix, rest) = line.split_at(1);
            if prefix == "[" {
                match rest.find(']') {
                    Some(pos) => {
                        let (name, _) = rest.split_at(pos);
                        self.handler.section(name.trim())?;
                    }
                    None => return Err(IniError::InvalidLine(lineno)),
                }
            } else if prefix == self.start_comment {
                self.handler.comment(rest.trim_start())?;
            } else {
                match line.find('=') {
                    Some(pos) => {
                        let (name, rest) = line.split_at(pos);
                        let (_, value) = rest.split_at(1);
                        self.handler.option(name.trim(), value.trim())?;
                    }
                    None => return Err(IniError::InvalidLine(lineno)),
                }
            }
            Ok(())
        }
    }

    /// Parse input from a buffered reader.
    pub fn parse_buffered<B: BufRead>(&mut self, input: B) -> Result<(), IniError<Error>> {
        let mut lineno = 0;
        for res in input.lines() {
            lineno += 1;
            match res {
                Ok(line) => self.parse_ini_line(line.trim_end(), lineno)?,
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

    use super::{IniError, IniHandler, IniParser};
    use std::error;
    use std::fmt;
    use std::io::{self, Seek, Write};

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

    #[derive(Debug)]
    enum ConfigSection {
        Default,
        Logging,
    }

    #[derive(Debug)]
    struct Config {
        section: ConfigSection,
        pub name: Option<String>,
        pub level: Option<String>,
        pub last_comment: Option<String>,
    }

    impl Config {
        fn new() -> Config {
            Config {
                section: ConfigSection::Default,
                name: None,
                level: None,
                last_comment: None,
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

        fn comment(&mut self, comment: &str) -> Result<(), Self::Error> {
            self.last_comment = Some(comment.to_string());
            Ok(())
        }
    }

    type ParserError = IniError<ConfigError>;
    type ParserResult<T> = Result<T, ParserError>;

    fn new_input_stream(content: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        let mut buf = io::Cursor::new(Vec::<u8>::new());
        writeln!(buf, "{}", content)?;
        buf.seek(io::SeekFrom::Start(0))?;
        Ok(buf)
    }

    fn read_config(content: &str, start_comment: Option<char>) -> ParserResult<Config> {
        let mut handler = Config::new();
        let buf = new_input_stream(content).map_err(IniError::Io)?;
        let mut parser = match start_comment {
            Some(ch) => IniParser::with_start_comment(&mut handler, ch),
            None => IniParser::new(&mut handler),
        };
        parser.parse(buf)?;
        Ok(handler)
    }

    const VALID_INI: &str = "name = test suite

; logging section
[logging]
level = error
";

    #[test]
    fn parse_valid_ini() -> ParserResult<()> {
        let config = read_config(VALID_INI, None)?;
        assert_eq!(Some("test suite".to_string()), config.name);
        assert_eq!(Some("logging section".to_string()), config.last_comment);
        Ok(())
    }

    const VALID_INI_ALT_COMMENT: &str = "# logging section
[logging]
level = error
";

    #[test]
    fn parse_valid_ini_alt_comment() -> ParserResult<()> {
        let config = read_config(VALID_INI_ALT_COMMENT, Some('#'))?;
        assert_eq!(Some("logging section".to_string()), config.last_comment);
        assert_eq!(Some("error".to_string()), config.level);
        Ok(())
    }

    const INVALID_SECTION: &str = "name = ok

[logging";

    #[test]
    fn parse_invalid_section() {
        let res = dbg!(read_config(INVALID_SECTION, None));
        assert!(matches!(res, Err(IniError::InvalidLine(3))));
    }

    const INVALID_OPTION: &str = "[logging]
level error";

    #[test]
    fn parse_invalid_option() {
        let res = dbg!(read_config(INVALID_OPTION, None));
        assert!(matches!(res, Err(IniError::InvalidLine(2))));
    }

    const UNEXPECTED_SECTION: &str = "name = test suite

[unknown]
level = error
";

    #[test]
    /// Parse ini-file with a section considered as invalid in the handler
    fn parse_unexpected_section() {
        let res = dbg!(read_config(UNEXPECTED_SECTION, None));
        assert!(matches!(
            res,
            Err(IniError::Handler(ConfigError::InvalidSection))
        ));
    }
}
