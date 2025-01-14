#![allow(clippy::result_unit_err)]
use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek},
    iter::Peekable,
};

use crate::{reader::JsonReader, value::Number};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    CurlyOpen,
    CurlyClose,
    Quotes,
    Colon,
    String(String),
    Number(Number),
    ArrayOpen,
    ArrayClose,
    Comma,
    Boolean(bool),
    Null,
}

pub struct Jsontokensizer<T>
where
    T: Read + Seek,
{
    tokens: Vec<Token>,
    iterator: Peekable<JsonReader<T>>,
}

impl<T> Jsontokensizer<T>
where
    T: Read + Seek,
{
    pub fn new(reader: File) -> Jsontokensizer<File> {
        let json_reader = JsonReader::<File>::new(BufReader::new(reader));
        Jsontokensizer {
            tokens: vec![],
            iterator: json_reader.peekable(),
        }
    }

    pub fn from_bytes<'a>(input: &'a [u8]) -> Jsontokensizer<Cursor<&'a [u8]>> {
        let json_reader = JsonReader::<Cursor<&'a [u8]>>::from_bytes(input);
        Jsontokensizer {
            tokens: Vec::with_capacity(input.len()),
            iterator: json_reader.peekable(),
        }
    }
}

impl<T> Jsontokensizer<T>
where
    T: Read + Seek,
{
    pub fn tokensize_json(&mut self) -> Result<&[Token], ()> {
        while let Some(character) = self.iterator.peek() {
            match *character {
                '"' => {
                    self.tokens.push(Token::Quotes);

                    let _ = self.iterator.next();

                    let string = self.parse_string();

                    self.tokens.push(Token::String(string));

                    self.tokens.push(Token::Quotes);
                }
                '-' | '0'..='9' => {
                    let number = self.parse_number()?;
                    self.tokens.push(Token::Number(number));
                }
                't' => {
                    let _ = self.iterator.next();

                    assert_eq!(Some('r'), self.iterator.next());
                    assert_eq!(Some('u'), self.iterator.next());
                    assert_eq!(Some('e'), self.iterator.next());
                    self.tokens.push(Token::Boolean(true));
                }
                'f' => {
                    let _ = self.iterator.next();
                    assert_eq!(Some('a'), self.iterator.next());

                    assert_eq!(Some('l'), self.iterator.next());
                    assert_eq!(Some('s'), self.iterator.next());

                    assert_eq!(Some('e'), self.iterator.next());
                    self.tokens.push(Token::Boolean(false));
                }
                'n' => {
                    let _ = self.iterator.next();
                    assert_eq!(Some('u'), self.iterator.next());
                    assert_eq!(Some('l'), self.iterator.next());
                    assert_eq!(Some('l'), self.iterator.next());
                    self.tokens.push(Token::Null);
                }
                '{' => {
                    self.tokens.push(Token::CurlyOpen);
                    let _ = self.iterator.next();
                }
                '}' => {
                    self.tokens.push(Token::CurlyClose);
                    let _ = self.iterator.next();
                }
                '[' => {
                    self.tokens.push(Token::ArrayOpen);
                    let _ = self.iterator.next();
                }
                ']' => {
                    self.tokens.push(Token::ArrayClose);
                    let _ = self.iterator.next();
                }
                ',' => {
                    self.tokens.push(Token::Comma);
                    let _ = self.iterator.next();
                }
                ':' => {
                    self.tokens.push(Token::Colon);
                    let _ = self.iterator.next();
                }
                '\0' => break,
                other => {
                    if !other.is_ascii_whitespace() {
                        panic!("Unexpected token encountered: {other}")
                    } else {
                        self.iterator.next();
                    }
                }
            }
        }
        Ok(&self.tokens)
    }

    fn parse_string(&mut self) -> String {
        let mut string_characters = Vec::<char>::new();

        for character in self.iterator.by_ref() {
            if character == '"' {
                break;
            }

            string_characters.push(character);
        }
        String::from_iter(string_characters)
    }

    fn parse_number(&mut self) -> Result<Number, ()> {
        let mut number_characters = Vec::<char>::new();

        let mut is_decimal = false;

        let mut epsilon_characters = Vec::<char>::new();

        let mut is_epsilon_characters = false;

        while let Some(character) = self.iterator.peek() {
            match character {
                '-' => {
                    if is_epsilon_characters {
                        epsilon_characters.push('-');
                    } else {
                        number_characters.push('-');
                    }
                    let _ = self.iterator.next();
                }
                '+' => {
                    let _ = self.iterator.next();
                }
                digit @ '0'..='9' => {
                    if is_epsilon_characters {
                        epsilon_characters.push(*digit);
                    } else {
                        number_characters.push(*digit);
                    }
                    let _ = self.iterator.next();
                }
                '.' => {
                    number_characters.push('.');

                    is_decimal = true;

                    let _ = self.iterator.next();
                }
                '}' | ',' | ']' | ':' => {
                    break;
                }
                'e' | 'E' => {
                    if is_epsilon_characters {
                        panic!("Unexpected character while parsing number: {character}. Double epsilon characters encountered");
                    }
                    is_epsilon_characters = true;
                    let _ = self.iterator.next();
                }
                other => {
                    if !other.is_ascii_whitespace() {
                        panic!("Unexpected character while parsing number: {character}")
                    } else {
                        self.iterator.next();
                    }
                }
            }
        }

        if is_epsilon_characters {
            let base: f64 = String::from_iter(number_characters).parse().unwrap();
            let exponential: f64 = String::from_iter(epsilon_characters).parse().unwrap();
            Ok(Number::F64(base * 10_f64.powf(exponential)))
        } else if is_decimal {
            Ok(Number::F64(
                String::from_iter(number_characters).parse::<f64>().unwrap(),
            ))
        } else {
            Ok(Number::I64(
                String::from_iter(number_characters).parse::<i64>().unwrap(),
            ))
        }
    }
}
