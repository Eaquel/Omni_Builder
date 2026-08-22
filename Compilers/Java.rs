//! Java, compiled here rather than somewhere else.
//!
//! There is no `javac` on a phone, and there never will be. A build that needs
//! one is a build that does not happen on the device it is for, which is the
//! one thing this project exists to make possible. So this reads Java source
//! and writes class files itself.
//!
//! # What it compiles
//!
//! Enough Java to write an Android application, and it says plainly where that
//! ends. A compiler that quietly mis-handles what it does not understand is
//! worse than one that refuses, because the refusal happens at build time and
//! the mis-handling happens on somebody's phone.
//!
//! Handled: a package and its imports; one top-level class per file, with a
//! superclass and interfaces; fields, methods and constructors with the usual
//! modifiers; the primitive types, `String`, arrays and declared types;
//! blocks, local declarations, `if`/`else`, `while`, `for`, `return`, `break`,
//! `continue` and expression statements; literals, names, field access, method
//! invocation, `new`, the arithmetic, comparison, logical and bitwise
//! operators with Java's precedence, assignment and compound assignment,
//! `++`/`--`, casts, array indexing, `this`, `super`, the conditional
//! operator, and string concatenation with `+`.
//!
//! Refused, by name, with the line it happened on: generics, lambdas and
//! method references, inner and anonymous classes, interfaces and enums as
//! declarations, `switch`, `try`/`catch`/`finally`, `synchronized`, `assert`,
//! labelled statements, `do`/`while`, and varargs. Annotations are parsed and
//! discarded, which is safe because they are metadata and nothing here reads
//! them; `@Override` therefore costs nothing.
//!
//! # What it targets
//!
//! Class file major version 49, which is Java 5. Not because the language
//! stops there -- what is accepted above is a subset of every Java since -- but
//! because of how class files are verified. From version 50 the JVM uses the
//! type-checking verifier, which wants a StackMapTable attribute on every
//! method that branches, and from 51 a missing one is a hard failure rather
//! than a fallback. This does not write frames yet.
//!
//! Claiming 52 and omitting the frames would produce class files that read
//! fine, that `javap` prints happily, that pass through `d8` -- and that a real
//! JVM refuses the moment a method contains an `if`. Claiming 49 produces class
//! files that verify everywhere, today, at the cost of saying the language
//! level is older than the language actually used. The first is a lie that
//! surfaces on somebody else's machine; the second is a limitation written on
//! the tin. Frames are the next thing this needs, and until they exist the
//! contract says so.

use crate::caps::Capability;
use crate::compiler::{
    Compiled, Compiler, Expected, Identity, Kind, Plan, Probe, Reproducibility, Request, Session,
};
use crate::diag::{Diagnostic, Severity};
use crate::plugin::{Contract, Version};
use crate::FailureClass;
use crate::Status;

pub const ORIGIN: &str = "omni.plugin.java";

/// Java 5 class files, which verify without stack map frames. See the note at
/// the top of this file for why that is the honest choice and not a shortcut.
pub const CLASS_MAJOR: u16 = 49;
pub const CLASS_MINOR: u16 = 0;

/// The Java this accepts is a subset of every release from 8 onward. The
/// version named here is the one the project pins, and it reaches the compiler
/// identity so that changing it invalidates what the old one built.
pub const LANGUAGE_RELEASE: &str = "25";

fn fail(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        code,
        Severity::Error,
        FailureClass::UserError,
        ORIGIN,
        message,
    )
}

fn at(code: &str, line: u32, column: u32, message: impl Into<String>) -> Diagnostic {
    fail(code, message).with_context(format!("Line {line}, column {column}"))
}

fn unsupported(line: u32, column: u32, what: &str) -> Diagnostic {
    at(
        "EJ900",
        line,
        column,
        format!("{what} is not compiled here."),
    )
    .with_suggestion(
        "This compiler handles a subset of Java, and refuses the rest rather than \
         mis-handling it. What it does and does not take is written at the top of \
         Compilers/Java.rs.",
    )
}

// ------------------------------------------------------------------ lexer

