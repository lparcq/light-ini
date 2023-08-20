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
        } else {
            let (prefix, rest) = if line.is_char_boundary(1) {
                line.split_at(1)
            } else {
                ("", line)
            };
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

    use std::{
        error, fmt,
        io::{self, Seek, Write},
        str,
    };

    #[derive(Debug)]
    enum TestError {
        InvalidSection,
        InvalidOption,
        Io(io::Error),
        Utf8(str::Utf8Error),
    }

    impl fmt::Display for TestError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TestError::InvalidSection => write!(fmt, "invalid section"),
                TestError::InvalidOption => write!(fmt, "invalid option"),
                TestError::Io(err) => write!(fmt, "i/o error: {}", err),
                TestError::Utf8(err) => write!(fmt, "utf-8 error: {}", err),
            }
        }
    }

    impl error::Error for TestError {}

    #[derive(Debug)]
    /// Generic handler for tests
    ///
    /// Convert an ini file in a string where:
    /// - section "[name]" are written <name>
    /// - option "name = value" are written (name=value)
    /// - comments are written /*comment*/
    struct Handler {
        stream: io::Cursor<Vec<u8>>,
    }

    impl Handler {
        fn new() -> Self {
            Self {
                stream: io::Cursor::new(Vec::<u8>::new()),
            }
        }

        fn get(&self) -> Result<&str, TestError> {
            str::from_utf8(self.stream.get_ref()).map_err(TestError::Utf8)
        }
    }

    impl IniHandler for Handler {
        type Error = TestError;

        fn section(&mut self, name: &str) -> Result<(), Self::Error> {
            if name == "invalid" {
                Err(TestError::InvalidSection)
            } else {
                write!(self.stream, "<{}>", name).map_err(Self::Error::Io)
            }
        }

        fn option(&mut self, name: &str, value: &str) -> Result<(), Self::Error> {
            if name == "invalid" {
                Err(TestError::InvalidOption)
            } else {
                write!(self.stream, "({}={})", name, value).map_err(Self::Error::Io)
            }
        }

        fn comment(&mut self, comment: &str) -> Result<(), Self::Error> {
            write!(self.stream, "/*{}*/", comment).map_err(Self::Error::Io)
        }
    }

    type ParserError = IniError<TestError>;
    type ParserResult<T> = Result<T, ParserError>;

    fn new_input_stream(content: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        let mut buf = io::Cursor::new(Vec::<u8>::new());
        writeln!(buf, "{}", content)?;
        buf.seek(io::SeekFrom::Start(0))?;
        Ok(buf)
    }

    fn read_ini(content: &str, start_comment: Option<char>) -> ParserResult<String> {
        let mut handler = Handler::new();
        let buf = new_input_stream(content).map_err(IniError::Io)?;
        let mut parser = match start_comment {
            Some(ch) => IniParser::with_start_comment(&mut handler, ch),
            None => IniParser::new(&mut handler),
        };
        parser.parse(buf)?;
        handler
            .get()
            .map(|s| s.to_string())
            .map_err(ParserError::Handler)
    }

    const VALID_INI: &str = "name = test suite

; logging section
[logging]
level = error
";

    #[test]
    fn parse_valid_ini() -> ParserResult<()> {
        let result = read_ini(VALID_INI, None)?;
        assert_eq!(
            "(name=test suite)/*logging section*/<logging>(level=error)",
            result
        );
        Ok(())
    }

    const VALID_INI_ALT_COMMENT: &str = "# logging section
[logging]
level = error
";

    #[test]
    fn parse_valid_ini_alt_comment() -> ParserResult<()> {
        let result = read_ini(VALID_INI_ALT_COMMENT, Some('#'))?;
        assert_eq!("/*logging section*/<logging>(level=error)", result);
        Ok(())
    }

    const VALID_INI_UNICODE: &str = "[ŝipo]
ĵurnalo = ĉirkaŭ";

    #[test]
    fn parse_unicode_ini() -> ParserResult<()> {
        let result = read_ini(VALID_INI_UNICODE, None)?;
        assert_eq!("<ŝipo>(ĵurnalo=ĉirkaŭ)", result);
        Ok(())
    }

    const INVALID_SECTION: &str = "name = ok

[logging";

    #[test]
    fn parse_invalid_section() {
        let res = dbg!(read_ini(INVALID_SECTION, None));
        assert!(matches!(res, Err(IniError::InvalidLine(3))));
    }

    const INVALID_OPTION: &str = "[logging]
level error";

    #[test]
    fn parse_invalid_option() {
        let res = dbg!(read_ini(INVALID_OPTION, None));
        assert!(matches!(res, Err(IniError::InvalidLine(2))));
    }

    const UNEXPECTED_SECTION: &str = "name = test suite

[invalid]
level = error
";

    #[test]
    /// Parse ini-file with a section considered as invalid in the handler
    fn parse_unexpected_section() {
        let res = dbg!(read_ini(UNEXPECTED_SECTION, None));
        assert!(matches!(
            res,
            Err(IniError::Handler(TestError::InvalidSection))
        ));
    }

    const UNEXPECTED_OPTION: &str = "[logging]
invalid = error
";

    #[test]
    /// Parse ini-file with an option considered as invalid in the handler
    fn parse_unexpected_option() {
        let res = dbg!(read_ini(UNEXPECTED_OPTION, None));
        assert!(matches!(
            res,
            Err(IniError::Handler(TestError::InvalidOption))
        ));
    }
}
