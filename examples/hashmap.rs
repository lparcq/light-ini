use light_ini::{IniHandler, IniParser};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Debug)]
enum HandlerError {
    DuplicateSection(String),
    UnknownSection(String),
}

struct Handler {
    pub globals: HashMap<String, String>,
    pub sections: HashMap<String, HashMap<String, String>>,
    section_name: Option<String>,
}

impl Handler {
    fn new() -> Handler {
        Handler {
            globals: HashMap::new(),
            sections: HashMap::new(),
            section_name: None,
        }
    }
}

impl IniHandler for Handler {
    type Error = HandlerError;

    fn section(&mut self, name: &str) -> Result<(), Self::Error> {
        self.section_name = Some(name.to_string());
        match self.sections.insert(name.to_string(), HashMap::new()) {
            Some(_) => Err(HandlerError::DuplicateSection(name.to_string())),
            None => Ok(()),
        }
    }

    fn option(&mut self, key: &str, value: &str) -> Result<(), Self::Error> {
        match &self.section_name {
            None => {
                self.globals.insert(key.to_string(), value.to_string());
                Ok(())
            }
            Some(ref section_name) => match self.sections.get_mut(section_name) {
                Some(ref mut section) => {
                    section.insert(key.to_string(), value.to_string());
                    Ok(())
                }
                None => return Err(HandlerError::UnknownSection(section_name.to_string())),
            },
        }
    }

    fn comment(&mut self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    for filename in env::args().skip(1) {
        let mut handler = Handler::new();
        let mut parser = IniParser::new(&mut handler);
        let path = PathBuf::from(&filename);
        parser.parse_file(path).unwrap();
        println!("File {}", filename);
        println!("Globals {:#?}", handler.globals);
        println!("Sections {:#?}", handler.sections);
    }
}