#[derive(Clone, PartialEq, Debug)]
pub enum Token {
    Identifier(String),
    Keyword(&'static str),
    Int(i64),
    Long(i64),
    Float(f64),
    Double(f64),
    Char(u16),
    Str(String),
    True,
    False,
    Null,
    Punctuation(&'static str),
    End,
}

impl Token {
    fn describe(&self) -> String {
        match self {
            Token::Identifier(name) => format!("the name `{name}`"),
            Token::Keyword(word) => format!("`{word}`"),
            Token::Int(value) => format!("`{value}`"),
            Token::Long(value) => format!("`{value}L`"),
            Token::Float(value) => format!("`{value}f`"),
            Token::Double(value) => format!("`{value}`"),
            Token::Char(value) => format!("a character literal `{value}`"),
            Token::Str(text) => format!("the text \"{text}\""),
            Token::True => "`true`".to_string(),
            Token::False => "`false`".to_string(),
            Token::Null => "`null`".to_string(),
            Token::Punctuation(mark) => format!("`{mark}`"),
            Token::End => "the end of the file".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spelled {
    pub token: Token,
    pub line: u32,
    pub column: u32,
}

/// Every word Java reserves. All of them are recognised, including the ones
/// this compiler goes on to refuse: a keyword read as a name would turn a
/// clear refusal into a confusing one.
const KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
];

/// Longest first, so that `>>>=` is never read as `>>` and `>=`.
const MARKS: &[&str] = &[
    ">>>=", "<<=", ">>=", ">>>", "...", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "{", "}", "(", ")", "[", "]", ";",
    ",", ".", "=", ">", "<", "!", "~", "?", ":", "+", "-", "*", "/", "&", "|", "^", "%", "@",
];

pub struct Lexer<'a> {
    source: &'a [u8],
    at: usize,
    line: u32,
    column: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Lexer<'a> {
        Lexer {
            source: source.as_bytes(),
            at: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self, ahead: usize) -> u8 {
        *self.source.get(self.at + ahead).unwrap_or(&0)
    }

    fn bump(&mut self) -> u8 {
        let byte = self.peek(0);
        self.at += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        byte
    }

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            match (self.peek(0), self.peek(1)) {
                (b' ' | b'\t' | b'\r' | b'\n' | 0x0c, _) => {
                    self.bump();
                }
                (b'/', b'/') => {
                    while self.at < self.source.len() && self.peek(0) != b'\n' {
                        self.bump();
                    }
                }
                (b'/', b'*') => {
                    let (line, column) = (self.line, self.column);
                    self.bump();
                    self.bump();
                    loop {
                        if self.at >= self.source.len() {
                            return Err(at(
                                "EJ001",
                                line,
                                column,
                                "A comment was opened and never closed.",
                            ));
                        }
                        if self.peek(0) == b'*' && self.peek(1) == b'/' {
                            self.bump();
                            self.bump();
                            break;
                        }
                        self.bump();
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// Every token in the source, with the line and column each was found at.
    pub fn tokens(mut self) -> Result<Vec<Spelled>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia()?;
            let (line, column) = (self.line, self.column);
            if self.at >= self.source.len() {
                out.push(Spelled {
                    token: Token::End,
                    line,
                    column,
                });
                return Ok(out);
            }
            let token = self.one(line, column)?;
            out.push(Spelled {
                token,
                line,
                column,
            });
        }
    }

    fn one(&mut self, line: u32, column: u32) -> Result<Token, Diagnostic> {
        let byte = self.peek(0);

        if byte == b'"' {
            return self.text(line, column);
        }
        if byte == b'\'' {
            return self.character(line, column);
        }
        if byte.is_ascii_digit() || (byte == b'.' && self.peek(1).is_ascii_digit()) {
            return self.number(line, column);
        }
        if byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic() || byte >= 0x80 {
            return Ok(self.word());
        }

        for mark in MARKS {
            let bytes = mark.as_bytes();
            if self.source[self.at..].starts_with(bytes) {
                for _ in 0..bytes.len() {
                    self.bump();
                }
                return Ok(Token::Punctuation(mark));
            }
        }

        Err(at(
            "EJ002",
            line,
            column,
            format!("`{}` is not part of Java.", byte as char),
        ))
    }

    fn word(&mut self) -> Token {
        let start = self.at;
        while {
            let byte = self.peek(0);
            byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric() || byte >= 0x80
        } {
            self.bump();
        }
        let text = String::from_utf8_lossy(&self.source[start..self.at]).into_owned();
        match text.as_str() {
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            _ => match KEYWORDS.iter().find(|word| **word == text) {
                Some(word) => Token::Keyword(word),
                None => Token::Identifier(text),
            },
        }
    }

    /// A backslash escape, as Java writes them. The value is a UTF-16 code
    /// unit, because that is what a Java `char` is.
    fn escape(&mut self, line: u32, column: u32) -> Result<u16, Diagnostic> {
        self.bump(); // the backslash
        let byte = self.bump();
        Ok(match byte {
            b'n' => 10,
            b't' => 9,
            b'b' => 8,
            b'r' => 13,
            b'f' => 12,
            b's' => 32,
            b'0'..=b'7' => {
                let mut value = u16::from(byte - b'0');
                while self.peek(0).is_ascii_digit() && self.peek(0) < b'8' && value < 32 {
                    value = value * 8 + u16::from(self.bump() - b'0');
                }
                value
            }
            b'\'' => 39,
            b'"' => 34,
            b'\\' => 92,
            b'u' => {
                while self.peek(0) == b'u' {
                    self.bump();
                }
                let mut value = 0u16;
                for _ in 0..4 {
                    let digit = (self.bump() as char).to_digit(16).ok_or_else(|| {
                        at("EJ003", line, column, "A \\u escape needs four hex digits.")
                    })?;
                    value = value * 16 + digit as u16;
                }
                value
            }
            other => {
                return Err(at(
                    "EJ004",
                    line,
                    column,
                    format!("`\\{}` is not an escape Java knows.", other as char),
                ))
            }
        })
    }

    fn text(&mut self, line: u32, column: u32) -> Result<Token, Diagnostic> {
        self.bump(); // the opening quote
        let mut units: Vec<u16> = Vec::new();
        loop {
            match self.peek(0) {
                0 if self.at >= self.source.len() => {
                    return Err(at("EJ005", line, column, "A string was never closed."))
                }
                b'\n' => return Err(at("EJ005", line, column, "A string was never closed.")),
                b'"' => {
                    self.bump();
                    break;
                }
                b'\\' => units.push(self.escape(line, column)?),
                _ => {
                    // Kept as it is written, which for anything outside ASCII
                    // means gathering the whole character rather than a byte of
                    // it.
                    let start = self.at;
                    self.bump();
                    while self.peek(0) >= 0x80 && self.peek(0) < 0xc0 {
                        self.bump();
                    }
                    let piece = String::from_utf8_lossy(&self.source[start..self.at]).into_owned();
                    units.extend(piece.encode_utf16());
                }
            }
        }
        Ok(Token::Str(String::from_utf16_lossy(&units)))
    }

    fn character(&mut self, line: u32, column: u32) -> Result<Token, Diagnostic> {
        self.bump(); // the opening quote
        let value = if self.peek(0) == b'\\' {
            self.escape(line, column)?
        } else {
            let start = self.at;
            self.bump();
            while self.peek(0) >= 0x80 && self.peek(0) < 0xc0 {
                self.bump();
            }
            let piece = String::from_utf8_lossy(&self.source[start..self.at]).into_owned();
            piece.encode_utf16().next().unwrap_or(0)
        };
        if self.peek(0) != b'\'' {
            return Err(at(
                "EJ006",
                line,
                column,
                "A character literal holds exactly one character.",
            ));
        }
        self.bump();
        Ok(Token::Char(value))
    }

    fn number(&mut self, line: u32, column: u32) -> Result<Token, Diagnostic> {
        let start = self.at;
        let mut radix = 10u32;
        if self.peek(0) == b'0' && matches!(self.peek(1), b'x' | b'X') {
            radix = 16;
            self.bump();
            self.bump();
        } else if self.peek(0) == b'0' && matches!(self.peek(1), b'b' | b'B') {
            radix = 2;
            self.bump();
            self.bump();
        }
        let digits_from = self.at;

        let mut floating = false;
        loop {
            let byte = self.peek(0);
            if byte == b'_' {
                self.bump();
                continue;
            }
            if (byte as char).is_digit(radix) {
                self.bump();
                continue;
            }
            if radix == 10 && byte == b'.' && self.peek(1) != b'.' && !floating {
                floating = true;
                self.bump();
                continue;
            }
            if radix == 10 && matches!(byte, b'e' | b'E') {
                floating = true;
                self.bump();
                if matches!(self.peek(0), b'+' | b'-') {
                    self.bump();
                }
                continue;
            }
            break;
        }

        let digits: String = String::from_utf8_lossy(&self.source[digits_from..self.at])
            .chars()
            .filter(|c| *c != '_')
            .collect();

        // A leading zero on a decimal integer is octal in Java, which is a trap
        // rather than a feature; it is refused rather than read either way.
        if radix == 10 && !floating && digits.len() > 1 && digits.starts_with('0') {
            return Err(at(
                "EJ007",
                line,
                column,
                "A number written with a leading zero is octal in Java.",
            )
            .with_suggestion("Write it in decimal, or as 0x for hexadecimal."));
        }

        let suffix = self.peek(0);
        if matches!(suffix, b'l' | b'L') {
            self.bump();
            let value = parse_integer(&digits, radix, true).ok_or_else(|| {
                at(
                    "EJ008",
                    line,
                    column,
                    "That number is too large for a long.",
                )
            })?;
            return Ok(Token::Long(value));
        }
        if matches!(suffix, b'f' | b'F') {
            self.bump();
            let value: f64 = digits
                .parse()
                .map_err(|_| at("EJ009", line, column, "That is not a number this reads."))?;
            return Ok(Token::Float(value));
        }
        if matches!(suffix, b'd' | b'D') {
            self.bump();
            floating = true;
        }

        if floating {
            let value: f64 = digits
                .parse()
                .map_err(|_| at("EJ009", line, column, "That is not a number this reads."))?;
            return Ok(Token::Double(value));
        }

        let _ = start;
        let value = parse_integer(&digits, radix, false).ok_or_else(|| {
            at(
                "EJ010",
                line,
                column,
                "That number is too large for an int.",
            )
        })?;
        Ok(Token::Int(value))
    }
}

/// Java's integer literals are unsigned in the source and signed in the value:
/// `0x80000000` is a legal `int` and it is negative. Parsing has to allow the
/// whole unsigned range and then reinterpret.
fn parse_integer(digits: &str, radix: u32, long: bool) -> Option<i64> {
    let value = u64::from_str_radix(digits, radix).ok()?;
    if long {
        return Some(value as i64);
    }
    if radix == 10 {
        // The one decimal literal allowed to exceed the range is 2147483648,
        // and only directly under a unary minus. Allowing it everywhere is a
        // small looseness against a large amount of machinery.
        if value > 2_147_483_648 {
            return None;
        }
        return Some(value as i32 as i64);
    }
    if value > u64::from(u32::MAX) {
        return None;
    }
    Some(value as u32 as i32 as i64)
}

// -------------------------------------------------------------------- ast

/// A type as it was written. Nothing is resolved yet: `Activity` is a name
/// here and becomes a class later, or fails to.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Written {
    Void,
    Boolean,
    Byte,
    Short,
    Char,
    Int,
    Long,
    Float,
    Double,
    Named(String),
    Array(Box<Written>),
}

impl Written {
    fn of_keyword(word: &str) -> Option<Written> {
        Some(match word {
            "void" => Written::Void,
            "boolean" => Written::Boolean,
            "byte" => Written::Byte,
            "short" => Written::Short,
            "char" => Written::Char,
            "int" => Written::Int,
            "long" => Written::Long,
            "float" => Written::Float,
            "double" => Written::Double,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unary {
    Negate,
    Not,
    Complement,
    Plus,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
    And,
    Or,
    Xor,
    ShiftLeft,
    ShiftRight,
    UnsignedShiftRight,
    AndAlso,
    OrElse,
}

impl Binary {
    fn of_mark(mark: &str) -> Option<Binary> {
        Some(match mark {
            "+" => Binary::Add,
            "-" => Binary::Subtract,
            "*" => Binary::Multiply,
            "/" => Binary::Divide,
            "%" => Binary::Remainder,
            "<" => Binary::Less,
            "<=" => Binary::LessOrEqual,
            ">" => Binary::Greater,
            ">=" => Binary::GreaterOrEqual,
            "==" => Binary::Equal,
            "!=" => Binary::NotEqual,
            "&" => Binary::And,
            "|" => Binary::Or,
            "^" => Binary::Xor,
            "<<" => Binary::ShiftLeft,
            ">>" => Binary::ShiftRight,
            ">>>" => Binary::UnsignedShiftRight,
            "&&" => Binary::AndAlso,
            "||" => Binary::OrElse,
            _ => return None,
        })
    }

    /// Java's own table. Higher binds tighter.
    fn precedence(self) -> u8 {
        match self {
            Binary::Multiply | Binary::Divide | Binary::Remainder => 12,
            Binary::Add | Binary::Subtract => 11,
            Binary::ShiftLeft | Binary::ShiftRight | Binary::UnsignedShiftRight => 10,
            Binary::Less | Binary::LessOrEqual | Binary::Greater | Binary::GreaterOrEqual => 9,
            Binary::Equal | Binary::NotEqual => 8,
            Binary::And => 7,
            Binary::Xor => 6,
            Binary::Or => 5,
            Binary::AndAlso => 4,
            Binary::OrElse => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Expression {
    Int(i64),
    Long(i64),
    Float(f64),
    Double(f64),
    Char(u16),
    Str(String),
    Boolean(bool),
    Null,
    This,
    /// A bare name: a local, a parameter, a field of this class, or the start
    /// of a qualified name. Which one it is is decided by the checker.
    Name(String),
    Field {
        of: Box<Expression>,
        name: String,
    },
    Call {
        /// Absent for a bare `name(...)`, which is a call on `this` or a static
        /// call on this class.
        on: Option<Box<Expression>>,
        /// Whether it was written `super.name(...)`.
        super_call: bool,
        name: String,
        arguments: Vec<Expression>,
    },
    New {
        what: Written,
        arguments: Vec<Expression>,
    },
    NewArray {
        of: Written,
        length: Box<Expression>,
    },
    Index {
        of: Box<Expression>,
        at: Box<Expression>,
    },
    Unary {
        operator: Unary,
        of: Box<Expression>,
    },
    Binary {
        operator: Binary,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Assign {
        target: Box<Expression>,
        /// `None` for `=`, otherwise the operator of a compound assignment.
        operator: Option<Binary>,
        value: Box<Expression>,
    },
    /// `++x`, `x++`, and the two decrements.
    Step {
        target: Box<Expression>,
        by: i32,
        after: bool,
    },
    Cast {
        to: Written,
        of: Box<Expression>,
    },
    Conditional {
        condition: Box<Expression>,
        then: Box<Expression>,
        otherwise: Box<Expression>,
    },
    InstanceOf {
        of: Box<Expression>,
        what: Written,
    },
}

#[derive(Clone, Debug)]
pub struct Positioned<T> {
    pub node: T,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug)]
pub enum Statement {
    Block(Vec<Positioned<Statement>>),
    Declare {
        what: Written,
        name: String,
        value: Option<Expression>,
    },
    Express(Expression),
    If {
        condition: Expression,
        then: Box<Positioned<Statement>>,
        otherwise: Option<Box<Positioned<Statement>>>,
    },
    While {
        condition: Expression,
        body: Box<Positioned<Statement>>,
    },
    For {
        start: Vec<Positioned<Statement>>,
        condition: Option<Expression>,
        step: Vec<Expression>,
        body: Box<Positioned<Statement>>,
    },
    Return(Option<Expression>),
    Break,
    Continue,
    Nothing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    pub public: bool,
    pub private: bool,
    pub protected: bool,
    pub static_: bool,
    pub final_: bool,
    pub abstract_: bool,
}

impl Modifiers {
    /// What the class file records. The values are the ones the JVM
    /// specification gives, and nothing here invents any.
    pub fn access_flags(self, and: u16) -> u16 {
        let mut flags = and;
        if self.public {
            flags |= 0x0001;
        }
        if self.private {
            flags |= 0x0002;
        }
        if self.protected {
            flags |= 0x0004;
        }
        if self.static_ {
            flags |= 0x0008;
        }
        if self.final_ {
            flags |= 0x0010;
        }
        if self.abstract_ {
            flags |= 0x0400;
        }
        flags
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    pub modifiers: Modifiers,
    pub what: Written,
    pub name: String,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub modifiers: Modifiers,
    pub returns: Written,
    pub name: String,
    pub parameters: Vec<(Written, String)>,
    pub body: Option<Vec<Positioned<Statement>>>,
    pub constructor: bool,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub modifiers: Modifiers,
    pub name: String,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
}

impl Unit {
    /// The name the class file records: the package and the class, separated
    /// by slashes.
    pub fn internal_name(&self) -> String {
        match &self.package {
            Some(package) => format!("{}/{}", package.replace('.', "/"), self.name),
            None => self.name.clone(),
        }
    }
}

// ----------------------------------------------------------------- parser

pub struct Parser {
    tokens: Vec<Spelled>,
    at: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spelled>) -> Parser {
        Parser { tokens, at: 0 }
    }

    fn here(&self) -> &Spelled {
        &self.tokens[self.at.min(self.tokens.len() - 1)]
    }

    fn ahead(&self, by: usize) -> &Token {
        &self.tokens[(self.at + by).min(self.tokens.len() - 1)].token
    }

    fn line(&self) -> u32 {
        self.here().line
    }

    fn column(&self) -> u32 {
        self.here().column
    }

    fn take(&mut self) -> Token {
        let token = self.here().token.clone();
        if self.at < self.tokens.len() - 1 {
            self.at += 1;
        }
        token
    }

    fn is_mark(&self, mark: &str) -> bool {
        matches!(&self.here().token, Token::Punctuation(found) if *found == mark)
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(&self.here().token, Token::Keyword(found) if *found == word)
    }

    fn eat_mark(&mut self, mark: &str) -> bool {
        if self.is_mark(mark) {
            self.take();
            return true;
        }
        false
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.is_word(word) {
            self.take();
            return true;
        }
        false
    }

    fn want_mark(&mut self, mark: &str) -> Result<(), Diagnostic> {
        if self.eat_mark(mark) {
            return Ok(());
        }
        Err(at(
            "EJ100",
            self.line(),
            self.column(),
            format!(
                "`{mark}` was expected, and {} was found.",
                self.here().token.describe()
            ),
        ))
    }

    fn want_name(&mut self) -> Result<String, Diagnostic> {
        match self.take() {
            Token::Identifier(name) => Ok(name),
            other => Err(at(
                "EJ101",
                self.line(),
                self.column(),
                format!("A name was expected, and {} was found.", other.describe()),
            )),
        }
    }

    /// A dotted name, as a package or an import writes one.
    fn qualified(&mut self) -> Result<String, Diagnostic> {
        let mut out = self.want_name()?;
        while self.is_mark(".") && matches!(self.ahead(1), Token::Identifier(_)) {
            self.take();
            out.push('.');
            out.push_str(&self.want_name()?);
        }
        Ok(out)
    }

    /// Annotations are read and thrown away. They are metadata, nothing here
    /// reads them, and refusing `@Override` would refuse most real Java for no
    /// reason at all.
    fn skip_annotations(&mut self) -> Result<(), Diagnostic> {
        while self.is_mark("@") {
            self.take();
            self.qualified()?;
            if self.is_mark("(") {
                let mut depth = 0usize;
                loop {
                    if self.is_mark("(") {
                        depth += 1;
                    } else if self.is_mark(")") {
                        depth -= 1;
                        if depth == 0 {
                            self.take();
                            break;
                        }
                    } else if matches!(self.here().token, Token::End) {
                        return Err(at(
                            "EJ102",
                            self.line(),
                            self.column(),
                            "An annotation was opened and never closed.",
                        ));
                    }
                    self.take();
                }
            }
        }
        Ok(())
    }

    fn modifiers(&mut self) -> Result<Modifiers, Diagnostic> {
        let mut found = Modifiers::default();
        loop {
            self.skip_annotations()?;
            let word = match &self.here().token {
                Token::Keyword(word) => *word,
                _ => break,
            };
            match word {
                "public" => found.public = true,
                "private" => found.private = true,
                "protected" => found.protected = true,
                "static" => found.static_ = true,
                "final" => found.final_ = true,
                "abstract" => found.abstract_ = true,
                "native" | "synchronized" | "transient" | "volatile" | "strictfp" => {
                    return Err(unsupported(
                        self.line(),
                        self.column(),
                        format!("`{word}`").as_str(),
                    ))
                }
                _ => break,
            }
            self.take();
        }
        Ok(found)
    }

    /// A type, with however many `[]` follow it.
    fn written_type(&mut self) -> Result<Written, Diagnostic> {
        let base = match self.here().token.clone() {
            Token::Keyword(word) => match Written::of_keyword(word) {
                Some(found) => {
                    self.take();
                    found
                }
                None => {
                    return Err(at(
                        "EJ103",
                        self.line(),
                        self.column(),
                        format!("A type was expected, and `{word}` was found."),
                    ))
                }
            },
            Token::Identifier(_) => Written::Named(self.qualified()?),
            other => {
                return Err(at(
                    "EJ103",
                    self.line(),
                    self.column(),
                    format!("A type was expected, and {} was found.", other.describe()),
                ))
            }
        };

        if self.is_mark("<") {
            return Err(unsupported(self.line(), self.column(), "A generic type"));
        }

        let mut found = base;
        while self.is_mark("[") && matches!(self.ahead(1), Token::Punctuation("]")) {
            self.take();
            self.take();
            found = Written::Array(Box::new(found));
        }
        Ok(found)
    }

    /// Whether what follows starts a local variable declaration rather than an
    /// expression. `Foo bar` is a declaration; `foo.bar()` is not.
    fn looks_like_declaration(&self) -> bool {
        match &self.here().token {
            Token::Keyword(word) => Written::of_keyword(word).is_some(),
            Token::Identifier(_) => {
                let mut ahead = 0usize;
                // Walk a dotted name.
                loop {
                    if !matches!(self.ahead(ahead), Token::Identifier(_)) {
                        return false;
                    }
                    ahead += 1;
                    if matches!(self.ahead(ahead), Token::Punctuation(".")) {
                        ahead += 1;
                        continue;
                    }
                    break;
                }
                // Then any number of `[]`.
                while matches!(self.ahead(ahead), Token::Punctuation("["))
                    && matches!(self.ahead(ahead + 1), Token::Punctuation("]"))
                {
                    ahead += 2;
                }
                matches!(self.ahead(ahead), Token::Identifier(_))
            }
            _ => false,
        }
    }

    pub fn unit(&mut self) -> Result<Unit, Diagnostic> {
        self.skip_annotations()?;
        let package = if self.eat_word("package") {
            let name = self.qualified()?;
            self.want_mark(";")?;
            Some(name)
        } else {
            None
        };

        let mut imports = Vec::new();
        while self.is_word("import") {
            self.take();
            if self.eat_word("static") {
                return Err(unsupported(self.line(), self.column(), "A static import"));
            }
            let mut name = self.qualified()?;
            if self.eat_mark(".") {
                self.want_mark("*")?;
                name.push_str(".*");
            }
            self.want_mark(";")?;
            imports.push(name);
        }

        let modifiers = self.modifiers()?;
        for (word, what) in [
            ("interface", "An interface declaration"),
            ("enum", "An enum declaration"),
        ] {
            if self.is_word(word) {
                return Err(unsupported(self.line(), self.column(), what));
            }
        }
        if !self.eat_word("class") {
            return Err(at(
                "EJ104",
                self.line(),
                self.column(),
                "A file compiled here holds one class.",
            )
            .with_suggestion(
                "Interfaces, enums and records are not compiled here yet. What is and \
                 is not taken is written at the top of Compilers/Java.rs.",
            ));
        }

        let name = self.want_name()?;
        if self.is_mark("<") {
            return Err(unsupported(self.line(), self.column(), "A generic class"));
        }

        let extends = if self.eat_word("extends") {
            Some(self.qualified()?)
        } else {
            None
        };
        let mut implements = Vec::new();
        if self.eat_word("implements") {
            loop {
                implements.push(self.qualified()?);
                if !self.eat_mark(",") {
                    break;
                }
            }
        }

        self.want_mark("{")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !self.is_mark("}") {
            if matches!(self.here().token, Token::End) {
                return Err(at(
                    "EJ105",
                    self.line(),
                    self.column(),
                    "The class was opened and never closed.",
                ));
            }
            if self.eat_mark(";") {
                continue;
            }
            self.member(&name, &mut fields, &mut methods)?;
        }
        self.want_mark("}")?;

        Ok(Unit {
            package,
            imports,
            modifiers,
            name,
            extends,
            implements,
            fields,
            methods,
        })
    }

    fn member(
        &mut self,
        class: &str,
        fields: &mut Vec<Field>,
        methods: &mut Vec<Method>,
    ) -> Result<(), Diagnostic> {
        let line = self.line();
        let modifiers = self.modifiers()?;

        for (word, what) in [
            ("class", "A nested class"),
            ("interface", "A nested interface"),
            ("enum", "A nested enum"),
        ] {
            if self.is_word(word) {
                return Err(unsupported(self.line(), self.column(), what));
            }
        }
        if self.is_mark("{") {
            return Err(unsupported(
                self.line(),
                self.column(),
                "An initialiser block",
            ));
        }

        // A constructor is the class's own name followed by a parameter list.
        if matches!(&self.here().token, Token::Identifier(found) if found == class)
            && matches!(self.ahead(1), Token::Punctuation("("))
        {
            self.take();
            let parameters = self.parameters()?;
            self.throws()?;
            let body = self.method_body()?;
            methods.push(Method {
                modifiers,
                returns: Written::Void,
                name: "<init>".to_string(),
                parameters,
                body,
                constructor: true,
                line,
            });
            return Ok(());
        }

        let what = self.written_type()?;
        let name = self.want_name()?;

        if self.is_mark("(") {
            let parameters = self.parameters()?;
            self.throws()?;
            let body = self.method_body()?;
            if body.is_none() && !modifiers.abstract_ {
                return Err(at(
                    "EJ106",
                    line,
                    1,
                    format!("`{name}` has no body and is not abstract."),
                ));
            }
            methods.push(Method {
                modifiers,
                returns: what,
                name,
                parameters,
                body,
                constructor: false,
                line,
            });
            return Ok(());
        }

        // One or more fields, possibly with initialisers, separated by commas.
        let mut names = vec![name];
        loop {
            if self.is_mark("=") {
                return Err(unsupported(
                    self.line(),
                    self.column(),
                    "A field with a value written on it",
                ));
            }
            if !self.eat_mark(",") {
                break;
            }
            names.push(self.want_name()?);
        }
        self.want_mark(";")?;
        for name in names {
            fields.push(Field {
                modifiers,
                what: what.clone(),
                name,
                line,
            });
        }
        Ok(())
    }

    fn parameters(&mut self) -> Result<Vec<(Written, String)>, Diagnostic> {
        self.want_mark("(")?;
        let mut found = Vec::new();
        if !self.is_mark(")") {
            loop {
                self.skip_annotations()?;
                self.eat_word("final");
                let what = self.written_type()?;
                if self.is_mark("...") {
                    return Err(unsupported(
                        self.line(),
                        self.column(),
                        "A varargs parameter",
                    ));
                }
                let name = self.want_name()?;
                found.push((what, name));
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        self.want_mark(")")?;
        Ok(found)
    }

    /// A `throws` clause is read and thrown away: nothing here checks what a
    /// method may throw, and recording the list would suggest otherwise.
    fn throws(&mut self) -> Result<(), Diagnostic> {
        if self.eat_word("throws") {
            loop {
                self.qualified()?;
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        Ok(())
    }

    fn method_body(&mut self) -> Result<Option<Vec<Positioned<Statement>>>, Diagnostic> {
        if self.eat_mark(";") {
            return Ok(None);
        }
        self.want_mark("{")?;
        let mut found = Vec::new();
        while !self.is_mark("}") {
            if matches!(self.here().token, Token::End) {
                return Err(at(
                    "EJ107",
                    self.line(),
                    self.column(),
                    "A method body was opened and never closed.",
                ));
            }
            found.push(self.statement()?);
        }
        self.want_mark("}")?;
        Ok(Some(found))
    }

    fn statement(&mut self) -> Result<Positioned<Statement>, Diagnostic> {
        let (line, column) = (self.line(), self.column());
        let node = self.statement_node(line, column)?;
        Ok(Positioned { node, line, column })
    }

    fn statement_node(&mut self, line: u32, column: u32) -> Result<Statement, Diagnostic> {
        for (word, what) in [
            ("switch", "`switch`"),
            ("try", "`try`"),
            ("synchronized", "`synchronized`"),
            ("assert", "`assert`"),
            ("do", "`do`/`while`"),
            ("throw", "`throw`"),
        ] {
            if self.is_word(word) {
                return Err(unsupported(line, column, what));
            }
        }

        if self.is_mark("{") {
            self.take();
            let mut found = Vec::new();
            while !self.is_mark("}") {
                if matches!(self.here().token, Token::End) {
                    return Err(at("EJ108", line, column, "A block was never closed."));
                }
                found.push(self.statement()?);
            }
            self.want_mark("}")?;
            return Ok(Statement::Block(found));
        }

        if self.eat_mark(";") {
            return Ok(Statement::Nothing);
        }

        if self.eat_word("if") {
            self.want_mark("(")?;
            let condition = self.expression()?;
            self.want_mark(")")?;
            let then = Box::new(self.statement()?);
            let otherwise = if self.eat_word("else") {
                Some(Box::new(self.statement()?))
            } else {
                None
            };
            return Ok(Statement::If {
                condition,
                then,
                otherwise,
            });
        }

        if self.eat_word("while") {
            self.want_mark("(")?;
            let condition = self.expression()?;
            self.want_mark(")")?;
            let body = Box::new(self.statement()?);
            return Ok(Statement::While { condition, body });
        }

        if self.eat_word("for") {
            return self.for_statement(line, column);
        }

        if self.eat_word("return") {
            let value = if self.is_mark(";") {
                None
            } else {
                Some(self.expression()?)
            };
            self.want_mark(";")?;
            return Ok(Statement::Return(value));
        }

        if self.eat_word("break") {
            if matches!(self.here().token, Token::Identifier(_)) {
                return Err(unsupported(line, column, "A labelled `break`"));
            }
            self.want_mark(";")?;
            return Ok(Statement::Break);
        }

        if self.eat_word("continue") {
            if matches!(self.here().token, Token::Identifier(_)) {
                return Err(unsupported(line, column, "A labelled `continue`"));
            }
            self.want_mark(";")?;
            return Ok(Statement::Continue);
        }

        if self.looks_like_declaration() {
            let statement = self.declaration()?;
            self.want_mark(";")?;
            return Ok(statement);
        }

        let expression = self.expression()?;
        self.want_mark(";")?;
        Ok(Statement::Express(expression))
    }

    fn declaration(&mut self) -> Result<Statement, Diagnostic> {
        let what = self.written_type()?;
        let name = self.want_name()?;
        let value = if self.eat_mark("=") {
            Some(self.expression()?)
        } else {
            None
        };
        if self.is_mark(",") {
            return Err(unsupported(
                self.line(),
                self.column(),
                "More than one variable in one declaration",
            ));
        }
        Ok(Statement::Declare { what, name, value })
    }

    fn for_statement(&mut self, line: u32, column: u32) -> Result<Statement, Diagnostic> {
        self.want_mark("(")?;

        // `for (Type name : thing)` is the enhanced form, which needs an
        // iterator and is not compiled here.
        {
            let mut ahead = 0usize;
            let mut depth = 0usize;
            loop {
                match self.ahead(ahead) {
                    Token::Punctuation("(") => depth += 1,
                    Token::Punctuation(")") if depth == 0 => break,
                    Token::Punctuation(")") => depth -= 1,
                    Token::Punctuation(";") if depth == 0 => break,
                    Token::Punctuation(":") if depth == 0 => {
                        return Err(unsupported(line, column, "A `for` over a collection"))
                    }
                    Token::End => break,
                    _ => {}
                }
                ahead += 1;
            }
        }

        let mut start = Vec::new();
        if !self.is_mark(";") {
            let (line, column) = (self.line(), self.column());
            let node = if self.looks_like_declaration() {
                self.declaration()?
            } else {
                Statement::Express(self.expression()?)
            };
            start.push(Positioned { node, line, column });
        }
        self.want_mark(";")?;

        let condition = if self.is_mark(";") {
            None
        } else {
            Some(self.expression()?)
        };
        self.want_mark(";")?;

        let mut step = Vec::new();
        if !self.is_mark(")") {
            loop {
                step.push(self.expression()?);
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        self.want_mark(")")?;

        let body = Box::new(self.statement()?);
        Ok(Statement::For {
            start,
            condition,
            step,
            body,
        })
    }

    pub fn expression(&mut self) -> Result<Expression, Diagnostic> {
        let left = self.binary(0)?;

        if self.is_mark("?") {
            self.take();
            let then = self.expression()?;
            self.want_mark(":")?;
            let otherwise = self.expression()?;
            return Ok(Expression::Conditional {
                condition: Box::new(left),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            });
        }

        let compound = [
            ("=", None),
            ("+=", Some(Binary::Add)),
            ("-=", Some(Binary::Subtract)),
            ("*=", Some(Binary::Multiply)),
            ("/=", Some(Binary::Divide)),
            ("%=", Some(Binary::Remainder)),
            ("&=", Some(Binary::And)),
            ("|=", Some(Binary::Or)),
            ("^=", Some(Binary::Xor)),
            ("<<=", Some(Binary::ShiftLeft)),
            (">>=", Some(Binary::ShiftRight)),
            (">>>=", Some(Binary::UnsignedShiftRight)),
        ];
        for (mark, operator) in compound {
            if self.is_mark(mark) {
                self.take();
                let value = self.expression()?;
                return Ok(Expression::Assign {
                    target: Box::new(left),
                    operator,
                    value: Box::new(value),
                });
            }
        }

        Ok(left)
    }

    fn binary(&mut self, least: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.unary()?;
        loop {
            if self.is_word("instanceof") {
                if least > 9 {
                    break;
                }
                self.take();
                let what = self.written_type()?;
                left = Expression::InstanceOf {
                    of: Box::new(left),
                    what,
                };
                continue;
            }
            let Token::Punctuation(mark) = self.here().token else {
                break;
            };
            let Some(operator) = Binary::of_mark(mark) else {
                break;
            };
            if operator.precedence() < least {
                break;
            }
            self.take();
            let right = self.binary(operator.precedence() + 1)?;
            left = Expression::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expression, Diagnostic> {
        for (mark, by) in [("++", 1i32), ("--", -1)] {
            if self.is_mark(mark) {
                self.take();
                let target = self.unary()?;
                return Ok(Expression::Step {
                    target: Box::new(target),
                    by,
                    after: false,
                });
            }
        }
        for (mark, operator) in [
            ("-", Unary::Negate),
            ("+", Unary::Plus),
            ("!", Unary::Not),
            ("~", Unary::Complement),
        ] {
            if self.is_mark(mark) {
                self.take();
                let of = self.unary()?;
                // `-5` is the literal, not a negation of it, so that
                // -2147483648 can be written at all.
                if operator == Unary::Negate {
                    if let Expression::Int(value) = of {
                        return Ok(Expression::Int(-value));
                    }
                    if let Expression::Long(value) = of {
                        return Ok(Expression::Long(value.wrapping_neg()));
                    }
                }
                return Ok(Expression::Unary {
                    operator,
                    of: Box::new(of),
                });
            }
        }

        // A cast is `(Type) expression`, and telling it from a parenthesised
        // expression takes a look ahead: `(a)` alone is a name, `(a) b` is a
        // cast, and `(int) x` is always a cast.
        if self.is_mark("(") && self.looks_like_cast() {
            self.take();
            let to = self.written_type()?;
            self.want_mark(")")?;
            let of = self.unary()?;
            return Ok(Expression::Cast {
                to,
                of: Box::new(of),
            });
        }

        self.postfix()
    }

    fn looks_like_cast(&self) -> bool {
        if let Token::Keyword(word) = self.ahead(1) {
            if Written::of_keyword(word).is_some() {
                return true;
            }
        }
        if !matches!(self.ahead(1), Token::Identifier(_)) {
            return false;
        }
        let mut ahead = 1usize;
        loop {
            if !matches!(self.ahead(ahead), Token::Identifier(_)) {
                return false;
            }
            ahead += 1;
            if matches!(self.ahead(ahead), Token::Punctuation(".")) {
                ahead += 1;
                continue;
            }
            break;
        }
        while matches!(self.ahead(ahead), Token::Punctuation("["))
            && matches!(self.ahead(ahead + 1), Token::Punctuation("]"))
        {
            ahead += 2;
        }
        if !matches!(self.ahead(ahead), Token::Punctuation(")")) {
            return false;
        }
        // `(Name) something` is a cast; `(name) + 1` is arithmetic on a name.
        matches!(
            self.ahead(ahead + 1),
            Token::Identifier(_)
                | Token::Str(_)
                | Token::Int(_)
                | Token::Long(_)
                | Token::Char(_)
                | Token::True
                | Token::False
                | Token::Null
                | Token::Keyword("this")
                | Token::Keyword("new")
                | Token::Punctuation("(")
        )
    }

    fn postfix(&mut self) -> Result<Expression, Diagnostic> {
        let mut found = self.primary()?;
        loop {
            if self.is_mark(".") {
                self.take();
                let name = self.want_name()?;
                if self.is_mark("(") {
                    let arguments = self.arguments()?;
                    found = Expression::Call {
                        on: Some(Box::new(found)),
                        super_call: false,
                        name,
                        arguments,
                    };
                } else {
                    found = Expression::Field {
                        of: Box::new(found),
                        name,
                    };
                }
                continue;
            }
            if self.is_mark("[") {
                self.take();
                let index = self.expression()?;
                self.want_mark("]")?;
                found = Expression::Index {
                    of: Box::new(found),
                    at: Box::new(index),
                };
                continue;
            }
            for (mark, by) in [("++", 1i32), ("--", -1)] {
                if self.is_mark(mark) {
                    self.take();
                    found = Expression::Step {
                        target: Box::new(found),
                        by,
                        after: true,
                    };
                }
            }
            break;
        }
        Ok(found)
    }

    fn arguments(&mut self) -> Result<Vec<Expression>, Diagnostic> {
        self.want_mark("(")?;
        let mut found = Vec::new();
        if !self.is_mark(")") {
            loop {
                found.push(self.expression()?);
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        self.want_mark(")")?;
        Ok(found)
    }

    /// Whether the bracket here opens a lambda's parameter list.
    ///
    /// Without this, `() -> {}` is read as an empty pair of brackets and
    /// refused for the bracket rather than for the lambda, which tells the
    /// person nothing about what is actually not supported.
    fn looks_like_lambda(&self) -> bool {
        let mut ahead = 1usize;
        let mut depth = 1usize;
        loop {
            match self.ahead(ahead) {
                Token::Punctuation("(") => depth += 1,
                Token::Punctuation(")") => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.ahead(ahead + 1), Token::Punctuation("->"));
                    }
                }
                Token::End => return false,
                _ => {}
            }
            ahead += 1;
            if ahead > 64 {
                return false;
            }
        }
    }

    fn primary(&mut self) -> Result<Expression, Diagnostic> {
        let (line, column) = (self.line(), self.column());

        if self.is_mark("(") {
            if self.looks_like_lambda() {
                return Err(unsupported(line, column, "A lambda"));
            }
            self.take();
            let inner = self.expression()?;
            self.want_mark(")")?;
            return Ok(inner);
        }

        if self.eat_word("this") {
            return Ok(Expression::This);
        }

        if self.eat_word("super") {
            self.want_mark(".")?;
            let name = self.want_name()?;
            let arguments = self.arguments()?;
            return Ok(Expression::Call {
                on: None,
                super_call: true,
                name,
                arguments,
            });
        }

        if self.eat_word("new") {
            let what = self.written_type()?;
            if self.is_mark("[") {
                self.take();
                let length = self.expression()?;
                self.want_mark("]")?;
                return Ok(Expression::NewArray {
                    of: what,
                    length: Box::new(length),
                });
            }
            let arguments = self.arguments()?;
            if self.is_mark("{") {
                return Err(unsupported(line, column, "An anonymous class"));
            }
            return Ok(Expression::New { what, arguments });
        }

        match self.take() {
            Token::Int(value) => Ok(Expression::Int(value)),
            Token::Long(value) => Ok(Expression::Long(value)),
            Token::Float(value) => Ok(Expression::Float(value)),
            Token::Double(value) => Ok(Expression::Double(value)),
            Token::Char(value) => Ok(Expression::Char(value)),
            Token::Str(text) => Ok(Expression::Str(text)),
            Token::True => Ok(Expression::Boolean(true)),
            Token::False => Ok(Expression::Boolean(false)),
            Token::Null => Ok(Expression::Null),
            Token::Identifier(name) => {
                if self.is_mark("(") {
                    let arguments = self.arguments()?;
                    return Ok(Expression::Call {
                        on: None,
                        super_call: false,
                        name,
                        arguments,
                    });
                }
                Ok(Expression::Name(name))
            }
            Token::Punctuation("->") => Err(unsupported(line, column, "A lambda")),
            Token::Punctuation("::") => Err(unsupported(line, column, "A method reference")),
            other => Err(at(
                "EJ109",
                line,
                column,
                format!(
                    "An expression was expected, and {} was found.",
                    other.describe()
                ),
            )),
        }
    }
}

/// Reads one Java file into the shape the rest of this compiler works on.
pub fn parse(source: &str) -> Result<Unit, Diagnostic> {
    let tokens = Lexer::new(source).tokens()?;
    let mut parser = Parser::new(tokens);
    let unit = parser.unit()?;
    if !matches!(parser.here().token, Token::End) {
        return Err(at(
            "EJ110",
            parser.line(),
            parser.column(),
            "A file compiled here holds one class, and this one carries more after it.",
        ));
    }
    Ok(unit)
}

// ------------------------------------------------------------------ types

/// A type once it is known what it is.
///
/// The written form says `String`; this says `java/lang/String`. Getting from
/// one to the other is what resolution does, and everything after resolution
/// works on this.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Type {
    Void,
    Boolean,
    Byte,
    Short,
    Char,
    Int,
    Long,
    Float,
    Double,
    Object(String),
    Array(Box<Type>),
}

impl Type {
    pub fn descriptor(&self) -> String {
        match self {
            Type::Void => "V".to_string(),
            Type::Boolean => "Z".to_string(),
            Type::Byte => "B".to_string(),
            Type::Short => "S".to_string(),
            Type::Char => "C".to_string(),
            Type::Int => "I".to_string(),
            Type::Long => "J".to_string(),
            Type::Float => "F".to_string(),
            Type::Double => "D".to_string(),
            Type::Object(name) => format!("L{name};"),
            Type::Array(of) => format!("[{}", of.descriptor()),
        }
    }

    pub fn readable(&self) -> String {
        match self {
            Type::Object(name) => name.replace('/', "."),
            Type::Array(of) => format!("{}[]", of.readable()),
            other => other.descriptor(),
        }
    }

    /// How many local slots and how much stack a value of this takes. Long and
    /// double take two, everything else takes one, and forgetting that is how
    /// a class file stops verifying.
    pub fn width(&self) -> u16 {
        match self {
            Type::Long | Type::Double => 2,
            Type::Void => 0,
            _ => 1,
        }
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Type::Object(_) | Type::Array(_))
    }

    /// Whether this is one of the types the JVM actually computes on as an
    /// int. Byte, short, char and boolean are all int on the stack.
    pub fn is_int_like(&self) -> bool {
        matches!(
            self,
            Type::Boolean | Type::Byte | Type::Short | Type::Char | Type::Int
        )
    }

    pub fn is_numeric(&self) -> bool {
        self.is_int_like() && !matches!(self, Type::Boolean)
            || matches!(self, Type::Long | Type::Float | Type::Double)
    }

    /// Whether a value of this may be given where that is wanted.
    ///
    /// The primitive half is Java's widening conversion, exactly. The reference
    /// half is not: without every class on the classpath there is no way to
    /// know whether one extends another, so any reference is accepted where a
    /// reference is wanted and the verifier on the device has the last word.
    /// That is written into the contract as a gap rather than hidden.
    pub fn may_be_given_to(&self, wanted: &Type) -> bool {
        if self == wanted {
            return true;
        }
        if self.is_reference() && wanted.is_reference() {
            return true;
        }
        matches!(
            (self, wanted),
            (
                Type::Byte,
                Type::Short | Type::Int | Type::Long | Type::Float | Type::Double
            ) | (
                Type::Short | Type::Char,
                Type::Int | Type::Long | Type::Float | Type::Double
            ) | (Type::Int, Type::Long | Type::Float | Type::Double)
                | (Type::Long, Type::Float | Type::Double)
                | (Type::Float, Type::Double)
        )
    }

    /// The type two operands of an arithmetic operator are both promoted to.
    pub fn promoted_with(&self, other: &Type) -> Option<Type> {
        for wide in [Type::Double, Type::Float, Type::Long] {
            if *self == wide || *other == wide {
                return Some(wide);
            }
        }
        (self.is_numeric() && other.is_numeric()).then_some(Type::Int)
    }
}

/// What a method is, as far as a call site needs to know.
#[derive(Clone, Debug)]
pub struct Signature {
    pub owner: String,
    pub name: String,
    pub parameters: Vec<Type>,
    pub returns: Type,
    pub static_: bool,
    /// Whether the owner is an interface, which decides the invoke opcode.
    pub interface: bool,
}

impl Signature {
    pub fn descriptor(&self) -> String {
        let mut out = String::from("(");
        for parameter in &self.parameters {
            out.push_str(&parameter.descriptor());
        }
        out.push(')');
        out.push_str(&self.returns.descriptor());
        out
    }
}

/// What this compiler knows about the classes a file mentions.
///
/// A Java compiler cannot check a call to `super.onCreate` without knowing what
/// `Activity` is. That knowledge comes from class files handed over as
/// dependencies, read with this project's own class reader. Nothing is guessed:
/// a call to something not on the classpath is refused, by name, rather than
/// emitted and left for the device to reject.
#[derive(Clone, Debug, Default)]
pub struct Classpath {
    known: std::collections::BTreeMap<String, KnownClass>,
}

#[derive(Clone, Debug, Default)]
pub struct KnownClass {
    pub name: String,
    pub superclass: Option<String>,
    pub methods: Vec<Signature>,
    pub fields: Vec<(String, Type, bool)>,
    pub interface: bool,
}

impl Classpath {
    pub fn new() -> Classpath {
        Classpath::default()
    }

    /// Adds everything one class file says about itself.
    ///
    /// The reader hands names back the way a person writes them, with dots.
    /// Everything from here down works in the form a class file holds, with
    /// slashes. Converting once, here, is what stops every lookup below from
    /// having to remember which of the two it is holding -- and forgetting
    /// that is exactly what made the first version of this find nothing.
    pub fn learn(&mut self, class: &crate::jvm::Class) -> Result<(), Diagnostic> {
        let interface = class.access_flags & 0x0200 != 0;
        let internal = class.name.replace('.', "/");
        let mut known = KnownClass {
            name: internal.clone(),
            superclass: class.superclass.as_ref().map(|name| name.replace('.', "/")),
            interface,
            ..KnownClass::default()
        };
        for method in &class.methods {
            let Some((parameters, returns)) = read_descriptor(&method.descriptor) else {
                continue;
            };
            known.methods.push(Signature {
                owner: internal.clone(),
                name: method.name.clone(),
                parameters,
                returns,
                static_: method.access_flags & 0x0008 != 0,
                interface,
            });
        }
        for field in &class.fields {
            let Some(what) = read_type(&field.descriptor, &mut 0) else {
                continue;
            };
            known
                .fields
                .push((field.name.clone(), what, field.access_flags & 0x0008 != 0));
        }
        self.known.insert(internal, known);
        Ok(())
    }

    pub fn add(&mut self, class: KnownClass) {
        self.known.insert(class.name.clone(), class);
    }

    pub fn get(&self, name: &str) -> Option<&KnownClass> {
        self.known.get(name)
    }

    pub fn len(&self) -> usize {
        self.known.len()
    }

    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// A method on a class or anything above it.
    pub fn find_method(&self, owner: &str, name: &str, arity: usize) -> Option<&Signature> {
        let mut at = Some(owner.to_string());
        let mut seen = 0;
        while let Some(current) = at {
            let known = self.known.get(&current)?;
            if let Some(found) = known
                .methods
                .iter()
                .find(|one| one.name == name && one.parameters.len() == arity)
            {
                return Some(found);
            }
            seen += 1;
            if seen > 64 {
                return None;
            }
            at = known.superclass.clone();
        }
        None
    }

    pub fn find_field(
        &self,
        owner: &str,
        name: &str,
    ) -> Option<(&KnownClass, &(String, Type, bool))> {
        let mut at = Some(owner.to_string());
        let mut seen = 0;
        while let Some(current) = at {
            let known = self.known.get(&current)?;
            if let Some(found) = known.fields.iter().find(|(held, _, _)| held == name) {
                return Some((known, found));
            }
            seen += 1;
            if seen > 64 {
                return None;
            }
            at = known.superclass.clone();
        }
        None
    }
}

/// One type out of a descriptor, from `at` onward.
pub fn read_type(descriptor: &str, at: &mut usize) -> Option<Type> {
    let bytes = descriptor.as_bytes();
    let byte = *bytes.get(*at)?;
    *at += 1;
    Some(match byte {
        b'V' => Type::Void,
        b'Z' => Type::Boolean,
        b'B' => Type::Byte,
        b'S' => Type::Short,
        b'C' => Type::Char,
        b'I' => Type::Int,
        b'J' => Type::Long,
        b'F' => Type::Float,
        b'D' => Type::Double,
        b'[' => Type::Array(Box::new(read_type(descriptor, at)?)),
        b'L' => {
            let end = descriptor[*at..].find(';')? + *at;
            let name = descriptor[*at..end].to_string();
            *at = end + 1;
            Type::Object(name)
        }
        _ => return None,
    })
}

/// A whole method descriptor, as its parameters and what it returns.
pub fn read_descriptor(descriptor: &str) -> Option<(Vec<Type>, Type)> {
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut at = 1usize;
    let mut parameters = Vec::new();
    while *bytes.get(at)? != b')' {
        parameters.push(read_type(descriptor, &mut at)?);
    }
    at += 1;
    let returns = read_type(descriptor, &mut at)?;
    Some((parameters, returns))
}

// --------------------------------------------------------- constant pool

/// The pool a class file keeps its names, types and constants in.
///
/// Everything is deduplicated, which is not an optimisation: a pool that holds
/// the same string twice is a pool whose entries mean nothing, and a class file
/// is mostly pool.
#[derive(Clone, Debug, Default)]
pub struct Pool {
    entries: Vec<PoolEntry>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum PoolEntry {
    Utf8(String),
    Integer(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    Class(u16),
    Str(u16),
    NameAndType(u16, u16),
    Field(u16, u16),
    Method(u16, u16),
    InterfaceMethod(u16, u16),
    /// The slot after a long or a double, which the format wastes and every
    /// reader has to know about.
    Unusable,
}

impl Pool {
    pub fn new() -> Pool {
        Pool::default()
    }

    fn put(&mut self, entry: PoolEntry) -> u16 {
        if let Some(found) = self.entries.iter().position(|held| *held == entry) {
            return found as u16 + 1;
        }
        let wide = matches!(entry, PoolEntry::Long(_) | PoolEntry::Double(_));
        self.entries.push(entry);
        let index = self.entries.len() as u16;
        if wide {
            self.entries.push(PoolEntry::Unusable);
        }
        index
    }

    pub fn utf8(&mut self, text: &str) -> u16 {
        self.put(PoolEntry::Utf8(text.to_string()))
    }

    pub fn class(&mut self, internal: &str) -> u16 {
        let name = self.utf8(internal);
        self.put(PoolEntry::Class(name))
    }

    pub fn string(&mut self, text: &str) -> u16 {
        let value = self.utf8(text);
        self.put(PoolEntry::Str(value))
    }

    pub fn integer(&mut self, value: i32) -> u16 {
        self.put(PoolEntry::Integer(value))
    }

    pub fn long(&mut self, value: i64) -> u16 {
        self.put(PoolEntry::Long(value))
    }

    pub fn float(&mut self, value: f32) -> u16 {
        self.put(PoolEntry::Float(value.to_bits()))
    }

    pub fn double(&mut self, value: f64) -> u16 {
        self.put(PoolEntry::Double(value.to_bits()))
    }

    fn name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        self.put(PoolEntry::NameAndType(name, descriptor))
    }

    pub fn field(&mut self, owner: &str, name: &str, descriptor: &str) -> u16 {
        let owner = self.class(owner);
        let what = self.name_and_type(name, descriptor);
        self.put(PoolEntry::Field(owner, what))
    }

    pub fn method(&mut self, owner: &str, name: &str, descriptor: &str, interface: bool) -> u16 {
        let owner_index = self.class(owner);
        let what = self.name_and_type(name, descriptor);
        if interface {
            return self.put(PoolEntry::InterfaceMethod(owner_index, what));
        }
        self.put(PoolEntry::Method(owner_index, what))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.entries.len() as u16 + 1).to_be_bytes());
        for entry in &self.entries {
            match entry {
                PoolEntry::Unusable => {}
                PoolEntry::Utf8(text) => {
                    out.push(1);
                    let bytes = crate::binary::modified_utf8::encode(text);
                    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
                    out.extend_from_slice(&bytes);
                }
                PoolEntry::Integer(value) => {
                    out.push(3);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                PoolEntry::Float(bits) => {
                    out.push(4);
                    out.extend_from_slice(&bits.to_be_bytes());
                }
                PoolEntry::Long(value) => {
                    out.push(5);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                PoolEntry::Double(bits) => {
                    out.push(6);
                    out.extend_from_slice(&bits.to_be_bytes());
                }
                PoolEntry::Class(name) => {
                    out.push(7);
                    out.extend_from_slice(&name.to_be_bytes());
                }
                PoolEntry::Str(value) => {
                    out.push(8);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                PoolEntry::Field(owner, what) => {
                    out.push(9);
                    out.extend_from_slice(&owner.to_be_bytes());
                    out.extend_from_slice(&what.to_be_bytes());
                }
                PoolEntry::Method(owner, what) => {
                    out.push(10);
                    out.extend_from_slice(&owner.to_be_bytes());
                    out.extend_from_slice(&what.to_be_bytes());
                }
                PoolEntry::InterfaceMethod(owner, what) => {
                    out.push(11);
                    out.extend_from_slice(&owner.to_be_bytes());
                    out.extend_from_slice(&what.to_be_bytes());
                }
                PoolEntry::NameAndType(name, descriptor) => {
                    out.push(12);
                    out.extend_from_slice(&name.to_be_bytes());
                    out.extend_from_slice(&descriptor.to_be_bytes());
                }
            }
        }
    }
}

// --------------------------------------------------------------- emitter

/// Where a local lives and what is in it.
#[derive(Clone, Debug)]
struct Local {
    name: String,
    slot: u16,
    what: Type,
}

/// A jump whose destination is not known yet.
struct Pending {
    at: usize,
    from: usize,
}

/// Turns one method body into bytecode.
///
/// There is no separate checking pass. Every expression is typed as it is
/// emitted and the type comes back out, which is enough for this subset and
/// keeps one truth instead of two that can disagree. A type error is therefore
/// found at the moment the code for it would have been written, and nothing is
/// written after one.
struct Emitter<'a> {
    pool: &'a mut Pool,
    classpath: &'a Classpath,
    unit: &'a Unit,
    this_class: String,
    code: Vec<u8>,
    locals: Vec<Local>,
    scopes: Vec<usize>,
    next_slot: u16,
    max_slot: u16,
    depth: i32,
    max_depth: i32,
    breaks: Vec<Vec<Pending>>,
    continues: Vec<Vec<Pending>>,
    static_: bool,
    /// What this method said it returns. A `return` is checked against it, and
    /// the first version of this did not do that -- so `int f() { return
    /// "text"; }` compiled, and produced a class file whose verifier would
    /// have thrown it out on the device.
    returns: Type,
}

impl<'a> Emitter<'a> {
    fn new(
        pool: &'a mut Pool,
        classpath: &'a Classpath,
        unit: &'a Unit,
        this_class: String,
        static_: bool,
    ) -> Emitter<'a> {
        Emitter {
            pool,
            classpath,
            unit,
            this_class,
            code: Vec::new(),
            locals: Vec::new(),
            scopes: Vec::new(),
            next_slot: 0,
            max_slot: 0,
            depth: 0,
            max_depth: 0,
            breaks: Vec::new(),
            continues: Vec::new(),
            static_,
            returns: Type::Void,
        }
    }

    // -- the stack, tracked as it goes, because a class file has to declare
    // -- how deep it gets and getting that wrong is a class that will not load.

    fn grow(&mut self, by: i32) {
        self.depth += by;
        if self.depth > self.max_depth {
            self.max_depth = self.depth;
        }
        if self.depth < 0 {
            self.depth = 0;
        }
    }

    fn op(&mut self, opcode: u8) {
        self.code.push(opcode);
    }

    fn op1(&mut self, opcode: u8, operand: u8) {
        self.code.push(opcode);
        self.code.push(operand);
    }

    fn op2(&mut self, opcode: u8, operand: u16) {
        self.code.push(opcode);
        self.code.extend_from_slice(&operand.to_be_bytes());
    }

    fn jump(&mut self, opcode: u8) -> Pending {
        let from = self.code.len();
        self.code.push(opcode);
        self.code.extend_from_slice(&[0, 0]);
        Pending {
            at: self.code.len() - 2,
            from,
        }
    }

    fn land(&mut self, pending: Pending) {
        let offset = (self.code.len() - pending.from) as i16;
        self.code[pending.at..pending.at + 2].copy_from_slice(&offset.to_be_bytes());
    }

    fn jump_back(&mut self, opcode: u8, to: usize) {
        let from = self.code.len();
        let offset = (to as i64 - from as i64) as i16;
        self.code.push(opcode);
        self.code.extend_from_slice(&offset.to_be_bytes());
    }

    // -- scopes

    fn open(&mut self) {
        self.scopes.push(self.locals.len());
    }

    fn close(&mut self) {
        if let Some(mark) = self.scopes.pop() {
            while self.locals.len() > mark {
                if let Some(gone) = self.locals.pop() {
                    self.next_slot -= gone.what.width();
                }
            }
        }
    }

    fn declare(&mut self, name: &str, what: Type) -> u16 {
        let slot = self.next_slot;
        self.next_slot += what.width();
        if self.next_slot > self.max_slot {
            self.max_slot = self.next_slot;
        }
        self.locals.push(Local {
            name: name.to_string(),
            slot,
            what,
        });
        slot
    }

    fn local(&self, name: &str) -> Option<Local> {
        self.locals
            .iter()
            .rev()
            .find(|held| held.name == name)
            .cloned()
    }

    // -- resolving what was written into what it is

    fn resolve(&self, written: &Written, line: u32) -> Result<Type, Diagnostic> {
        Ok(match written {
            Written::Void => Type::Void,
            Written::Boolean => Type::Boolean,
            Written::Byte => Type::Byte,
            Written::Short => Type::Short,
            Written::Char => Type::Char,
            Written::Int => Type::Int,
            Written::Long => Type::Long,
            Written::Float => Type::Float,
            Written::Double => Type::Double,
            Written::Array(of) => Type::Array(Box::new(self.resolve(of, line)?)),
            Written::Named(name) => Type::Object(self.resolve_class(name, line)?),
        })
    }

    /// A written class name as the internal name it stands for.
    ///
    /// In order: the class being compiled, an import that ends in it, a
    /// wildcard import, `java.lang`, and finally the name as written if it is
    /// already qualified. A name that resolves to nothing is refused here
    /// rather than written into a class file for a device to reject.
    fn resolve_class(&self, name: &str, line: u32) -> Result<String, Diagnostic> {
        if name == self.unit.name {
            return Ok(self.this_class.clone());
        }
        if name.contains('.') {
            let internal = name.replace('.', "/");
            if self.classpath.get(&internal).is_some() || WELL_KNOWN.contains(&internal.as_str()) {
                return Ok(internal);
            }
        }
        for import in &self.unit.imports {
            if let Some(last) = import.rsplit('.').next() {
                if last == name {
                    let internal = import.replace('.', "/");
                    // An import says where a class would live if it exists. It
                    // is not proof that it does, and taking it as proof is how
                    // a call to a class nobody handed over got as far as being
                    // written into a class file.
                    if self.classpath.get(&internal).is_some()
                        || WELL_KNOWN.contains(&internal.as_str())
                    {
                        return Ok(internal);
                    }
                }
            }
        }
        for import in &self.unit.imports {
            if let Some(prefix) = import.strip_suffix(".*") {
                let candidate = format!("{}/{name}", prefix.replace('.', "/"));
                if self.classpath.get(&candidate).is_some() {
                    return Ok(candidate);
                }
            }
        }
        let in_lang = format!("java/lang/{name}");
        if self.classpath.get(&in_lang).is_some() || WELL_KNOWN.contains(&in_lang.as_str()) {
            return Ok(in_lang);
        }
        Err(at(
            "EJ200",
            line,
            1,
            format!("`{name}` is not a type this compilation knows."),
        )
        .with_suggestion(
            "Import it, write it out in full, or hand the class file that declares it \
             over as a dependency. Nothing is guessed: a name that resolves to nothing \
             here would become a class file the device refuses.",
        ))
    }
}

/// The handful of `java.lang` classes that can be named without anything on the
/// classpath, because a compiler that cannot say `String` cannot compile
/// anything at all.
const WELL_KNOWN: &[&str] = &[
    "java/lang/Object",
    "java/lang/String",
    "java/lang/CharSequence",
    "java/lang/StringBuilder",
];

impl Emitter<'_> {
    /// Puts a constant on the stack, using the narrowest instruction that
    /// holds it. `iconst_1` is one byte where `ldc` is two and a pool entry.
    fn push_int(&mut self, value: i64) {
        match value {
            -1..=5 => self.op(0x03u8.wrapping_add((value + 1) as u8)),
            -128..=127 => self.op1(0x10, value as i8 as u8),
            -32768..=32767 => self.op2(0x11, value as i16 as u16),
            _ => {
                let index = self.pool.integer(value as i32);
                if index <= 255 {
                    self.op1(0x12, index as u8);
                } else {
                    self.op2(0x13, index);
                }
            }
        }
        self.grow(1);
    }

    fn push_string(&mut self, text: &str) {
        let index = self.pool.string(text);
        if index <= 255 {
            self.op1(0x12, index as u8);
        } else {
            self.op2(0x13, index);
        }
        self.grow(1);
    }

    fn load(&mut self, slot: u16, what: &Type) {
        let base = match what {
            Type::Long => 0x16u8,
            Type::Float => 0x17,
            Type::Double => 0x18,
            other if other.is_reference() => 0x19,
            _ => 0x15,
        };
        if slot <= 3 {
            // The compact forms: iload_0 is 0x1a, and each type follows four
            // slots later.
            let compact = match base {
                0x15 => 0x1a,
                0x16 => 0x1e,
                0x17 => 0x22,
                0x18 => 0x26,
                _ => 0x2a,
            };
            self.op(compact + slot as u8);
        } else if slot <= 255 {
            self.op1(base, slot as u8);
        } else {
            self.op(0xc4);
            self.op2(base, slot);
        }
        self.grow(i32::from(what.width()));
    }

    fn store(&mut self, slot: u16, what: &Type) {
        let base = match what {
            Type::Long => 0x38u8,
            Type::Float => 0x39,
            Type::Double => 0x3a,
            other if other.is_reference() => 0x3b,
            _ => 0x37,
        };
        if slot <= 3 {
            let compact = match base {
                0x37 => 0x3b,
                0x38 => 0x3f,
                0x39 => 0x43,
                0x3a => 0x47,
                _ => 0x4b,
            };
            self.op(compact + slot as u8);
        } else if slot <= 255 {
            self.op1(base, slot as u8);
        } else {
            self.op(0xc4);
            self.op2(base, slot);
        }
        self.grow(-i32::from(what.width()));
    }

    /// Widens what is on the stack, when Java says it happens on its own.
    fn convert(&mut self, from: &Type, to: &Type, line: u32) -> Result<(), Diagnostic> {
        if from == to || (from.is_int_like() && to.is_int_like()) {
            return Ok(());
        }
        let opcode = match (from, to) {
            (f, Type::Long) if f.is_int_like() => 0x85u8,
            (f, Type::Float) if f.is_int_like() => 0x86,
            (f, Type::Double) if f.is_int_like() => 0x87,
            (Type::Long, Type::Int) => 0x88,
            (Type::Long, Type::Float) => 0x89,
            (Type::Long, Type::Double) => 0x8a,
            (Type::Float, Type::Int) => 0x8b,
            (Type::Float, Type::Long) => 0x8c,
            (Type::Float, Type::Double) => 0x8d,
            (Type::Double, Type::Int) => 0x8e,
            (Type::Double, Type::Long) => 0x8f,
            (Type::Double, Type::Float) => 0x90,
            (Type::Int, Type::Byte) => 0x91,
            (Type::Int, Type::Char) => 0x92,
            (Type::Int, Type::Short) => 0x93,
            _ if from.is_reference() && to.is_reference() => return Ok(()),
            _ => {
                return Err(at(
                    "EJ201",
                    line,
                    1,
                    format!("A {} does not become a {}.", from.readable(), to.readable()),
                ))
            }
        };
        self.op(opcode);
        self.grow(i32::from(to.width()) - i32::from(from.width()));
        Ok(())
    }

    /// Emits an expression and says what type it left on the stack.
    fn value(&mut self, expression: &Expression, line: u32) -> Result<Type, Diagnostic> {
        match expression {
            Expression::Int(value) => {
                self.push_int(*value);
                Ok(Type::Int)
            }
            Expression::Char(value) => {
                self.push_int(i64::from(*value));
                Ok(Type::Char)
            }
            Expression::Boolean(value) => {
                self.push_int(i64::from(*value));
                Ok(Type::Boolean)
            }
            Expression::Long(value) => {
                if *value == 0 || *value == 1 {
                    self.op(0x09 + *value as u8);
                } else {
                    let index = self.pool.long(*value);
                    self.op2(0x14, index);
                }
                self.grow(2);
                Ok(Type::Long)
            }
            Expression::Float(value) => {
                let index = self.pool.float(*value as f32);
                if index <= 255 {
                    self.op1(0x12, index as u8);
                } else {
                    self.op2(0x13, index);
                }
                self.grow(1);
                Ok(Type::Float)
            }
            Expression::Double(value) => {
                let index = self.pool.double(*value);
                self.op2(0x14, index);
                self.grow(2);
                Ok(Type::Double)
            }
            Expression::Str(text) => {
                self.push_string(text);
                Ok(Type::Object("java/lang/String".to_string()))
            }
            Expression::Null => {
                self.op(0x01);
                self.grow(1);
                Ok(Type::Object("java/lang/Object".to_string()))
            }
            Expression::This => {
                if self.static_ {
                    return Err(at(
                        "EJ202",
                        line,
                        1,
                        "`this` has no meaning in a static method.",
                    ));
                }
                self.load(0, &Type::Object(self.this_class.clone()));
                Ok(Type::Object(self.this_class.clone()))
            }
            Expression::Name(name) => self.name_value(name, line),
            Expression::Field { of, name } => {
                let owner = self.value(of, line)?;
                self.read_field(&owner, name, line)
            }
            Expression::Index { of, at: index } => {
                let array = self.value(of, line)?;
                let Type::Array(element) = array.clone() else {
                    return Err(at(
                        "EJ203",
                        line,
                        1,
                        format!("A {} cannot be indexed.", array.readable()),
                    ));
                };
                let found = self.value(index, line)?;
                self.convert(&found, &Type::Int, line)?;
                let opcode = match *element {
                    Type::Long => 0x2fu8,
                    Type::Float => 0x30,
                    Type::Double => 0x31,
                    Type::Byte | Type::Boolean => 0x33,
                    Type::Char => 0x34,
                    Type::Short => 0x35,
                    ref other if other.is_reference() => 0x32,
                    _ => 0x2e,
                };
                self.op(opcode);
                self.grow(i32::from(element.width()) - 2);
                Ok(*element)
            }
            Expression::Cast { to, of } => {
                let target = self.resolve(to, line)?;
                let found = self.value(of, line)?;
                if target.is_reference() && found.is_reference() {
                    let index = self.pool.class(&match &target {
                        Type::Object(name) => name.clone(),
                        other => other.descriptor(),
                    });
                    self.op2(0xc0, index);
                    return Ok(target);
                }
                // A narrowing cast is written out, unlike a widening one which
                // Java does on its own.
                let opcode = match (&found, &target) {
                    (f, t) if f == t => None,
                    (f, Type::Byte) if f.is_int_like() => Some(0x91u8),
                    (f, Type::Char) if f.is_int_like() => Some(0x92),
                    (f, Type::Short) if f.is_int_like() => Some(0x93),
                    _ => {
                        self.convert(&found, &target, line)?;
                        None
                    }
                };
                if let Some(opcode) = opcode {
                    self.op(opcode);
                }
                Ok(target)
            }
            Expression::Unary { operator, of } => {
                let found = self.value(of, line)?;
                match operator {
                    Unary::Plus => Ok(found),
                    Unary::Negate => {
                        let opcode = match found {
                            Type::Long => 0x75u8,
                            Type::Float => 0x76,
                            Type::Double => 0x77,
                            ref other if other.is_int_like() => 0x74,
                            _ => {
                                return Err(at(
                                    "EJ204",
                                    line,
                                    1,
                                    format!("A {} cannot be negated.", found.readable()),
                                ))
                            }
                        };
                        self.op(opcode);
                        Ok(if found.is_int_like() {
                            Type::Int
                        } else {
                            found
                        })
                    }
                    Unary::Complement => {
                        // There is no `not` instruction: it is xor with all
                        // ones, which is what the language means anyway.
                        if found == Type::Long {
                            let index = self.pool.long(-1);
                            self.op2(0x14, index);
                            self.grow(2);
                            self.op(0x83);
                            self.grow(-2);
                            return Ok(Type::Long);
                        }
                        if !found.is_int_like() {
                            return Err(at(
                                "EJ204",
                                line,
                                1,
                                format!("A {} has no bits to flip.", found.readable()),
                            ));
                        }
                        self.push_int(-1);
                        self.op(0x82);
                        self.grow(-1);
                        Ok(Type::Int)
                    }
                    Unary::Not => {
                        if found != Type::Boolean {
                            return Err(at(
                                "EJ205",
                                line,
                                1,
                                format!(
                                    "`!` wants a boolean and was given a {}.",
                                    found.readable()
                                ),
                            ));
                        }
                        // Turned into a branch, because a boolean on the stack
                        // is an int and there is no instruction that flips one.
                        let jump = self.jump(0x99);
                        self.grow(-1);
                        self.push_int(0);
                        let over = self.jump(0xa7);
                        self.land(jump);
                        self.grow(-1);
                        self.push_int(1);
                        self.land(over);
                        Ok(Type::Boolean)
                    }
                }
            }
            Expression::Binary {
                operator,
                left,
                right,
            } => self.binary(*operator, left, right, line),
            Expression::Conditional {
                condition,
                then,
                otherwise,
            } => {
                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at(
                        "EJ206",
                        line,
                        1,
                        format!(
                            "A condition is a boolean, and this is a {}.",
                            found.readable()
                        ),
                    ));
                }
                let to_else = self.jump(0x99);
                self.grow(-1);
                let depth_before = self.depth;
                let taken = self.value(then, line)?;
                let over = self.jump(0xa7);
                self.land(to_else);
                self.depth = depth_before;
                let other = self.value(otherwise, line)?;
                self.land(over);
                if taken != other && !taken.is_reference() {
                    let both = taken.promoted_with(&other).ok_or_else(|| {
                        at(
                            "EJ207",
                            line,
                            1,
                            format!(
                                "The two sides of `?:` are a {} and a {}.",
                                taken.readable(),
                                other.readable()
                            ),
                        )
                    })?;
                    return Ok(both);
                }
                Ok(taken)
            }
            Expression::InstanceOf { of, what } => {
                let found = self.value(of, line)?;
                if !found.is_reference() {
                    return Err(at(
                        "EJ208",
                        line,
                        1,
                        format!(
                            "`instanceof` wants an object and was given a {}.",
                            found.readable()
                        ),
                    ));
                }
                let target = self.resolve(what, line)?;
                let index = self.pool.class(&match &target {
                    Type::Object(name) => name.clone(),
                    other => other.descriptor(),
                });
                self.op2(0xc1, index);
                Ok(Type::Boolean)
            }
            Expression::NewArray { of, length } => {
                let element = self.resolve(of, line)?;
                let found = self.value(length, line)?;
                self.convert(&found, &Type::Int, line)?;
                match &element {
                    Type::Object(name) => {
                        let index = self.pool.class(name);
                        self.op2(0xbd, index);
                    }
                    Type::Array(_) => {
                        let index = self.pool.class(&element.descriptor());
                        self.op2(0xbd, index);
                    }
                    primitive => {
                        let code = match primitive {
                            Type::Boolean => 4u8,
                            Type::Char => 5,
                            Type::Float => 6,
                            Type::Double => 7,
                            Type::Byte => 8,
                            Type::Short => 9,
                            Type::Int => 10,
                            Type::Long => 11,
                            _ => {
                                return Err(at(
                                    "EJ209",
                                    line,
                                    1,
                                    "An array of void is not a thing.",
                                ))
                            }
                        };
                        self.op1(0xbc, code);
                    }
                }
                Ok(Type::Array(Box::new(element)))
            }
            Expression::New { what, arguments } => self.new_object(what, arguments, line),
            Expression::Call {
                on,
                super_call,
                name,
                arguments,
            } => self.call(on.as_deref(), *super_call, name, arguments, line),
            Expression::Assign {
                target,
                operator,
                value,
            } => self.assign(target, *operator, value, line, true),
            Expression::Step { target, by, after } => self.step(target, *by, *after, line, true),
        }
    }
}

impl Emitter<'_> {
    /// A bare name: a local first, then a field of the class being compiled,
    /// then a field of anything above it.
    fn name_value(&mut self, name: &str, line: u32) -> Result<Type, Diagnostic> {
        if let Some(local) = self.local(name) {
            self.load(local.slot, &local.what);
            return Ok(local.what);
        }
        if let Some(field) = self
            .unit
            .fields
            .iter()
            .find(|held| held.name == name)
            .cloned()
        {
            let what = self.resolve(&field.what, line)?;
            let descriptor = what.descriptor();
            let owner = self.this_class.clone();
            let index = self.pool.field(&owner, name, &descriptor);
            if field.modifiers.static_ {
                self.op2(0xb2, index);
                self.grow(i32::from(what.width()));
            } else {
                if self.static_ {
                    return Err(at(
                        "EJ210",
                        line,
                        1,
                        format!("`{name}` belongs to an instance and this method is static."),
                    ));
                }
                self.load(0, &Type::Object(owner));
                self.op2(0xb4, index);
                self.grow(i32::from(what.width()) - 1);
            }
            return Ok(what);
        }
        let inherited = self
            .unit
            .extends
            .clone()
            .map(|name| self.resolve_class(&name, line))
            .transpose()?;
        if let Some(owner) = inherited {
            if let Some((holder, (_, what, static_))) = self.classpath.find_field(&owner, name) {
                let (holder, what, static_) = (holder.name.clone(), what.clone(), *static_);
                let descriptor = what.descriptor();
                let index = self.pool.field(&holder, name, &descriptor);
                if static_ {
                    self.op2(0xb2, index);
                    self.grow(i32::from(what.width()));
                } else {
                    self.load(0, &Type::Object(self.this_class.clone()));
                    self.op2(0xb4, index);
                    self.grow(i32::from(what.width()) - 1);
                }
                return Ok(what);
            }
        }
        Err(at(
            "EJ211",
            line,
            1,
            format!("`{name}` is not anything this method can see."),
        )
        .with_suggestion(
            "Nothing here guesses at a name. A local, a parameter, a field of this \
                 class or of one above it: anything else is refused rather than written \
                 into a class file.",
        ))
    }

    fn read_field(&mut self, owner: &Type, name: &str, line: u32) -> Result<Type, Diagnostic> {
        if let Type::Array(_) = owner {
            if name == "length" {
                self.op(0xbe);
                return Ok(Type::Int);
            }
        }
        let Type::Object(class) = owner else {
            return Err(at(
                "EJ212",
                line,
                1,
                format!("A {} has no fields.", owner.readable()),
            ));
        };
        let Some((holder, (_, what, static_))) = self.classpath.find_field(class, name) else {
            return Err(at(
                "EJ213",
                line,
                1,
                format!(
                    "`{}` has no field called `{name}` that this compilation knows.",
                    class.replace('/', ".")
                ),
            )
            .with_suggestion(
                "Hand the class file that declares it over as a dependency. Nothing is \
                 emitted for a field nobody has seen.",
            ));
        };
        let (holder, what, static_) = (holder.name.clone(), what.clone(), *static_);
        let descriptor = what.descriptor();
        let index = self.pool.field(&holder, name, &descriptor);
        if static_ {
            // The object it was read off is not wanted after all.
            self.op(0x57);
            self.grow(-1);
            self.op2(0xb2, index);
            self.grow(i32::from(what.width()));
        } else {
            self.op2(0xb4, index);
            self.grow(i32::from(what.width()) - 1);
        }
        Ok(what)
    }

    fn binary(
        &mut self,
        operator: Binary,
        left: &Expression,
        right: &Expression,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        // `&&` and `||` do not evaluate their right side unless they have to,
        // which makes them control flow rather than arithmetic.
        if matches!(operator, Binary::AndAlso | Binary::OrElse) {
            let first = self.value(left, line)?;
            if first != Type::Boolean {
                return Err(at("EJ214", line, 1, "`&&` and `||` want booleans."));
            }
            let shortcut = self.jump(if operator == Binary::AndAlso {
                0x99
            } else {
                0x9a
            });
            self.grow(-1);
            let second = self.value(right, line)?;
            if second != Type::Boolean {
                return Err(at("EJ214", line, 1, "`&&` and `||` want booleans."));
            }
            let over = self.jump(0xa7);
            self.land(shortcut);
            self.grow(-1);
            self.push_int(i64::from(operator == Binary::OrElse));
            self.land(over);
            return Ok(Type::Boolean);
        }

        // String concatenation is not arithmetic either: it is a StringBuilder,
        // which is what every Java compiler did before invokedynamic and what
        // this target still wants.
        if operator == Binary::Add {
            let is_string = |what: &Type| *what == Type::Object("java/lang/String".to_string());
            let peeked_left = self.peek_type(left, line)?;
            let peeked_right = self.peek_type(right, line)?;
            if is_string(&peeked_left) || is_string(&peeked_right) {
                return self.concatenate(left, right, line);
            }
        }

        let left_type = self.value(left, line)?;

        if matches!(
            operator,
            Binary::ShiftLeft | Binary::ShiftRight | Binary::UnsignedShiftRight
        ) {
            let right_type = self.value(right, line)?;
            self.convert(&right_type, &Type::Int, line)?;
            let long = left_type == Type::Long;
            let opcode = match (operator, long) {
                (Binary::ShiftLeft, false) => 0x78u8,
                (Binary::ShiftLeft, true) => 0x79,
                (Binary::ShiftRight, false) => 0x7a,
                (Binary::ShiftRight, true) => 0x7b,
                (Binary::UnsignedShiftRight, false) => 0x7c,
                _ => 0x7d,
            };
            self.op(opcode);
            self.grow(-1);
            return Ok(if long { Type::Long } else { Type::Int });
        }

        // Both sides are promoted to one type before the operator sees them.
        let right_peeked = self.peek_type(right, line)?;
        let common = if left_type.is_reference() || right_peeked.is_reference() {
            left_type.clone()
        } else {
            left_type.promoted_with(&right_peeked).ok_or_else(|| {
                at(
                    "EJ215",
                    line,
                    1,
                    format!(
                        "A {} and a {} have no operator between them.",
                        left_type.readable(),
                        right_peeked.readable()
                    ),
                )
            })?
        };
        if !left_type.is_reference() {
            self.convert(&left_type, &common, line)?;
        }
        let right_type = self.value(right, line)?;
        if !right_type.is_reference() {
            self.convert(&right_type, &common, line)?;
        }

        match operator {
            Binary::Add
            | Binary::Subtract
            | Binary::Multiply
            | Binary::Divide
            | Binary::Remainder => {
                let base = match operator {
                    Binary::Add => 0x60u8,
                    Binary::Subtract => 0x64,
                    Binary::Multiply => 0x68,
                    Binary::Divide => 0x6c,
                    _ => 0x70,
                };
                let step = match common {
                    Type::Long => 1u8,
                    Type::Float => 2,
                    Type::Double => 3,
                    _ => 0,
                };
                self.op(base + step);
                self.grow(-i32::from(common.width()));
                Ok(if common.is_int_like() {
                    Type::Int
                } else {
                    common
                })
            }
            Binary::And | Binary::Or | Binary::Xor => {
                if common == Type::Boolean || common.is_int_like() {
                    let base = match operator {
                        Binary::And => 0x7eu8,
                        Binary::Or => 0x80,
                        _ => 0x82,
                    };
                    self.op(base);
                    self.grow(-1);
                    return Ok(if left_type == Type::Boolean {
                        Type::Boolean
                    } else {
                        Type::Int
                    });
                }
                if common == Type::Long {
                    let base = match operator {
                        Binary::And => 0x7fu8,
                        Binary::Or => 0x81,
                        _ => 0x83,
                    };
                    self.op(base);
                    self.grow(-2);
                    return Ok(Type::Long);
                }
                Err(at(
                    "EJ216",
                    line,
                    1,
                    "That operator wants integers or booleans.",
                ))
            }
            _ => self.compare(operator, &common, line),
        }
    }

    /// Turns a comparison into the branch it is. There is no instruction that
    /// leaves a boolean; there are instructions that jump.
    fn compare(&mut self, operator: Binary, common: &Type, line: u32) -> Result<Type, Diagnostic> {
        let reference = common.is_reference();
        let opcode = match (common, operator) {
            (Type::Long, _) => {
                self.op(0x94);
                self.grow(-3);
                None
            }
            (Type::Float, _) => {
                self.op(0x95);
                self.grow(-1);
                None
            }
            (Type::Double, _) => {
                self.op(0x97);
                self.grow(-3);
                None
            }
            _ if reference => match operator {
                Binary::Equal => Some(0xa5u8),
                Binary::NotEqual => Some(0xa6),
                _ => {
                    return Err(at(
                        "EJ217",
                        line,
                        1,
                        "Objects can only be compared with `==` and `!=` here.",
                    ))
                }
            },
            _ => Some(match operator {
                Binary::Equal => 0x9fu8,
                Binary::NotEqual => 0xa0,
                Binary::Less => 0xa1,
                Binary::GreaterOrEqual => 0xa2,
                Binary::Greater => 0xa3,
                _ => 0xa4,
            }),
        };

        let jump = match opcode {
            Some(code) => {
                self.grow(-2);
                self.jump(code)
            }
            None => {
                // The compare left an int; branch on it against zero.
                let code = match operator {
                    Binary::Equal => 0x99u8,
                    Binary::NotEqual => 0x9a,
                    Binary::Less => 0x9b,
                    Binary::GreaterOrEqual => 0x9c,
                    Binary::Greater => 0x9d,
                    _ => 0x9e,
                };
                self.grow(-1);
                self.jump(code)
            }
        };
        self.push_int(0);
        let over = self.jump(0xa7);
        self.land(jump);
        self.grow(-1);
        self.push_int(1);
        self.land(over);
        Ok(Type::Boolean)
    }

    /// What an expression would leave, worked out without emitting anything.
    ///
    /// Needed in two places: deciding whether `+` is arithmetic or
    /// concatenation, and promoting both sides of an operator before either is
    /// on the stack. It runs the emitter on a copy and throws the code away,
    /// which is slower than a separate typing pass and cannot disagree with
    /// one.
    fn peek_type(&mut self, expression: &Expression, line: u32) -> Result<Type, Diagnostic> {
        let code = self.code.len();
        let depth = self.depth;
        let max_depth = self.max_depth;
        let locals = self.locals.len();
        let next_slot = self.next_slot;
        let found = self.value(expression, line);
        self.code.truncate(code);
        self.depth = depth;
        self.max_depth = max_depth;
        self.locals.truncate(locals);
        self.next_slot = next_slot;
        found
    }

    fn concatenate(
        &mut self,
        left: &Expression,
        right: &Expression,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let builder = "java/lang/StringBuilder";
        let index = self.pool.class(builder);
        self.op2(0xbb, index);
        self.grow(1);
        self.op(0x59);
        self.grow(1);
        let init = self.pool.method(builder, "<init>", "()V", false);
        self.op2(0xb7, init);
        self.grow(-1);

        for side in [left, right] {
            let what = self.value(side, line)?;
            let taken = match &what {
                Type::Object(name) if name == "java/lang/String" => {
                    "Ljava/lang/String;".to_string()
                }
                other if other.is_reference() => "Ljava/lang/Object;".to_string(),
                Type::Boolean => "Z".to_string(),
                Type::Char => "C".to_string(),
                Type::Long => "J".to_string(),
                Type::Float => "F".to_string(),
                Type::Double => "D".to_string(),
                _ => "I".to_string(),
            };
            let descriptor = format!("({taken})Ljava/lang/StringBuilder;");
            let append = self.pool.method(builder, "append", &descriptor, false);
            self.op2(0xb6, append);
            self.grow(-i32::from(what.width()));
        }

        let finish = self
            .pool
            .method(builder, "toString", "()Ljava/lang/String;", false);
        self.op2(0xb6, finish);
        Ok(Type::Object("java/lang/String".to_string()))
    }
}

impl Emitter<'_> {
    fn new_object(
        &mut self,
        what: &Written,
        arguments: &[Expression],
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let target = self.resolve(what, line)?;
        let Type::Object(class) = target.clone() else {
            return Err(at(
                "EJ218",
                line,
                1,
                format!("`new` wants a class and was given a {}.", target.readable()),
            ));
        };
        let index = self.pool.class(&class);
        self.op2(0xbb, index);
        self.grow(1);
        self.op(0x59);
        self.grow(1);

        let signature = self
            .classpath
            .find_method(&class, "<init>", arguments.len())
            .cloned();
        let descriptor = match signature {
            Some(found) => {
                self.arguments_for(&found.parameters, arguments, line)?;
                found.descriptor()
            }
            None if arguments.is_empty() && WELL_KNOWN.contains(&class.as_str()) => {
                "()V".to_string()
            }
            None => {
                return Err(at(
                    "EJ219",
                    line,
                    1,
                    format!(
                    "`{}` has no constructor taking {} argument(s) that this compilation knows.",
                    class.replace('/', "."),
                    arguments.len()
                ),
                )
                .with_suggestion("Hand the class file that declares it over as a dependency."))
            }
        };
        let init = self.pool.method(&class, "<init>", &descriptor, false);
        self.op2(0xb7, init);
        let taken: i32 = read_descriptor(&descriptor)
            .map(|(parameters, _)| parameters.iter().map(|p| i32::from(p.width())).sum())
            .unwrap_or(0);
        self.grow(-(taken + 1));
        Ok(target)
    }

    /// Puts arguments on the stack, each converted to what the method wants.
    fn arguments_for(
        &mut self,
        wanted: &[Type],
        given: &[Expression],
        line: u32,
    ) -> Result<(), Diagnostic> {
        if wanted.len() != given.len() {
            return Err(at(
                "EJ220",
                line,
                1,
                format!(
                    "That takes {} argument(s) and was given {}.",
                    wanted.len(),
                    given.len()
                ),
            ));
        }
        for (want, expression) in wanted.iter().zip(given.iter()) {
            let found = self.value(expression, line)?;
            if !found.may_be_given_to(want) {
                return Err(at(
                    "EJ221",
                    line,
                    1,
                    format!(
                        "A {} cannot be given where a {} is wanted.",
                        found.readable(),
                        want.readable()
                    ),
                ));
            }
            if !found.is_reference() {
                self.convert(&found, want, line)?;
            }
        }
        Ok(())
    }

    fn call(
        &mut self,
        on: Option<&Expression>,
        super_call: bool,
        name: &str,
        arguments: &[Expression],
        line: u32,
    ) -> Result<Type, Diagnostic> {
        // `super.name(...)`: an exact call on the superclass, not a virtual one.
        if super_call {
            if self.static_ {
                return Err(at(
                    "EJ222",
                    line,
                    1,
                    "`super` has no meaning in a static method.",
                ));
            }
            let Some(parent) = self.unit.extends.clone() else {
                return Err(at(
                    "EJ223",
                    line,
                    1,
                    "This class extends nothing to call up into.",
                ));
            };
            let owner = self.resolve_class(&parent, line)?;
            let Some(signature) = self
                .classpath
                .find_method(&owner, name, arguments.len())
                .cloned()
            else {
                return Err(self.no_such_method(&owner, name, arguments.len(), line));
            };
            self.load(0, &Type::Object(self.this_class.clone()));
            self.arguments_for(&signature.parameters, arguments, line)?;
            let descriptor = signature.descriptor();
            let index = self.pool.method(&signature.owner, name, &descriptor, false);
            self.op2(0xb7, index);
            let taken: i32 = signature
                .parameters
                .iter()
                .map(|p| i32::from(p.width()))
                .sum();
            self.grow(-(taken + 1) + i32::from(signature.returns.width()));
            return Ok(signature.returns);
        }

        // A bare `name(...)`: a method of the class being compiled.
        let Some(on) = on else {
            let own = self
                .unit
                .methods
                .iter()
                .find(|held| held.name == name && held.parameters.len() == arguments.len())
                .cloned();
            let Some(own) = own else {
                return Err(at(
                    "EJ224",
                    line,
                    1,
                    format!(
                        "`{name}` is not a method of this class taking {} argument(s).",
                        arguments.len()
                    ),
                ));
            };
            let mut parameters = Vec::new();
            for (what, _) in &own.parameters {
                parameters.push(self.resolve(what, line)?);
            }
            let returns = self.resolve(&own.returns, line)?;
            let descriptor = Signature {
                owner: self.this_class.clone(),
                name: name.to_string(),
                parameters: parameters.clone(),
                returns: returns.clone(),
                static_: own.modifiers.static_,
                interface: false,
            }
            .descriptor();

            if !own.modifiers.static_ {
                if self.static_ {
                    return Err(at(
                        "EJ225",
                        line,
                        1,
                        format!("`{name}` belongs to an instance and this method is static."),
                    ));
                }
                self.load(0, &Type::Object(self.this_class.clone()));
            }
            self.arguments_for(&parameters, arguments, line)?;
            let owner = self.this_class.clone();
            let index = self.pool.method(&owner, name, &descriptor, false);
            self.op2(if own.modifiers.static_ { 0xb8 } else { 0xb6 }, index);
            let taken: i32 = parameters.iter().map(|p| i32::from(p.width())).sum();
            let popped = taken + i32::from(!own.modifiers.static_);
            self.grow(-popped + i32::from(returns.width()));
            return Ok(returns);
        };

        // `something.name(...)`. If `something` is a bare name that is a class
        // rather than a value, this is a static call.
        if let Expression::Name(maybe_class) = on {
            let inherited = self
                .unit
                .extends
                .clone()
                .and_then(|parent| self.resolve_class(&parent, line).ok())
                .and_then(|owner| {
                    self.classpath
                        .find_field(&owner, maybe_class)
                        .map(|(holder, _)| holder.name.clone())
                });
            if self.local(maybe_class).is_none()
                && inherited.is_none()
                && !self
                    .unit
                    .fields
                    .iter()
                    .any(|held| held.name == *maybe_class)
            {
                // It is not a value, so it is meant to be a class, and saying
                // what is wrong with it as a class is the useful thing. Falling
                // through to read it as a value would report that a name is not
                // visible, which is true and tells nobody anything.
                let owner = self.resolve_class(maybe_class, line)?;
                let Some(signature) = self
                    .classpath
                    .find_method(&owner, name, arguments.len())
                    .cloned()
                else {
                    return Err(self.no_such_method(&owner, name, arguments.len(), line));
                };
                if !signature.static_ {
                    return Err(at(
                        "EJ234",
                        line,
                        1,
                        format!(
                            "`{name}` belongs to an instance of `{}`, and this names the class.",
                            owner.replace('/', ".")
                        ),
                    ));
                }
                self.arguments_for(&signature.parameters, arguments, line)?;
                let descriptor = signature.descriptor();
                let index = self.pool.method(&signature.owner, name, &descriptor, false);
                self.op2(0xb8, index);
                let taken: i32 = signature
                    .parameters
                    .iter()
                    .map(|p| i32::from(p.width()))
                    .sum();
                self.grow(-taken + i32::from(signature.returns.width()));
                return Ok(signature.returns);
            }
        }

        let owner_type = self.value(on, line)?;
        let Type::Object(owner) = owner_type.clone() else {
            return Err(at(
                "EJ226",
                line,
                1,
                format!("A {} has no methods to call.", owner_type.readable()),
            ));
        };
        let Some(signature) = self
            .classpath
            .find_method(&owner, name, arguments.len())
            .cloned()
        else {
            return Err(self.no_such_method(&owner, name, arguments.len(), line));
        };
        self.arguments_for(&signature.parameters, arguments, line)?;
        let descriptor = signature.descriptor();
        let index = self
            .pool
            .method(&signature.owner, name, &descriptor, signature.interface);
        if signature.interface {
            let taken: i32 = signature
                .parameters
                .iter()
                .map(|p| i32::from(p.width()))
                .sum();
            self.code.push(0xb9);
            self.code.extend_from_slice(&index.to_be_bytes());
            self.code.push((taken + 1) as u8);
            self.code.push(0);
            self.grow(-(taken + 1) + i32::from(signature.returns.width()));
        } else {
            self.op2(0xb6, index);
            let taken: i32 = signature
                .parameters
                .iter()
                .map(|p| i32::from(p.width()))
                .sum();
            self.grow(-(taken + 1) + i32::from(signature.returns.width()));
        }
        Ok(signature.returns)
    }

    fn no_such_method(&self, owner: &str, name: &str, arity: usize, line: u32) -> Diagnostic {
        at(
            "EJ227",
            line,
            1,
            format!(
                "`{}` has no method `{name}` taking {arity} argument(s) that this compilation knows.",
                owner.replace('/', ".")
            ),
        )
        .with_suggestion(
            "Hand the class file that declares it over as a dependency. Nothing is guessed: \
             a call written against a method nobody has seen would be a class file the \
             device refuses.",
        )
    }

    /// An assignment. `wanted` says whether the value has to be left behind,
    /// which is the difference between `a = b;` and `c = (a = b);`.
    fn assign(
        &mut self,
        target: &Expression,
        operator: Option<Binary>,
        value: &Expression,
        line: u32,
        wanted: bool,
    ) -> Result<Type, Diagnostic> {
        match target {
            Expression::Name(name) if self.local(name).is_some() => {
                let local = self.local(name).unwrap();
                let found = match operator {
                    None => self.value(value, line)?,
                    Some(operator) => {
                        self.binary(operator, &Expression::Name(name.clone()), value, line)?
                    }
                };
                if !found.may_be_given_to(&local.what) {
                    return Err(at(
                        "EJ228",
                        line,
                        1,
                        format!(
                            "A {} cannot be put in `{name}`, which is a {}.",
                            found.readable(),
                            local.what.readable()
                        ),
                    ));
                }
                if !found.is_reference() {
                    self.convert(&found, &local.what, line)?;
                }
                if wanted {
                    self.op(if local.what.width() == 2 { 0x5c } else { 0x59 });
                    self.grow(i32::from(local.what.width()));
                }
                self.store(local.slot, &local.what);
                Ok(local.what)
            }
            Expression::Name(name) => {
                let field = self
                    .unit
                    .fields
                    .iter()
                    .find(|held| held.name == *name)
                    .cloned()
                    .ok_or_else(|| {
                        at(
                            "EJ229",
                            line,
                            1,
                            format!("`{name}` is nothing that can be assigned to."),
                        )
                    })?;
                let what = self.resolve(&field.what, line)?;
                let owner = self.this_class.clone();
                if !field.modifiers.static_ {
                    if self.static_ {
                        return Err(at(
                            "EJ210",
                            line,
                            1,
                            format!("`{name}` belongs to an instance and this method is static."),
                        ));
                    }
                    self.load(0, &Type::Object(owner.clone()));
                }
                let found = match operator {
                    None => self.value(value, line)?,
                    Some(operator) => {
                        self.binary(operator, &Expression::Name(name.clone()), value, line)?
                    }
                };
                if !found.is_reference() {
                    self.convert(&found, &what, line)?;
                }
                if wanted {
                    self.op(if what.width() == 2 { 0x5c } else { 0x59 });
                    self.grow(i32::from(what.width()));
                }
                let descriptor = what.descriptor();
                let index = self.pool.field(&owner, name, &descriptor);
                if field.modifiers.static_ {
                    self.op2(0xb3, index);
                    self.grow(-i32::from(what.width()));
                } else {
                    self.op2(0xb5, index);
                    self.grow(-i32::from(what.width()) - 1);
                }
                Ok(what)
            }
            Expression::Index { of, at: index } => {
                let array = self.value(of, line)?;
                let Type::Array(element) = array.clone() else {
                    return Err(at("EJ203", line, 1, "That is not an array."));
                };
                let found = self.value(index, line)?;
                self.convert(&found, &Type::Int, line)?;
                if operator.is_some() {
                    return Err(unsupported(line, 1, "A compound assignment into an array"));
                }
                let given = self.value(value, line)?;
                if !given.is_reference() {
                    self.convert(&given, &element, line)?;
                }
                if wanted {
                    return Err(unsupported(
                        line,
                        1,
                        "Using the value of an array assignment",
                    ));
                }
                let opcode = match *element {
                    Type::Long => 0x50u8,
                    Type::Float => 0x51,
                    Type::Double => 0x52,
                    Type::Byte | Type::Boolean => 0x54,
                    Type::Char => 0x55,
                    Type::Short => 0x56,
                    ref other if other.is_reference() => 0x53,
                    _ => 0x4f,
                };
                self.op(opcode);
                self.grow(-2 - i32::from(element.width()));
                Ok(*element)
            }
            _ => Err(unsupported(line, 1, "Assigning to that")),
        }
    }

    fn step(
        &mut self,
        target: &Expression,
        by: i32,
        after: bool,
        line: u32,
        wanted: bool,
    ) -> Result<Type, Diagnostic> {
        let Expression::Name(name) = target else {
            return Err(unsupported(line, 1, "Stepping anything but a name"));
        };
        if let Some(local) = self.local(name) {
            if local.what.is_int_like() {
                if wanted && after {
                    self.load(local.slot, &local.what);
                }
                // `iinc` steps a local in place, which is the whole reason
                // `for (int i = 0; i < n; i++)` costs three bytes.
                self.code.push(0x84);
                self.code.push(local.slot as u8);
                self.code.push(by as i8 as u8);
                if wanted && !after {
                    self.load(local.slot, &local.what);
                }
                return Ok(local.what);
            }
        }
        // Anything else becomes the assignment it means.
        let value = Expression::Assign {
            target: Box::new(target.clone()),
            operator: Some(if by > 0 {
                Binary::Add
            } else {
                Binary::Subtract
            }),
            value: Box::new(Expression::Int(1)),
        };
        if after && wanted {
            return Err(unsupported(
                line,
                1,
                "Using the old value of a stepped field",
            ));
        }
        self.value(&value, line)?;
        if !wanted {
            return Ok(Type::Void);
        }
        Ok(Type::Int)
    }
}

impl Emitter<'_> {
    fn statement(&mut self, statement: &Positioned<Statement>) -> Result<(), Diagnostic> {
        let line = statement.line;
        match &statement.node {
            Statement::Nothing => Ok(()),
            Statement::Block(inside) => {
                self.open();
                for one in inside {
                    self.statement(one)?;
                }
                self.close();
                Ok(())
            }
            Statement::Declare { what, name, value } => {
                let target = self.resolve(what, line)?;
                match value {
                    Some(expression) => {
                        let found = self.value(expression, line)?;
                        if !found.may_be_given_to(&target) {
                            return Err(at(
                                "EJ230",
                                line,
                                1,
                                format!(
                                    "`{name}` is a {} and was given a {}.",
                                    target.readable(),
                                    found.readable()
                                ),
                            ));
                        }
                        if !found.is_reference() {
                            self.convert(&found, &target, line)?;
                        }
                    }
                    None => {
                        // A local with no value written on it is not readable
                        // until one is, so nothing is stored and nothing is
                        // zeroed: the slot simply exists.
                        let slot = self.declare(name, target);
                        let _ = slot;
                        return Ok(());
                    }
                }
                let slot = self.declare(name, target.clone());
                self.store(slot, &target);
                Ok(())
            }
            Statement::Express(expression) => {
                // The value is not wanted, so anything left behind is popped.
                let what = match expression {
                    Expression::Assign {
                        target,
                        operator,
                        value,
                    } => {
                        self.assign(target, *operator, value, line, false)?;
                        Type::Void
                    }
                    Expression::Step { target, by, after } => {
                        self.step(target, *by, *after, line, false)?;
                        Type::Void
                    }
                    other => self.value(other, line)?,
                };
                match what.width() {
                    0 => {}
                    1 => {
                        self.op(0x57);
                        self.grow(-1);
                    }
                    _ => {
                        self.op(0x58);
                        self.grow(-2);
                    }
                }
                Ok(())
            }
            Statement::If {
                condition,
                then,
                otherwise,
            } => {
                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at(
                        "EJ206",
                        line,
                        1,
                        format!(
                            "An `if` wants a boolean and was given a {}.",
                            found.readable()
                        ),
                    ));
                }
                let to_else = self.jump(0x99);
                self.grow(-1);
                self.statement(then)?;
                match otherwise {
                    Some(otherwise) => {
                        let over = self.jump(0xa7);
                        self.land(to_else);
                        self.statement(otherwise)?;
                        self.land(over);
                    }
                    None => self.land(to_else),
                }
                Ok(())
            }
            Statement::While { condition, body } => {
                let top = self.code.len();
                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at("EJ206", line, 1, "A `while` wants a boolean."));
                }
                let out = self.jump(0x99);
                self.grow(-1);
                self.breaks.push(Vec::new());
                self.continues.push(Vec::new());
                self.statement(body)?;
                for pending in self.continues.pop().unwrap_or_default() {
                    self.land(pending);
                }
                self.jump_back(0xa7, top);
                self.land(out);
                for pending in self.breaks.pop().unwrap_or_default() {
                    self.land(pending);
                }
                Ok(())
            }
            Statement::For {
                start,
                condition,
                step,
                body,
            } => {
                self.open();
                for one in start {
                    self.statement(one)?;
                }
                let top = self.code.len();
                let out = match condition {
                    Some(condition) => {
                        let found = self.value(condition, line)?;
                        if found != Type::Boolean {
                            return Err(at("EJ206", line, 1, "A `for` condition is a boolean."));
                        }
                        let jump = self.jump(0x99);
                        self.grow(-1);
                        Some(jump)
                    }
                    None => None,
                };
                self.breaks.push(Vec::new());
                self.continues.push(Vec::new());
                self.statement(body)?;
                for pending in self.continues.pop().unwrap_or_default() {
                    self.land(pending);
                }
                for expression in step {
                    match expression {
                        Expression::Step { target, by, after } => {
                            self.step(target, *by, *after, line, false)?;
                        }
                        Expression::Assign {
                            target,
                            operator,
                            value,
                        } => {
                            self.assign(target, *operator, value, line, false)?;
                        }
                        other => {
                            let what = self.value(other, line)?;
                            match what.width() {
                                0 => {}
                                1 => {
                                    self.op(0x57);
                                    self.grow(-1);
                                }
                                _ => {
                                    self.op(0x58);
                                    self.grow(-2);
                                }
                            }
                        }
                    }
                }
                self.jump_back(0xa7, top);
                if let Some(out) = out {
                    self.land(out);
                }
                for pending in self.breaks.pop().unwrap_or_default() {
                    self.land(pending);
                }
                self.close();
                Ok(())
            }
            Statement::Break => {
                if self.breaks.is_empty() {
                    return Err(at("EJ231", line, 1, "`break` is not inside a loop."));
                }
                let jump = self.jump(0xa7);
                self.breaks.last_mut().unwrap().push(jump);
                Ok(())
            }
            Statement::Continue => {
                if self.continues.is_empty() {
                    return Err(at("EJ231", line, 1, "`continue` is not inside a loop."));
                }
                let jump = self.jump(0xa7);
                self.continues.last_mut().unwrap().push(jump);
                Ok(())
            }
            Statement::Return(value) => {
                let wanted = self.returns.clone();
                match value {
                    None => {
                        if wanted != Type::Void {
                            return Err(at(
                                "EJ233",
                                line,
                                1,
                                format!(
                                    "This returns a {} and the `return` carries nothing.",
                                    wanted.readable()
                                ),
                            ));
                        }
                        self.op(0xb1);
                    }
                    Some(expression) => {
                        if wanted == Type::Void {
                            return Err(at(
                                "EJ233",
                                line,
                                1,
                                "This returns nothing, and the `return` carries a value.",
                            ));
                        }
                        let found = self.value(expression, line)?;
                        if !found.may_be_given_to(&wanted) {
                            return Err(at(
                                "EJ201",
                                line,
                                1,
                                format!(
                                    "This returns a {} and the `return` carries a {}.",
                                    wanted.readable(),
                                    found.readable()
                                ),
                            ));
                        }
                        if !found.is_reference() {
                            self.convert(&found, &wanted, line)?;
                        }
                        let opcode = match &wanted {
                            Type::Long => 0xadu8,
                            Type::Float => 0xae,
                            Type::Double => 0xaf,
                            other if other.is_reference() => 0xb0,
                            _ => 0xac,
                        };
                        self.op(opcode);
                        self.grow(-i32::from(wanted.width()));
                    }
                }
                Ok(())
            }
        }
    }
}

// ------------------------------------------------------ the class file

fn write_attribute(out: &mut Vec<u8>, name: u16, body: &[u8]) {
    out.extend_from_slice(&name.to_be_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

/// Compiles one unit into the bytes of a class file.
pub fn compile_unit(unit: &Unit, classpath: &Classpath) -> Result<Vec<u8>, Diagnostic> {
    let this_class = unit.internal_name();
    let mut pool = Pool::new();

    let superclass = match &unit.extends {
        Some(name) => {
            let probe = Emitter::new(&mut pool, classpath, unit, this_class.clone(), true);
            probe.resolve_class(name, 1)?
        }
        None => "java/lang/Object".to_string(),
    };

    let mut interfaces = Vec::new();
    for name in &unit.implements {
        let probe = Emitter::new(&mut pool, classpath, unit, this_class.clone(), true);
        interfaces.push(probe.resolve_class(name, 1)?);
    }

    // Fields.
    let mut field_bytes = Vec::new();
    for field in &unit.fields {
        let probe = Emitter::new(&mut pool, classpath, unit, this_class.clone(), true);
        let what = probe.resolve(&field.what, field.line)?;
        let name = pool.utf8(&field.name);
        let descriptor = pool.utf8(&what.descriptor());
        field_bytes.extend_from_slice(&field.modifiers.access_flags(0).to_be_bytes());
        field_bytes.extend_from_slice(&name.to_be_bytes());
        field_bytes.extend_from_slice(&descriptor.to_be_bytes());
        field_bytes.extend_from_slice(&0u16.to_be_bytes());
    }

    // Methods. A class with no constructor written gets the one Java would
    // have written for it, which calls up into the superclass and returns.
    let mut methods: Vec<Method> = unit.methods.clone();
    if !methods.iter().any(|held| held.constructor) {
        methods.insert(
            0,
            Method {
                modifiers: Modifiers {
                    public: unit.modifiers.public,
                    ..Modifiers::default()
                },
                returns: Written::Void,
                name: "<init>".to_string(),
                parameters: Vec::new(),
                body: Some(Vec::new()),
                constructor: true,
                line: 1,
            },
        );
    }

    let code_name = pool.utf8("Code");
    let mut method_bytes = Vec::new();
    let mut written = 0u16;

    for method in &methods {
        let Some(body) = &method.body else {
            let name = pool.utf8(&method.name);
            let probe = Emitter::new(&mut pool, classpath, unit, this_class.clone(), true);
            let mut parameters = Vec::new();
            for (what, _) in &method.parameters {
                parameters.push(probe.resolve(what, method.line)?);
            }
            let returns = probe.resolve(&method.returns, method.line)?;
            let descriptor = Signature {
                owner: this_class.clone(),
                name: method.name.clone(),
                parameters,
                returns,
                static_: method.modifiers.static_,
                interface: false,
            }
            .descriptor();
            let descriptor = pool.utf8(&descriptor);
            method_bytes.extend_from_slice(&method.modifiers.access_flags(0).to_be_bytes());
            method_bytes.extend_from_slice(&name.to_be_bytes());
            method_bytes.extend_from_slice(&descriptor.to_be_bytes());
            method_bytes.extend_from_slice(&0u16.to_be_bytes());
            written += 1;
            continue;
        };

        let mut emitter = Emitter::new(
            &mut pool,
            classpath,
            unit,
            this_class.clone(),
            method.modifiers.static_,
        );
        emitter.open();
        if !method.modifiers.static_ {
            emitter.declare("this", Type::Object(this_class.clone()));
        }
        let mut parameters = Vec::new();
        for (what, name) in &method.parameters {
            let resolved = emitter.resolve(what, method.line)?;
            emitter.declare(name, resolved.clone());
            parameters.push(resolved);
        }
        let returns = emitter.resolve(&method.returns, method.line)?;
        emitter.returns = returns.clone();

        if method.constructor {
            // Every constructor begins by calling one above it, or the class
            // will not load.
            emitter.load(0, &Type::Object(this_class.clone()));
            let up = emitter.pool.method(&superclass, "<init>", "()V", false);
            emitter.op2(0xb7, up);
            emitter.grow(-1);
        }

        for statement in body {
            emitter.statement(statement)?;
        }

        // A method that can fall off its end needs a return there. A `void`
        // one gets it; anything else that reaches the end without returning is
        // a mistake in the source and is said so.
        let ends_returned = matches!(body.last().map(|one| &one.node), Some(Statement::Return(_)));
        if returns == Type::Void {
            if !ends_returned {
                emitter.op(0xb1);
            }
        } else if !ends_returned {
            return Err(at(
                "EJ232",
                method.line,
                1,
                format!(
                    "`{}` can reach its end without returning a value.",
                    method.name
                ),
            ));
        }

        let code = emitter.code;
        let max_stack = emitter.max_depth.max(1) as u16;
        let max_locals = emitter.max_slot.max(1);

        let mut attribute = Vec::new();
        attribute.extend_from_slice(&max_stack.to_be_bytes());
        attribute.extend_from_slice(&max_locals.to_be_bytes());
        attribute.extend_from_slice(&(code.len() as u32).to_be_bytes());
        attribute.extend_from_slice(&code);
        attribute.extend_from_slice(&0u16.to_be_bytes()); // no exception table
        attribute.extend_from_slice(&0u16.to_be_bytes()); // no attributes

        let name = pool.utf8(&method.name);
        let descriptor = Signature {
            owner: this_class.clone(),
            name: method.name.clone(),
            parameters,
            returns,
            static_: method.modifiers.static_,
            interface: false,
        }
        .descriptor();
        let descriptor = pool.utf8(&descriptor);

        method_bytes.extend_from_slice(&method.modifiers.access_flags(0).to_be_bytes());
        method_bytes.extend_from_slice(&name.to_be_bytes());
        method_bytes.extend_from_slice(&descriptor.to_be_bytes());
        method_bytes.extend_from_slice(&1u16.to_be_bytes());
        write_attribute(&mut method_bytes, code_name, &attribute);
        written += 1;
    }

    let this_index = pool.class(&this_class);
    let super_index = pool.class(&superclass);
    let interface_indices: Vec<u16> = interfaces.iter().map(|name| pool.class(name)).collect();

    let mut out = Vec::with_capacity(1024 + method_bytes.len());
    out.extend_from_slice(&0xcafe_babeu32.to_be_bytes());
    out.extend_from_slice(&CLASS_MINOR.to_be_bytes());
    out.extend_from_slice(&CLASS_MAJOR.to_be_bytes());
    pool.write(&mut out);
    // ACC_SUPER, which every class written since Java 1.1 sets.
    out.extend_from_slice(&unit.modifiers.access_flags(0x0020).to_be_bytes());
    out.extend_from_slice(&this_index.to_be_bytes());
    out.extend_from_slice(&super_index.to_be_bytes());
    out.extend_from_slice(&(interface_indices.len() as u16).to_be_bytes());
    for index in interface_indices {
        out.extend_from_slice(&index.to_be_bytes());
    }
    out.extend_from_slice(&(unit.fields.len() as u16).to_be_bytes());
    out.extend_from_slice(&field_bytes);
    out.extend_from_slice(&written.to_be_bytes());
    out.extend_from_slice(&method_bytes);
    out.extend_from_slice(&0u16.to_be_bytes());
    Ok(out)
}

/// Reads Java and writes a class file.
pub fn compile(source: &str, classpath: &Classpath) -> Result<(String, Vec<u8>), Diagnostic> {
    let unit = parse(source)?;
    let name = format!("{}.class", unit.internal_name());
    let bytes = compile_unit(&unit, classpath)?;
    Ok((name, bytes))
}

// ------------------------------------------------------- the contract

pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.java",
    display_name: "Java",
    version: Version::new(0, 2, 0),
    status: Status::Experimental,
    summary: "Reads Java source and writes class files, without a javac anywhere. \
              A subset of the language, named at the top of Compilers/Java.rs, with \
              everything outside it refused by name and line rather than mis-handled.",
    inputs: &["java.source", "jvm.class"],
    outputs: &["jvm.class"],
    required_capabilities: &[Capability::Cache, Capability::TempStorage],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::ProcessExec,
        Capability::KeyAccess,
        Capability::SensitiveOutput,
    ],
    non_responsibilities: &[
        "Converting class files to DEX; that is another step's work.",
        "Packaging or signing anything.",
        "Resolving dependencies. What it may call, it is handed.",
        "Verifying the class files it writes. That is the device's, and d8's.",
    ],
};

pub struct JavaCompiler;

pub static COMPILER: JavaCompiler = JavaCompiler;

/// The old plugin surface, kept so the registry still lists what exists.
///
/// A compilation does not fit through it: it has no request, no session and
/// nowhere to put what it made. Anything that wants to compile Java goes
/// through [`COMPILER`] and the contract in `crate::compiler`.
pub struct JavaPlugin;

pub static PLUGIN: JavaPlugin = JavaPlugin;

impl crate::plugin::Plugin for JavaPlugin {
    fn contract(&self) -> &'static Contract {
        &CONTRACT
    }

    fn execute(
        &self,
        _ctx: &mut crate::plugin::Context<'_>,
    ) -> Result<crate::plugin::Outcome, Diagnostic> {
        Err(fail(
            "EJ300",
            "Java compiles through the compiler contract, not through this one.",
        )
        .with_suggestion(
            "The plugin surface has no request to compile and nowhere to put what comes \
             out. Use crate::compiler::Compiler, which has both.",
        ))
    }
}

impl JavaCompiler {
    /// Everything the classpath knows, out of the class files handed over as
    /// dependencies.
    fn classpath_from(
        &self,
        session: &Session<'_>,
        request: &Request,
    ) -> Result<Classpath, Diagnostic> {
        let mut classpath = Classpath::new();
        for digest in &request.dependencies {
            let bytes = session.store().get(*digest)?;
            if bytes.len() < 4 || bytes[..4] != 0xcafe_babeu32.to_be_bytes() {
                // A dependency that is not a class file is not this compiler's
                // to complain about; something else may want it.
                continue;
            }
            let class = crate::jvm::read(&bytes)?;
            classpath.learn(&class)?;
        }
        Ok(classpath)
    }
}

impl Compiler for JavaCompiler {
    fn contract(&self) -> &'static Contract {
        &CONTRACT
    }

    fn probe(&self) -> Probe {
        Probe::usable(
            Identity::new("java", "omni.java", env!("CARGO_PKG_VERSION"), "jvm")
                .with("languageRelease", LANGUAGE_RELEASE)
                .with("classFileMajor", CLASS_MAJOR.to_string())
                .with("stackMapFrames", "none"),
            // Nothing here reads a clock, a path or an environment variable,
            // and the constant pool is built in the order the source mentions
            // things. The same source is the same class file, on any machine.
            Reproducibility::Always,
        )
    }

    fn plan(&self, request: &Request) -> Result<Plan, Diagnostic> {
        if !crate::compiler::api_is_supported(request.api_level) {
            return Err(fail(
                "EJ301",
                format!(
                    "Android API {} is outside what this builds for.",
                    request.api_level
                ),
            )
            .with_context(format!(
                "Supported: {} to {}",
                crate::compiler::OLDEST_API,
                crate::compiler::NEWEST_API
            )));
        }

        let identity = self
            .probe()
            .identity
            .expect("this compiler is always usable");
        let mut expected = Vec::new();
        for source in request.ordered_sources() {
            // The class a file produces is named by what is in the file, so the
            // plan has to read far enough to know. Everything else about the
            // plan is worked out without compiling.
            expected.push(Expected {
                name: source.path.clone(),
                kind: Kind::JvmClass,
            });
        }

        Ok(Plan {
            key: Plan::key_for(&identity, request),
            identity,
            expected,
            reproducibility: Reproducibility::Always,
        })
    }

    fn compile(
        &self,
        plan: &Plan,
        request: &Request,
        mut session: Session<'_>,
    ) -> Result<Compiled, Diagnostic> {
        let classpath = self.classpath_from(&session, request)?;
        let sources = request.ordered_sources();

        // The sources themselves come from the store, by the digest the request
        // named. A compiler that read them from anywhere else would be
        // compiling something other than what was planned.
        for source in &sources {
            session.carry_on()?;
            let bytes = session.store().get(source.digest)?;
            let text = String::from_utf8(bytes).map_err(|_| {
                fail("EJ302", "A Java source file is not text.")
                    .with_context(format!("Path: {}", source.path))
            })?;
            let (_, class) = compile(&text, &classpath)
                .map_err(|error| error.with_context(format!("File: {}", source.path)))?;
            session.offer(source.path.clone(), Kind::JvmClass, class)?;
        }

        session.commit(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Classpath {
        Classpath::new()
    }

    #[test]
    fn the_contract_says_what_this_is_and_holds_together() {
        assert_eq!(CONTRACT.id, "omni.plugin.java");
        assert!(CONTRACT.status.may_produce_artifacts());
        for required in CONTRACT.required_capabilities {
            assert!(!CONTRACT.forbidden_capabilities.contains(required));
        }
        let wrong = crate::compiler::check_contract(&COMPILER);
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    #[test]
    fn a_class_this_writes_reads_back_as_the_class_it_is() {
        let source = r#"
            package com.my.app;

            public class Counted {
                private int count;
                public static int twice(int value) {
                    return value * 2;
                }
                public int add(int by) {
                    count = count + by;
                    return count;
                }
                public String describe() {
                    return "count is " + count;
                }
                public int sumTo(int limit) {
                    int total = 0;
                    for (int i = 0; i < limit; i++) {
                        if (i % 2 == 0) {
                            total += i;
                        }
                    }
                    return total;
                }
            }
        "#;

        let (name, bytes) = compile(source, &empty()).expect("this must compile");
        assert_eq!(name, "com/my/app/Counted.class");

        // Read back with this project's own class reader, which knows nothing
        // about the compiler that wrote it.
        let class = crate::jvm::read(&bytes).expect("what was written must read");
        // The reader hands names back the way a person writes them.
        assert_eq!(class.name, "com.my.app.Counted");
        assert_eq!(class.superclass.as_deref(), Some("java.lang.Object"));
        assert_eq!(class.major_version, CLASS_MAJOR);

        let names = class.method_names();
        for wanted in ["<init>", "twice", "add", "describe", "sumTo"] {
            assert!(
                names.contains(&wanted),
                "{wanted} is missing from {names:?}"
            );
        }
        assert_eq!(class.fields.len(), 1);
        assert_eq!(class.fields[0].name, "count");
        assert_eq!(class.fields[0].descriptor, "I");

        let twice = class.methods.iter().find(|m| m.name == "twice").unwrap();
        assert_eq!(twice.descriptor, "(I)I");
        assert!(twice.access_flags & 0x0008 != 0, "twice is static");

        let describe = class.methods.iter().find(|m| m.name == "describe").unwrap();
        assert_eq!(describe.descriptor, "()Ljava/lang/String;");

        // The same source is the same bytes, which is what the contract
        // promises when it says Always.
        let (_, again) = compile(source, &empty()).unwrap();
        assert_eq!(bytes, again, "the same source must be the same class file");

        eprintln!(
            "java: {} bytes, {} constants, {} methods, read back by the Core's own reader",
            bytes.len(),
            class.constants.len(),
            class.methods.len()
        );
    }

    #[test]
    fn what_it_does_not_compile_it_refuses_by_name_and_line() {
        for (source, expected) in [
            ("public class A { void f() { switch (1) {} } }", "EJ900"),
            (
                "public class A { void f() { try { } catch (E e) { } } }",
                "EJ900",
            ),
            ("public class A<T> { }", "EJ900"),
            ("public interface A { }", "EJ900"),
            (
                "public class A { void f() { Runnable r = () -> {}; } }",
                "EJ900",
            ),
            ("public class A { class B { } }", "EJ900"),
            ("public class A { void f(int... x) { } }", "EJ900"),
            (
                "public class A { void f() { for (String s : list) { } } }",
                "EJ900",
            ),
        ] {
            let refused = compile(source, &empty()).expect_err("this must be refused");
            assert_eq!(refused.code, expected, "{source}: {}", refused.message);
            assert!(refused.suggestion.is_some(), "{source}");
        }

        // And what it does compile, it type-checks rather than waving through.
        for (source, expected) in [
            ("public class A { int f() { return \"text\"; } }", "EJ201"),
            ("public class A { void f() { int x = nothing; } }", "EJ211"),
            ("public class A { void f() { unknown(); } }", "EJ224"),
            ("public class A { void f() { if (1) { } } }", "EJ206"),
            ("public class A { int f() { int x = 1; } }", "EJ232"),
            ("public class A { void f() { Unknown u = null; } }", "EJ200"),
            ("public class A { void f() { break; } }", "EJ231"),
        ] {
            let refused = compile(source, &empty()).expect_err("this must be refused");
            assert_eq!(refused.code, expected, "{source}: {}", refused.message);
        }

        eprintln!("java: fifteen things refused, each by its own code");
    }

    #[test]
    fn it_calls_into_classes_it_was_handed_and_refuses_ones_it_was_not() {
        // A class file for something to call, written by this compiler, then
        // read back as a dependency -- which is exactly what the contract does
        // with the digests in a request.
        let library = r#"
            package com.my.lib;
            public class Greeter {
                public static String greet(String who) {
                    return "hello " + who;
                }
                public int size;
            }
        "#;
        let (_, bytes) = compile(library, &empty()).unwrap();
        let class = crate::jvm::read(&bytes).unwrap();
        let mut classpath = Classpath::new();
        classpath.learn(&class).unwrap();
        assert_eq!(classpath.len(), 1);

        let caller = r#"
            package com.my.app;
            import com.my.lib.Greeter;
            public class Caller {
                public String hello() {
                    return Greeter.greet("world");
                }
                public int sizeOf(Greeter g) {
                    return g.size;
                }
            }
        "#;
        let (name, made) = compile(caller, &classpath).expect("a call it was handed must compile");
        assert_eq!(name, "com/my/app/Caller.class");
        let read = crate::jvm::read(&made).unwrap();
        assert_eq!(read.name, "com.my.app.Caller");
        assert!(read.method_names().contains(&"hello"));

        // Without the dependency, the same source is refused rather than
        // guessed at.
        let refused = compile(caller, &empty()).expect_err("an unknown class must be refused");
        assert_eq!(refused.code, "EJ200", "{}", refused.message);

        // And a method the class does not have is refused even when the class
        // is known.
        let wrong = r#"
            package com.my.app;
            import com.my.lib.Greeter;
            public class Caller {
                public void f() { Greeter.shout("world"); }
            }
        "#;
        let refused = compile(wrong, &classpath).expect_err("an unknown method must be refused");
        assert_eq!(refused.code, "EJ227", "{}", refused.message);

        eprintln!("java: a class compiled here became the classpath for the next one");
    }
}
