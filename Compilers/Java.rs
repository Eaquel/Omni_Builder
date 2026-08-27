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
//! Handled: a package and its imports; the classes and interfaces one file
//! declares, which may be several and may name each other, with a superclass
//! and interfaces; fields with values on them, initialiser blocks
//! -- static and not -- methods, varargs, and constructors that hand off to
//! `this(...)` or up to `super(...)`; the primitive types, `String`, arrays
//! and declared types; blocks, local declarations -- several to a line, and
//! `var` where the value already says the type -- `if`/`else`, `while`,
//! `do`/`while`, `for` in both the counted and the enhanced form, `switch` as
//! a statement and as an expression, in both the colon and the arrow form and
//! over integers or strings, `try`/`catch`/`finally` including multi-catch,
//! `throw`, `return`, `yield`, `break` and `continue` -- plain and labelled --
//! and expression statements; literals including text blocks and underscored
//! numbers; names, field access, method invocation with the overload chosen by
//! what is handed over, `new`, the arithmetic, comparison, logical and bitwise
//! operators with Java's precedence, assignment and compound assignment,
//! `++`/`--`, casts, array indexing, `this`, `super`, the conditional
//! operator, `instanceof` with a pattern that names what it matched, and
//! string concatenation with `+`.
//!
//! Generic types are read and erased, which is what the JVM does with them:
//! `List<String>` and `List` are the same class at run time, and the only
//! thing lost is the checking `javac` does before erasing them itself.
//!
//! Enums and records are read and turned into the classes they stand for --
//! a final class extending `java.lang.Enum` with its constants, its class
//! initialiser, `values` and `valueOf`; a final class with a field and an
//! accessor per component and a `toString`. A `switch` over an enum compares
//! its constants, which are the only instances of their class there are. A
//! `try` that holds things closes them in the reverse of the order it opened
//! them.
//!
//! There is a small fixed table of runtime signatures -- the exceptions,
//! printing, string work, boxing, arithmetic, the collections -- so that code
//! which has never been handed `android.jar` still compiles. Anything handed
//! over as a dependency wins over it.
//!
//! A class written where it is used -- `new Iface() { ... }` -- and a lambda
//! both become a class of their own, named after the one they were written
//! inside. What they read from around them is what they are built with: the
//! enclosing instance, and every local of the method they were written in that
//! their body names. `javac` writes an `invokedynamic` for a lambda and lets
//! the runtime assemble the class; Android rewrites that back into a class
//! anyway, so it is written out here.
//!
//! A method reference is the same lambda written shorter, so it is written as
//! one. A class, an interface, an enum or a record declared inside another is
//! a class of its own, named for where it was written; one written without
//! `static` belongs to an instance and keeps it in a field, which is what lets
//! it read what that instance holds.
//!
//! Refused, by name, with the line it happened on: `synchronized` and
//! `assert`. Annotations are parsed and discarded, which is safe because they
//! are metadata and nothing here reads them; `@Override` therefore costs
//! nothing.
//!
//! # What it targets
//!
//! Class file major version 69, which is Java 25.
//!
//! That number is not a label. From version 50 the JVM verifies with the
//! type-checking verifier, which wants a StackMapTable attribute on every
//! method that branches, and from 51 a missing one is a hard failure rather
//! than a fallback. Writing 69 without frames would produce class files that
//! read fine, that `javap` prints happily, that pass through `d8` -- and that a
//! real JVM refuses the moment a method contains an `if`. So this writes
//! frames.
//!
//! What that costs is a second thing to be right about. The verifier does not
//! infer the state of the stack and locals at a branch target; it is told, and
//! it checks that every path arriving there agrees with what it was told. A
//! frame that is wrong is worse than no frame, because it is a claim. The
//! emitter therefore tracks what type is in every local and every stack slot as
//! it goes, and records that state wherever a branch can land.

use crate::caps::Capability;
use crate::compiler::{
    Compiled, Compiler, Expected, Identity, Kind, Plan, Probe, Reproducibility, Request, Session,
};
use crate::diag::{Diagnostic, Severity};
use crate::plugin::{Contract, Version};
use crate::FailureClass;
use crate::Status;

pub const ORIGIN: &str = "omni.plugin.java";

/// Java 25 class files. Verified by the type-checking verifier, which means
/// every method that branches carries a StackMapTable and every one of those
/// has to be right. See the note at the top of this file.
pub const CLASS_MAJOR: u16 = 69;
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

fn at_line(code: &str, line: u32, message: impl Into<String>) -> Diagnostic {
    at(code, line, 1, message)
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
            if self.peek(1) == b'"' && self.peek(2) == b'"' {
                return self.text_block(line, column);
            }
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

    /// A text block: `"""`, a line terminator, the content, and `"""` again.
    ///
    /// The rule that makes these worth having is the one about indentation.
    /// A block written inside a method is indented to sit with the code around
    /// it, and none of that indentation is part of the string. Java decides
    /// how much to remove by taking the smallest indentation of any non-blank
    /// line -- and of the closing delimiter's line, whether it is blank or
    /// not, which is what lets the closing `"""` choose the margin on its own.
    fn text_block(&mut self, line: u32, column: u32) -> Result<Token, Diagnostic> {
        self.bump();
        self.bump();
        self.bump();

        // Only whitespace may follow the opening delimiter, and then the line
        // has to end. Anything else is not a text block at all.
        while matches!(self.peek(0), b' ' | b'\t' | b'\x0c') {
            self.bump();
        }
        if self.peek(0) == b'\r' {
            self.bump();
        }
        if self.peek(0) != b'\n' {
            return Err(at(
                "EJ011",
                line,
                column,
                "A text block opens with `\"\"\"` and then a line break.",
            )
            .with_suggestion("Move the content to the next line."));
        }
        self.bump();

        // The raw content, with escapes left alone: an escaped quote must not
        // be able to end the block, and `\\n` must not become a line the
        // indentation rule then measures.
        let mut raw = String::new();
        loop {
            if self.at >= self.source.len() {
                return Err(at("EJ005", line, column, "A text block was never closed."));
            }
            if self.peek(0) == b'"' && self.peek(1) == b'"' && self.peek(2) == b'"' {
                self.bump();
                self.bump();
                self.bump();
                break;
            }
            if self.peek(0) == b'\\' {
                raw.push('\\');
                self.bump();
                if self.at < self.source.len() {
                    let start = self.at;
                    self.bump();
                    while self.peek(0) >= 0x80 && self.peek(0) < 0xc0 {
                        self.bump();
                    }
                    raw.push_str(&String::from_utf8_lossy(&self.source[start..self.at]));
                }
                continue;
            }
            let start = self.at;
            self.bump();
            while self.peek(0) >= 0x80 && self.peek(0) < 0xc0 {
                self.bump();
            }
            raw.push_str(&String::from_utf8_lossy(&self.source[start..self.at]));
        }

        let stripped = strip_incidental_whitespace(&raw);
        Ok(Token::Str(text_block_escapes(&stripped, line, column)?))
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

/// Removes the indentation that is there to make the source readable rather
/// than to be part of the string.
///
/// The measure is the smallest indentation of any non-blank line, and of the
/// closing delimiter's own line when it has one -- so moving the closing
/// `"""` left or right moves the margin without touching the content. Trailing
/// whitespace goes from every line, because it is invisible and therefore
/// cannot have been meant.
fn strip_incidental_whitespace(raw: &str) -> String {
    let normalised = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalised.split('\n').collect();

    let closing_is_alone = lines
        .last()
        .is_some_and(|last| last.chars().all(char::is_whitespace));

    let mut margin = usize::MAX;
    for (index, line) in lines.iter().enumerate() {
        let blank = line.chars().all(char::is_whitespace);
        let last = index + 1 == lines.len();
        if blank && !(last && closing_is_alone) {
            continue;
        }
        let indent = if blank {
            line.chars().count()
        } else {
            line.chars().take_while(|one| one.is_whitespace()).count()
        };
        margin = margin.min(indent);
    }
    if margin == usize::MAX {
        margin = 0;
    }

    let mut kept: Vec<String> = lines
        .iter()
        .map(|line| {
            let body: String = line.chars().skip(margin).collect();
            body.trim_end().to_string()
        })
        .collect();
    if closing_is_alone && kept.len() > 1 {
        // The delimiter's own line is the margin, not content. What it leaves
        // behind is the line break before it.
        kept.pop();
        kept.push(String::new());
    }
    kept.join("\n")
}

/// The escapes a text block takes, which are the ordinary ones and two more.
///
/// `\s` is a space that survives the trailing-whitespace rule, and a backslash
/// at the end of a line joins it to the next one. Both exist because the rule
/// that removes invisible whitespace would otherwise make some strings
/// impossible to write.
fn text_block_escapes(text: &str, line: u32, column: u32) -> Result<String, Diagnostic> {
    let source: Vec<char> = text.chars().collect();
    let mut units: Vec<u16> = Vec::new();
    let mut cursor = 0usize;
    while cursor < source.len() {
        if source[cursor] != '\\' {
            let mut buffer = [0u16; 2];
            units.extend_from_slice(source[cursor].encode_utf16(&mut buffer));
            cursor += 1;
            continue;
        }
        cursor += 1;
        let Some(&marker) = source.get(cursor) else {
            return Err(at_end_of_escape(line, column));
        };
        cursor += 1;
        match marker {
            'n' => units.push(u16::from(b'\n')),
            't' => units.push(u16::from(b'\t')),
            'r' => units.push(u16::from(b'\r')),
            'b' => units.push(8),
            'f' => units.push(12),
            's' => units.push(u16::from(b' ')),
            '0'..='7' => {
                // An octal escape is one to three digits, and no more than
                // 0377.
                let mut value = u32::from(marker) - u32::from('0');
                let mut taken = 1;
                while taken < 3 {
                    match source.get(cursor) {
                        Some(next @ '0'..='7') if value * 8 + 7 <= 0o377 => {
                            value = value * 8 + (u32::from(*next) - u32::from('0'));
                            cursor += 1;
                            taken += 1;
                        }
                        _ => break,
                    }
                }
                units.push(value as u16);
            }
            '\'' | '"' | '\\' => {
                let mut buffer = [0u16; 2];
                units.extend_from_slice(marker.encode_utf16(&mut buffer));
            }
            // A backslash cursor the end of a line joins the two lines.
            '\n' => {}
            'u' => {
                while source.get(cursor) == Some(&'u') {
                    cursor += 1;
                }
                let mut value = 0u16;
                for _ in 0..4 {
                    let Some(digit) = source.get(cursor).and_then(|one| one.to_digit(16)) else {
                        return Err(at(
                            "EJ006",
                            line,
                            column,
                            "A `\\u` escape wants four hexadecimal digits.",
                        ));
                    };
                    value = value * 16 + digit as u16;
                    cursor += 1;
                }
                units.push(value);
            }
            other => {
                return Err(at(
                    "EJ006",
                    line,
                    column,
                    format!("`\\{other}` is not an escape Java has."),
                ))
            }
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

fn at_end_of_escape(line: u32, column: u32) -> Diagnostic {
    at(
        "EJ006",
        line,
        column,
        "A backslash was the last thing in the text block.",
    )
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
    /// `var`. The type is whatever the value turns out to be, so there is
    /// nothing to resolve until there is a value to resolve it from.
    Inferred,
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
        /// One per dimension written with a length in it. `new int[3][4]` has
        /// two; `new int[3][]` has one and is still two-dimensional.
        lengths: Vec<Expression>,
        /// How many empty pairs of brackets follow the ones with lengths.
        empty: usize,
    },
    /// `{ 1, 2, 3 }`, with or without `new int[]` in front of it.
    ArrayOf {
        of: Option<Written>,
        values: Vec<Expression>,
        line: u32,
    },
    /// `Foo.class`.
    ClassLiteral {
        of: Written,
        line: u32,
    },
    /// `Outer.this`, inside a class that belongs to an instance of `Outer`.
    OuterThis {
        of: Written,
        line: u32,
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
        /// `o instanceof String s`, which names the value after the check has
        /// passed.
        binds: Option<String>,
    },
    /// A `switch` used for its value rather than its effect.
    Switch {
        subject: Box<Expression>,
        arms: Vec<Arm>,
    },
    /// `Type::method`, `value::method`, `Type::new`: a lambda whose body is
    /// one call, and whose parameters are whatever the interface hands over.
    MethodRef {
        on: Box<Expression>,
        name: String,
        line: u32,
    },
    /// `x -> ...`: the one method of an interface, written where it is
    /// wanted. Which interface is decided by what it is being handed to.
    Lambda {
        parameters: Vec<(Option<Written>, String)>,
        /// The body, as a block. An expression body is wrapped in one.
        body: Vec<Positioned<Statement>>,
        /// True when it was written as an expression, which means its value is
        /// what the method returns -- unless the method returns nothing.
        expression: bool,
        line: u32,
    },
    /// A class with no name of its own, written where it is used.
    Anonymous {
        what: Written,
        arguments: Vec<Expression>,
        body: Box<Body>,
        line: u32,
        column: u32,
    },
}

/// The members of a class body written without a name around it.
#[derive(Clone, Debug)]
pub struct Body {
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub instance_setup: Vec<Positioned<Statement>>,
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
    DoWhile {
        body: Box<Positioned<Statement>>,
        condition: Expression,
    },
    /// `for (T name : over)`, over an array.
    ForEach {
        what: Written,
        name: String,
        over: Expression,
        body: Box<Positioned<Statement>>,
    },
    Switch {
        subject: Expression,
        arms: Vec<Arm>,
    },
    Try {
        body: Vec<Positioned<Statement>>,
        catches: Vec<Catch>,
        finally: Option<Vec<Positioned<Statement>>>,
    },
    Throw(Expression),
    /// A name in front of a statement, which `break` and `continue` can then
    /// say by name.
    Labelled {
        label: String,
        body: Box<Positioned<Statement>>,
    },
    /// `synchronized (x) { ... }`: the lock is taken before the block and
    /// given back after it, however the block is left.
    Synchronized {
        on: Expression,
        body: Vec<Positioned<Statement>>,
    },
    /// `assert x;` and `assert x : said;`, which run only where the runtime
    /// was asked for them.
    Assert {
        condition: Expression,
        said: Option<Expression>,
    },
    Return(Option<Expression>),
    Break(Option<String>),
    Continue(Option<String>),
    /// `yield`, which is how a `switch` written with `:` says what its value
    /// is.
    Yield(Expression),
    /// `super(...)` or `this(...)`, which may only be the first statement of a
    /// constructor.
    Chain {
        to_super: bool,
        arguments: Vec<Expression>,
    },
    /// Several declarations written as one, which share a scope with what is
    /// around them rather than opening one of their own.
    Several(Vec<Positioned<Statement>>),
    Nothing,
}

/// One arm of a `switch`.
#[derive(Clone, Debug)]
pub struct Arm {
    /// The values this arm answers to. Empty means `default`.
    pub labels: Vec<Expression>,
    /// Written `->` rather than `:`, which means it does not fall through.
    pub arrow: bool,
    pub body: Vec<Positioned<Statement>>,
    pub line: u32,
    pub column: u32,
}

/// Turns an enum into the class it stands for.
///
/// There is no enum in a class file. There is a final class extending
/// `java.lang.Enum` with one static field per constant, a class initialiser
/// that makes them, a private constructor that hands the name and the position
/// upwards, and the two static methods every enum has. `javac` writes all of
/// that too; the difference is only that this writes it as source-shaped
/// members before anything else looks, so nothing downstream needs to know an
/// enum was ever involved.
fn as_the_class_it_is(mut unit: Unit, constants: &[Constant]) -> Unit {
    let named = Written::Named(unit.name.clone());
    let array_of = Written::Array(Box::new(named.clone()));
    let public_constant = Modifiers {
        public: true,
        static_: true,
        final_: true,
        ..Modifiers::default()
    };
    let at = |line: u32, node: Statement| Positioned {
        node,
        line,
        column: 1,
    };
    let set = |line: u32, target: Expression, value: Expression| {
        at(
            line,
            Statement::Express(Expression::Assign {
                target: Box::new(target),
                operator: None,
                value: Box::new(value),
            }),
        )
    };

    // Every constant is a field of the enum's own type, and the class
    // initialiser is what fills them in -- in the order they were written,
    // because that order is what `ordinal` means.
    let mut setup = Vec::new();
    for (position, constant) in constants.iter().enumerate() {
        unit.fields.push(Field {
            modifiers: public_constant,
            what: named.clone(),
            name: constant.name.clone(),
            value: None,
            line: constant.line,
        });
        let mut arguments = vec![
            Expression::Str(constant.name.clone()),
            Expression::Int(position as i64),
        ];
        arguments.extend(constant.arguments.iter().cloned());
        setup.push(set(
            constant.line,
            Expression::Name(constant.name.clone()),
            Expression::New {
                what: named.clone(),
                arguments,
            },
        ));
    }

    // The array `values()` hands out a copy of. It is private so that nothing
    // outside can reach in and change what the enum's constants are.
    const HELD: &str = "$VALUES";
    unit.fields.push(Field {
        modifiers: Modifiers {
            private: true,
            static_: true,
            final_: true,
            ..Modifiers::default()
        },
        what: array_of.clone(),
        name: HELD.to_string(),
        value: None,
        line: 1,
    });
    setup.push(set(
        1,
        Expression::Name(HELD.to_string()),
        Expression::NewArray {
            of: named.clone(),
            lengths: vec![Expression::Int(constants.len() as i64)],
            empty: 0,
        },
    ));
    for (position, constant) in constants.iter().enumerate() {
        setup.push(set(
            constant.line,
            Expression::Index {
                of: Box::new(Expression::Name(HELD.to_string())),
                at: Box::new(Expression::Int(position as i64)),
            },
            Expression::Name(constant.name.clone()),
        ));
    }
    // Before anything the person wrote in a `static` block, because that block
    // may name the constants.
    setup.extend(std::mem::take(&mut unit.static_setup));
    unit.static_setup = setup;

    // Every constructor gains the name and the position in front of what was
    // written, and hands them up. A constructor nobody wrote is the one taking
    // nothing else.
    if !unit.methods.iter().any(|held| held.constructor) {
        unit.methods.push(Method {
            modifiers: Modifiers {
                private: true,
                ..Modifiers::default()
            },
            returns: Written::Void,
            name: "<init>".to_string(),
            parameters: Vec::new(),
            body: Some(Vec::new()),
            constructor: true,
            variadic: false,
            line: 1,
        });
    }
    for method in &mut unit.methods {
        if !method.constructor {
            continue;
        }
        method.modifiers.public = false;
        method.modifiers.protected = false;
        method.modifiers.private = true;
        let mut parameters = vec![
            (Written::Named("String".to_string()), "$name".to_string()),
            (Written::Int, "$ordinal".to_string()),
        ];
        parameters.append(&mut method.parameters);
        method.parameters = parameters;
        let mut body = vec![at(
            method.line,
            Statement::Chain {
                to_super: true,
                arguments: vec![
                    Expression::Name("$name".to_string()),
                    Expression::Name("$ordinal".to_string()),
                ],
            },
        )];
        body.extend(method.body.take().unwrap_or_default());
        method.body = Some(body);
    }

    // `values()` hands out a new array every time, which is what a copy is
    // for: nothing a caller does to it can reach the enum.
    let mut values = vec![at(
        1,
        Statement::Declare {
            what: array_of.clone(),
            name: "$out".to_string(),
            value: Some(Expression::NewArray {
                of: named.clone(),
                lengths: vec![Expression::Int(constants.len() as i64)],
                empty: 0,
            }),
        },
    )];
    for position in 0..constants.len() {
        values.push(set(
            1,
            Expression::Index {
                of: Box::new(Expression::Name("$out".to_string())),
                at: Box::new(Expression::Int(position as i64)),
            },
            Expression::Index {
                of: Box::new(Expression::Name(HELD.to_string())),
                at: Box::new(Expression::Int(position as i64)),
            },
        ));
    }
    values.push(at(
        1,
        Statement::Return(Some(Expression::Name("$out".to_string()))),
    ));
    unit.methods.push(Method {
        modifiers: Modifiers {
            public: true,
            static_: true,
            ..Modifiers::default()
        },
        returns: array_of,
        name: "values".to_string(),
        parameters: Vec::new(),
        body: Some(values),
        constructor: false,
        variadic: false,
        line: 1,
    });

    // `valueOf` is written out rather than handed to `Enum.valueOf`, which
    // would need a class literal this compiler has no way to write.
    let mut value_of = Vec::new();
    for constant in constants {
        value_of.push(at(
            constant.line,
            Statement::If {
                condition: Expression::Call {
                    on: Some(Box::new(Expression::Name("$name".to_string()))),
                    super_call: false,
                    name: "equals".to_string(),
                    arguments: vec![Expression::Str(constant.name.clone())],
                },
                then: Box::new(at(
                    constant.line,
                    Statement::Return(Some(Expression::Name(constant.name.clone()))),
                )),
                otherwise: None,
            },
        ));
    }
    value_of.push(at(
        1,
        Statement::Throw(Expression::New {
            what: Written::Named("IllegalArgumentException".to_string()),
            arguments: vec![Expression::Name("$name".to_string())],
        }),
    ));
    unit.methods.push(Method {
        modifiers: Modifiers {
            public: true,
            static_: true,
            ..Modifiers::default()
        },
        returns: named,
        name: "valueOf".to_string(),
        parameters: vec![(Written::Named("String".to_string()), "$name".to_string())],
        body: Some(value_of),
        constructor: false,
        variadic: false,
        line: 1,
    });

    unit.extends = Some("java.lang.Enum".to_string());
    unit.modifiers.final_ = true;
    unit
}

/// Turns a record into the class it stands for.
///
/// A record is a final class extending `java.lang.Record` with a private final
/// field per component, a constructor that fills them in, an accessor per
/// component, and `equals`, `hashCode` and `toString`. `javac` writes those
/// three with `invokedynamic` and a bootstrap that builds them at run time;
/// they are written out here instead, because an `invokedynamic` is a call
/// into a class this compiler would then have to know about and Android
/// rewrites it anyway.
fn as_the_class_a_record_is(
    mut unit: Unit,
    components: &[(Written, String)],
) -> Result<Unit, Diagnostic> {
    let at = |node: Statement| Positioned {
        node,
        line: 1,
        column: 1,
    };
    let private_final = Modifiers {
        private: true,
        final_: true,
        ..Modifiers::default()
    };
    let public = Modifiers {
        public: true,
        ..Modifiers::default()
    };

    for (what, name) in components {
        if unit.fields.iter().any(|held| held.name == *name) {
            return Err(at_line(
                "EJ117",
                1,
                format!("`{name}` is both a component of this record and a field of it."),
            ));
        }
        unit.fields.push(Field {
            modifiers: private_final,
            what: what.clone(),
            name: name.clone(),
            value: None,
            line: 1,
        });
    }

    // The canonical constructor, unless the person wrote one taking exactly
    // the components.
    let written_canonical = unit
        .methods
        .iter()
        .any(|held| held.constructor && held.parameters.len() == components.len());
    if !written_canonical {
        let body = components
            .iter()
            .map(|(_, name)| {
                at(Statement::Express(Expression::Assign {
                    target: Box::new(Expression::Field {
                        of: Box::new(Expression::This),
                        name: name.clone(),
                    }),
                    operator: None,
                    value: Box::new(Expression::Name(name.clone())),
                }))
            })
            .collect();
        unit.methods.push(Method {
            modifiers: public,
            returns: Written::Void,
            name: "<init>".to_string(),
            parameters: components.to_vec(),
            body: Some(body),
            constructor: true,
            variadic: false,
            line: 1,
        });
    }

    // An accessor per component, unless one was written.
    for (what, name) in components {
        if unit
            .methods
            .iter()
            .any(|held| held.name == *name && held.parameters.is_empty())
        {
            continue;
        }
        unit.methods.push(Method {
            modifiers: public,
            returns: what.clone(),
            name: name.clone(),
            parameters: Vec::new(),
            body: Some(vec![at(Statement::Return(Some(Expression::Name(
                name.clone(),
            ))))]),
            constructor: false,
            variadic: false,
            line: 1,
        });
    }

    // `toString`, as `Name[x=1, y=2]`, which is the shape the language
    // specifies.
    if !unit
        .methods
        .iter()
        .any(|held| held.name == "toString" && held.parameters.is_empty())
    {
        let mut text = Expression::Str(format!("{}[", unit.name));
        for (index, (_, name)) in components.iter().enumerate() {
            let lead = if index == 0 {
                format!("{name}=")
            } else {
                format!(", {name}=")
            };
            text = Expression::Binary {
                operator: Binary::Add,
                left: Box::new(text),
                right: Box::new(Expression::Str(lead)),
            };
            text = Expression::Binary {
                operator: Binary::Add,
                left: Box::new(text),
                right: Box::new(Expression::Name(name.clone())),
            };
        }
        text = Expression::Binary {
            operator: Binary::Add,
            left: Box::new(text),
            right: Box::new(Expression::Str("]".to_string())),
        };
        unit.methods.push(Method {
            modifiers: public,
            returns: Written::Named("String".to_string()),
            name: "toString".to_string(),
            parameters: Vec::new(),
            body: Some(vec![at(Statement::Return(Some(text)))]),
            constructor: false,
            variadic: false,
            line: 1,
        });
    }

    unit.implements.push("java.lang.Record".to_string());
    unit.modifiers.final_ = true;
    Ok(unit)
}

/// Gives a class written inside another the instance it belongs to.
///
/// A nested class written without `static` can read what the instance around
/// it holds, so it keeps that instance in a field and every constructor takes
/// it. `Outer.this` is that field; `new Inner()` written inside `Outer` hands
/// `this` over without anybody writing it down.
fn belonging_to_an_instance(mut unit: Unit, holder: &str, package: &Option<String>) -> Unit {
    let enclosing = match package {
        Some(package) => format!("{}/{holder}", package.replace('.', "/")),
        None => holder.to_string(),
    };
    let written = Written::Named(enclosing.replace('/', "."));

    unit.fields.insert(
        0,
        Field {
            modifiers: Modifiers {
                private: true,
                final_: true,
                ..Modifiers::default()
            },
            what: written.clone(),
            name: OUTER.to_string(),
            value: None,
            line: 1,
        },
    );

    if !unit.methods.iter().any(|held| held.constructor) {
        unit.methods.push(Method {
            modifiers: Modifiers {
                public: true,
                ..Modifiers::default()
            },
            returns: Written::Void,
            name: "<init>".to_string(),
            parameters: Vec::new(),
            body: Some(Vec::new()),
            constructor: true,
            variadic: false,
            line: 1,
        });
    }
    for method in &mut unit.methods {
        if !method.constructor {
            continue;
        }
        let mut parameters = vec![(written.clone(), OUTER.to_string())];
        parameters.append(&mut method.parameters);
        method.parameters = parameters;

        // After the call up into the superclass, which has to be first.
        let mut body = method.body.take().unwrap_or_default();
        let filling = Positioned {
            node: Statement::Express(Expression::Assign {
                target: Box::new(Expression::Field {
                    of: Box::new(Expression::This),
                    name: OUTER.to_string(),
                }),
                operator: None,
                value: Box::new(Expression::Name(OUTER.to_string())),
            }),
            line: method.line,
            column: 1,
        };
        let at = usize::from(matches!(
            body.first().map(|one| &one.node),
            Some(Statement::Chain { .. })
        ));
        body.insert(at, filling);
        method.body = Some(body);
    }

    unit.outer = Some(enclosing);
    unit
}

/// The same type with the flag its assertions are guarded by, and the line
/// that works it out.
fn with_the_assertion_flag(unit: &Unit) -> Unit {
    let mut held = unit.clone();
    held.fields.push(Field {
        modifiers: Modifiers {
            static_: true,
            final_: true,
            ..Modifiers::default()
        },
        what: Written::Boolean,
        name: ASSERTIONS_OFF.to_string(),
        value: None,
        line: 1,
    });
    held.static_setup.insert(
        0,
        Positioned {
            node: Statement::Express(Expression::Assign {
                target: Box::new(Expression::Name(ASSERTIONS_OFF.to_string())),
                operator: None,
                value: Box::new(Expression::Unary {
                    operator: Unary::Not,
                    of: Box::new(Expression::Call {
                        on: Some(Box::new(Expression::ClassLiteral {
                            of: Written::Named(unit.name.clone()),
                            line: 1,
                        })),
                        super_call: false,
                        name: "desiredAssertionStatus".to_string(),
                        arguments: Vec::new(),
                    }),
                }),
            }),
            line: 1,
            column: 1,
        },
    );
    held
}

/// Whether anything in this type writes an `assert`.
fn holds_an_assert(unit: &Unit) -> bool {
    fn inside(statement: &Statement) -> bool {
        let mut found = matches!(statement, Statement::Assert { .. });
        match statement {
            Statement::Block(held) | Statement::Several(held) => {
                found = found || held.iter().any(|one| inside(&one.node))
            }
            Statement::If {
                then, otherwise, ..
            } => {
                found = found
                    || inside(&then.node)
                    || otherwise.as_ref().is_some_and(|one| inside(&one.node))
            }
            Statement::While { body, .. }
            | Statement::DoWhile { body, .. }
            | Statement::For { body, .. }
            | Statement::ForEach { body, .. }
            | Statement::Labelled { body, .. } => found = found || inside(&body.node),
            Statement::Switch { arms, .. } => {
                found = found
                    || arms
                        .iter()
                        .any(|arm| arm.body.iter().any(|one| inside(&one.node)))
            }
            Statement::Try {
                body,
                catches,
                finally,
            } => {
                found = found
                    || body.iter().any(|one| inside(&one.node))
                    || catches
                        .iter()
                        .any(|catch| catch.body.iter().any(|one| inside(&one.node)))
                    || finally
                        .as_ref()
                        .is_some_and(|held| held.iter().any(|one| inside(&one.node)))
            }
            Statement::Synchronized { body, .. } => {
                found = found || body.iter().any(|one| inside(&one.node))
            }
            _ => {}
        }
        found
    }

    unit.methods
        .iter()
        .flat_map(|method| method.body.iter().flatten())
        .chain(unit.instance_setup.iter())
        .chain(unit.static_setup.iter())
        .any(|one| inside(&one.node))
}

/// The field an `assert` is guarded by, which the class initialiser works out
/// once from what the runtime was asked for.
const ASSERTIONS_OFF: &str = "$assertionsDisabled";

/// The field a class written where it is used holds its enclosing instance in.
const OUTER: &str = "$outer";

/// A type as it would have been written, so that a synthesised member can name
/// it. Only what a name can stand for: a primitive, an array, or a class.
fn written_for(what: &Type) -> Option<Written> {
    Some(match what {
        Type::Void => return None,
        Type::Boolean => Written::Boolean,
        Type::Byte => Written::Byte,
        Type::Short => Written::Short,
        Type::Char => Written::Char,
        Type::Int => Written::Int,
        Type::Long => Written::Long,
        Type::Float => Written::Float,
        Type::Double => Written::Double,
        Type::Array(of) => Written::Array(Box::new(written_for(of)?)),
        Type::Object(name) => Written::Named(name.replace('/', ".")),
    })
}

/// Every bare name a class body reads.
///
/// More than it needs: a name a method of the body declares itself is counted
/// too. That costs an unused field at worst, because a local always wins over
/// a field where both are visible -- and missing one would be a name that
/// resolves to nothing.
fn names_read(body: &Body, out: &mut Vec<String>) {
    for field in &body.fields {
        if let Some(value) = &field.value {
            names_in_expression(value, out);
        }
    }
    for method in &body.methods {
        for statement in method.body.iter().flatten() {
            names_in_statement(&statement.node, out);
        }
    }
    for statement in &body.instance_setup {
        names_in_statement(&statement.node, out);
    }
}

/// Every bare name a class body writes to.
fn names_assigned(body: &Body, out: &mut Vec<String>) {
    fn assigned_in_expression(expression: &Expression, out: &mut Vec<String>) {
        match expression {
            Expression::Assign { target, value, .. } => {
                if let Expression::Name(name) = target.as_ref() {
                    out.push(name.clone());
                }
                assigned_in_expression(value, out);
            }
            Expression::Step { target, .. } => {
                if let Expression::Name(name) = target.as_ref() {
                    out.push(name.clone());
                }
            }
            _ => walk_expression(expression, &mut |inside| {
                assigned_in_expression(inside, out)
            }),
        }
    }

    let mut visit = |expression: &Expression| assigned_in_expression(expression, out);
    for field in &body.fields {
        if let Some(value) = &field.value {
            visit(value);
        }
    }
    for method in &body.methods {
        for statement in method.body.iter().flatten() {
            walk_statement(&statement.node, &mut visit);
        }
    }
    for statement in &body.instance_setup {
        walk_statement(&statement.node, &mut visit);
    }
}

fn names_in_statement(statement: &Statement, out: &mut Vec<String>) {
    walk_statement(statement, &mut |expression| {
        names_in_expression(expression, out)
    });
}

fn names_in_expression(expression: &Expression, out: &mut Vec<String>) {
    if let Expression::Name(name) = expression {
        out.push(name.clone());
    }
    walk_expression(expression, &mut |inside| names_in_expression(inside, out));
}

/// Hands every expression written directly in a statement to `visit`, and
/// walks into the statements inside it.
fn walk_statement(statement: &Statement, visit: &mut impl FnMut(&Expression)) {
    match statement {
        Statement::Nothing | Statement::Break(_) | Statement::Continue(_) => {}
        Statement::Block(held) | Statement::Several(held) => {
            for one in held {
                walk_statement(&one.node, visit);
            }
        }
        Statement::Declare { value, .. } => {
            if let Some(value) = value {
                visit(value);
            }
        }
        Statement::Express(value) | Statement::Throw(value) | Statement::Yield(value) => {
            visit(value)
        }
        Statement::Return(value) => {
            if let Some(value) = value {
                visit(value);
            }
        }
        Statement::If {
            condition,
            then,
            otherwise,
        } => {
            visit(condition);
            walk_statement(&then.node, visit);
            if let Some(otherwise) = otherwise {
                walk_statement(&otherwise.node, visit);
            }
        }
        Statement::While { condition, body } | Statement::DoWhile { body, condition } => {
            visit(condition);
            walk_statement(&body.node, visit);
        }
        Statement::For {
            start,
            condition,
            step,
            body,
        } => {
            for one in start {
                walk_statement(&one.node, visit);
            }
            if let Some(condition) = condition {
                visit(condition);
            }
            for one in step {
                visit(one);
            }
            walk_statement(&body.node, visit);
        }
        Statement::ForEach { over, body, .. } => {
            visit(over);
            walk_statement(&body.node, visit);
        }
        Statement::Switch { subject, arms } => {
            visit(subject);
            for arm in arms {
                for label in &arm.labels {
                    visit(label);
                }
                for one in &arm.body {
                    walk_statement(&one.node, visit);
                }
            }
        }
        Statement::Try {
            body,
            catches,
            finally,
        } => {
            for one in body {
                walk_statement(&one.node, visit);
            }
            for catch in catches {
                for one in &catch.body {
                    walk_statement(&one.node, visit);
                }
            }
            for one in finally.iter().flatten() {
                walk_statement(&one.node, visit);
            }
        }
        Statement::Labelled { body, .. } => walk_statement(&body.node, visit),
        Statement::Synchronized { on, body } => {
            visit(on);
            for one in body {
                walk_statement(&one.node, visit);
            }
        }
        Statement::Assert { condition, said } => {
            visit(condition);
            if let Some(said) = said {
                visit(said);
            }
        }
        Statement::Chain { arguments, .. } => {
            for one in arguments {
                visit(one);
            }
        }
    }
}

/// Hands every expression written directly inside this one to `visit`.
fn walk_expression(expression: &Expression, visit: &mut impl FnMut(&Expression)) {
    match expression {
        Expression::Int(_)
        | Expression::Long(_)
        | Expression::Float(_)
        | Expression::Double(_)
        | Expression::Char(_)
        | Expression::Str(_)
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::This
        | Expression::Name(_) => {}
        Expression::Field { of, .. } => visit(of),
        Expression::Call { on, arguments, .. } => {
            if let Some(on) = on {
                visit(on);
            }
            arguments.iter().for_each(visit);
        }
        Expression::New { arguments, .. } => arguments.iter().for_each(visit),
        Expression::NewArray { lengths, .. } => {
            for one in lengths {
                visit(one);
            }
        }
        Expression::ArrayOf { values, .. } => {
            for one in values {
                visit(one);
            }
        }
        Expression::ClassLiteral { .. } | Expression::OuterThis { .. } => {}
        Expression::Index { of, at } => {
            visit(of);
            visit(at);
        }
        Expression::Unary { of, .. } => visit(of),
        Expression::Binary { left, right, .. } => {
            visit(left);
            visit(right);
        }
        Expression::Assign { target, value, .. } => {
            visit(target);
            visit(value);
        }
        Expression::Step { target, .. } => visit(target),
        Expression::Cast { of, .. } => visit(of),
        Expression::Conditional {
            condition,
            then,
            otherwise,
        } => {
            visit(condition);
            visit(then);
            visit(otherwise);
        }
        Expression::InstanceOf { of, .. } => visit(of),
        Expression::Switch { subject, arms } => {
            visit(subject);
            for arm in arms {
                arm.labels.iter().for_each(&mut *visit);
                for statement in &arm.body {
                    walk_statement(&statement.node, visit);
                }
            }
        }
        Expression::Lambda { body, .. } => {
            for statement in body {
                walk_statement(&statement.node, visit);
            }
        }
        Expression::MethodRef { on, .. } => visit(on),
        Expression::Anonymous {
            arguments, body, ..
        } => {
            arguments.iter().for_each(&mut *visit);
            // A class inside a class reads from further out too, and what it
            // reads has to reach the one holding it.
            for field in &body.fields {
                if let Some(value) = &field.value {
                    visit(value);
                }
            }
            for method in &body.methods {
                for statement in method.body.iter().flatten() {
                    walk_statement(&statement.node, visit);
                }
            }
            for statement in &body.instance_setup {
                walk_statement(&statement.node, visit);
            }
        }
    }
}

/// One constant of an enum, as it was written.
#[derive(Clone, Debug)]
pub struct Constant {
    pub name: String,
    pub arguments: Vec<Expression>,
    pub line: u32,
    pub column: u32,
}

/// One `catch` clause.
#[derive(Clone, Debug)]
pub struct Catch {
    /// More than one for `catch (A | B name)`.
    pub types: Vec<Written>,
    pub name: String,
    pub body: Vec<Positioned<Statement>>,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    pub public: bool,
    pub private: bool,
    pub protected: bool,
    pub static_: bool,
    pub final_: bool,
    pub abstract_: bool,
    pub synchronized: bool,
    pub volatile: bool,
    pub transient: bool,
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
        if self.synchronized {
            flags |= 0x0020;
        }
        if self.volatile {
            flags |= 0x0040;
        }
        if self.transient {
            flags |= 0x0080;
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
    /// What was written on it, which runs in a constructor for an instance
    /// field and in the class initialiser for a static one.
    pub value: Option<Expression>,
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
    /// Written with `...` on the last parameter, so a call may leave the array
    /// unwritten and hand over the elements instead.
    pub variadic: bool,
    pub line: u32,
}

/// What kind of type a declaration is.
///
/// The JVM has one shape for all of these -- a class file -- and the
/// differences are access flags, a superclass, and members written by the
/// compiler rather than by the person.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Class,
    Interface,
    /// Read as an enum and turned into the class it stands for before anything
    /// else sees it. See [`as_the_class_it_is`].
    Enum,
    /// Read as a record and turned into the class it stands for the same way.
    /// See [`as_the_class_a_record_is`].
    Record,
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub shape: Shape,
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub modifiers: Modifiers,
    pub name: String,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    /// `{ ... }` written directly in the class body, which runs at the top of
    /// every constructor.
    pub instance_setup: Vec<Positioned<Statement>>,
    /// `static { ... }`, which runs once when the class is loaded.
    pub static_setup: Vec<Positioned<Statement>>,
    /// The class this one was written inside, when it has no name of its own.
    /// Its instance is held in a field, and a name this class cannot find is
    /// looked for there before it is refused.
    pub outer: Option<String>,
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
                // `default` in front of a member is an interface method with a
                // body. The flag it needs is the absence of ACC_ABSTRACT, so
                // there is nothing to record beyond having read it.
                "default" if !matches!(self.ahead(1), Token::Punctuation(":")) => {}
                // `synchronized` on a method takes the object's own lock
                // around the whole of it, which is a flag rather than an
                // instruction. `volatile` and `transient` are flags too.
                "synchronized" if !matches!(self.ahead(1), Token::Punctuation("(")) => {
                    found.synchronized = true
                }
                "volatile" => found.volatile = true,
                "transient" => found.transient = true,
                "native" | "strictfp" => {
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
            Token::Identifier(name)
                if name == "var"
                    && !matches!(
                        self.ahead(1),
                        Token::Punctuation(".") | Token::Punctuation("[")
                    ) =>
            {
                self.take();
                Written::Inferred
            }
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

        // A type argument is erased. The JVM has never seen one: `List<String>`
        // and `List` are the same class at run time, and the only thing lost by
        // reading them and throwing them away is the checking `javac` does
        // before it does exactly the same thing.
        self.skip_type_arguments()?;

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

    /// Every type a file declares, in the order they were written.
    pub fn file(&mut self) -> Result<Vec<Unit>, Diagnostic> {
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

        let mut declared = Vec::new();
        while !matches!(self.here().token, Token::End) {
            if self.eat_mark(";") {
                continue;
            }
            let mut beside = Vec::new();
            let one = self.declaration_of_a_type(&package, &imports, None, &mut beside)?;
            declared.push(one);
            declared.append(&mut beside);
        }
        if declared.is_empty() {
            return Err(at(
                "EJ104",
                self.line(),
                self.column(),
                "This file declares nothing.",
            )
            .with_suggestion("A file compiled here holds a class or an interface."));
        }
        Ok(declared)
    }

    /// One `class` or `interface` in a file, which may hold more than one.
    ///
    /// `beside` collects the types declared inside this one, which are classes
    /// in their own right with a name saying where they were written.
    fn declaration_of_a_type(
        &mut self,
        package: &Option<String>,
        imports: &[String],
        inside: Option<&str>,
        beside: &mut Vec<Unit>,
    ) -> Result<Unit, Diagnostic> {
        self.skip_annotations()?;
        let modifiers = self.modifiers()?;
        // `record` is not a keyword: Java kept it a name so that code already
        // using it still compiles. It is a declaration only when a name and a
        // parameter list follow it.
        let is_record = matches!(&self.here().token, Token::Identifier(word) if word == "record")
            && matches!(self.ahead(1), Token::Identifier(_));
        let shape = if self.eat_word("interface") {
            Shape::Interface
        } else if self.eat_word("class") {
            Shape::Class
        } else if self.eat_word("enum") {
            Shape::Enum
        } else if is_record {
            self.take();
            Shape::Record
        } else {
            return Err(at(
                "EJ104",
                self.line(),
                self.column(),
                format!(
                    "A class or an interface was expected, and {} was found.",
                    self.here().token.describe()
                ),
            )
            .with_suggestion(
                "Enums and records are not compiled here yet. What is and is not taken \
                 is written at the top of Compilers/Java.rs.",
            ));
        };

        let name = match inside {
            // A class inside a class is named for where it was written, which
            // is what the JVM has always called them.
            Some(holder) => format!("{holder}${}", self.want_name()?),
            None => self.want_name()?,
        };
        self.skip_type_arguments()?;

        // A record says what it holds in front of everything else.
        let components = if shape == Shape::Record {
            self.parameters_and_shape()?.0
        } else {
            Vec::new()
        };

        // An interface's `extends` lists interfaces, which is what a class
        // calls `implements`. In the class file they are the same list.
        let mut extends = None;
        let mut implements = Vec::new();
        if self.eat_word("extends") {
            loop {
                let named = self.qualified()?;
                self.skip_type_arguments()?;
                match shape {
                    Shape::Class | Shape::Enum => extends = Some(named),
                    // A record's superclass is always `java.lang.Record`, so
                    // `extends` on one is not Java at all.
                    Shape::Interface | Shape::Record => implements.push(named),
                }
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        if self.eat_word("implements") {
            loop {
                implements.push(self.qualified()?);
                self.skip_type_arguments()?;
                if !self.eat_mark(",") {
                    break;
                }
            }
        }

        self.want_mark("{")?;

        // An enum's constants come first, before anything else it declares.
        let mut constants: Vec<Constant> = Vec::new();
        if shape == Shape::Enum {
            while matches!(self.here().token, Token::Identifier(_)) {
                self.skip_annotations()?;
                let (line, column) = (self.line(), self.column());
                let name = self.want_name()?;
                let arguments = if self.is_mark("(") {
                    self.arguments()?
                } else {
                    Vec::new()
                };
                if self.is_mark("{") {
                    return Err(unsupported(
                        self.line(),
                        self.column(),
                        "An enum constant with a body of its own",
                    ));
                }
                constants.push(Constant {
                    name,
                    arguments,
                    line,
                    column,
                });
                if !self.eat_mark(",") {
                    break;
                }
            }
            // The semicolon is only needed when something follows.
            self.eat_mark(";");
        }

        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut instance_setup = Vec::new();
        let mut static_setup = Vec::new();
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
            self.member(
                shape,
                &name,
                package,
                imports,
                &mut fields,
                &mut methods,
                &mut instance_setup,
                &mut static_setup,
                beside,
            )?;
        }
        self.want_mark("}")?;

        let unit = Unit {
            shape,
            package: package.clone(),
            imports: imports.to_vec(),
            modifiers,
            name,
            extends,
            implements,
            fields,
            methods,
            instance_setup,
            static_setup,
            outer: None,
        };
        if shape == Shape::Enum {
            return Ok(as_the_class_it_is(unit, &constants));
        }
        if shape == Shape::Record {
            return as_the_class_a_record_is(unit, &components);
        }
        // A class written inside another without `static` belongs to an
        // instance of it. An enum, a record and an interface never do, which
        // is why they are settled above.
        if let Some(holder) = inside {
            if !modifiers.static_ && shape == Shape::Class {
                return Ok(belonging_to_an_instance(unit, holder, package));
            }
        }
        Ok(unit)
    }

    #[allow(clippy::too_many_arguments)]
    fn member(
        &mut self,
        shape: Shape,
        class: &str,
        package: &Option<String>,
        imports: &[String],
        fields: &mut Vec<Field>,
        methods: &mut Vec<Method>,
        instance_setup: &mut Vec<Positioned<Statement>>,
        static_setup: &mut Vec<Positioned<Statement>>,
        beside: &mut Vec<Unit>,
    ) -> Result<(), Diagnostic> {
        let line = self.line();
        // Where this member began, so that a type declared here can be read
        // from the start by the reader that knows how.
        let began = self.at;
        self.skip_annotations()?;
        let mut modifiers = self.modifiers()?;
        if shape == Shape::Interface {
            // Every member of an interface is public; a field of one is a
            // constant. Java lets all of that go unwritten, and most code does.
            modifiers.public = true;
        }

        // A type written inside another is a class of its own, named for
        // where it was written.
        let nested = self.is_word("class")
            || self.is_word("interface")
            || self.is_word("enum")
            || (matches!(&self.here().token, Token::Identifier(word) if word == "record")
                && matches!(self.ahead(1), Token::Identifier(_)));
        if nested {
            self.at = began;
            let mut theirs = Vec::new();
            let one = self.declaration_of_a_type(package, imports, Some(class), &mut theirs)?;
            beside.push(one);
            beside.append(&mut theirs);
            return Ok(());
        }
        // `static { ... }` runs once when the class loads; a bare `{ ... }`
        // runs at the top of every constructor.
        if self.is_mark("{") {
            let block = self.braced_block()?;
            if modifiers.static_ {
                static_setup.extend(block);
            } else {
                instance_setup.extend(block);
            }
            return Ok(());
        }

        // A constructor is the class's own name followed by a parameter list.
        // The name written is the simple one, whatever the class is called in
        // a file that holds it.
        let simple = class.rsplit('$').next().unwrap_or(class);
        if matches!(&self.here().token, Token::Identifier(found) if found == simple)
            && matches!(self.ahead(1), Token::Punctuation("("))
        {
            self.take();
            let (parameters, variadic) = self.parameters_and_shape()?;
            self.throws()?;
            let body = self.method_body()?;
            methods.push(Method {
                modifiers,
                returns: Written::Void,
                name: "<init>".to_string(),
                parameters,
                body,
                constructor: true,
                variadic,
                line,
            });
            return Ok(());
        }

        // `<T> void f(...)`: the method's own type parameters, erased like
        // every other one.
        self.skip_type_arguments()?;

        let what = self.written_type()?;
        let name = self.want_name()?;

        if self.is_mark("(") {
            let (parameters, variadic) = self.parameters_and_shape()?;
            self.throws()?;
            let body = self.method_body()?;
            // A method of an interface with no body is abstract; anywhere
            // else, a method with no body is a mistake.
            if body.is_none() && !modifiers.abstract_ && shape != Shape::Interface {
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
                variadic,
                line,
            });
            return Ok(());
        }

        // One or more fields, each possibly with a value, separated by commas.
        let mut declared = Vec::new();
        let mut name = name;
        loop {
            let mut what = what.clone();
            while self.is_mark("[") && matches!(self.ahead(1), Token::Punctuation("]")) {
                // `int a[]` is the older way of writing `int[] a`, and it is
                // still Java.
                self.take();
                self.take();
                what = Written::Array(Box::new(what));
            }
            let value = if self.eat_mark("=") {
                Some(self.value_or_array()?)
            } else {
                None
            };
            declared.push((what, name, value));
            if !self.eat_mark(",") {
                break;
            }
            name = self.want_name()?;
        }
        self.want_mark(";")?;
        for (what, name, value) in declared {
            let mut modifiers = modifiers;
            if shape == Shape::Interface {
                modifiers.static_ = true;
                modifiers.final_ = true;
            }
            fields.push(Field {
                modifiers,
                what,
                name,
                value,
                line,
            });
        }
        Ok(())
    }

    /// Reads `<...>` and discards it, balanced, so that a `>>` closing two
    /// levels at once does not leave one of them open.
    ///
    /// Type arguments do not survive compilation: the JVM has never seen one.
    /// Reading them and throwing them away loses only the checking `javac`
    /// does before it erases them itself.
    fn skip_type_arguments(&mut self) -> Result<(), Diagnostic> {
        if !self.is_mark("<") {
            return Ok(());
        }
        let (line, column) = (self.line(), self.column());
        let mut depth = 0usize;
        loop {
            let closes = match &self.here().token {
                Token::Punctuation("<") => {
                    depth += 1;
                    0
                }
                Token::Punctuation(">") => 1,
                // The lexer reads `>>` as one shift, and in a type it is two
                // closing brackets.
                Token::Punctuation(">>") => 2,
                Token::Punctuation(">>>") => 3,
                Token::End => {
                    return Err(at(
                        "EJ115",
                        line,
                        column,
                        "A type argument list was opened and never closed.",
                    ))
                }
                _ => 0,
            };
            if closes > 0 {
                if closes > depth {
                    return Err(at(
                        "EJ115",
                        self.line(),
                        self.column(),
                        "A type argument list closes more levels than it opened.",
                    ));
                }
                depth -= closes;
            }
            self.take();
            if depth == 0 {
                return Ok(());
            }
        }
    }

    /// The parameters, and whether the last one was written with `...`.
    fn parameters_and_shape(&mut self) -> Result<(Vec<(Written, String)>, bool), Diagnostic> {
        let mut variadic = false;
        self.want_mark("(")?;
        let mut found = Vec::new();
        if !self.is_mark(")") {
            loop {
                self.skip_annotations()?;
                self.eat_word("final");
                let mut what = self.written_type()?;
                if self.eat_mark("...") {
                    // `int... v` is `int[] v`, with a note on the method saying
                    // it may be called without the array being written out.
                    what = Written::Array(Box::new(what));
                    variadic = true;
                }
                let name = self.want_name()?;
                found.push((what, name));
                if !self.eat_mark(",") {
                    break;
                }
            }
        }
        self.want_mark(")")?;
        Ok((found, variadic))
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
        if self.eat_word("synchronized") {
            self.want_mark("(")?;
            let on = self.expression()?;
            self.want_mark(")")?;
            let body = self.braced_block()?;
            return Ok(Statement::Synchronized { on, body });
        }

        if self.eat_word("assert") {
            let condition = self.expression()?;
            let said = if self.eat_mark(":") {
                Some(self.expression()?)
            } else {
                None
            };
            self.want_mark(";")?;
            let _ = column;
            return Ok(Statement::Assert { condition, said });
        }

        // A name and a colon in front of a statement is a label. A colon after
        // anything else at the start of a statement is not, so one token of
        // lookahead settles it.
        if matches!(self.here().token, Token::Identifier(_))
            && matches!(self.ahead(1), Token::Punctuation(":"))
        {
            let label = self.want_name()?;
            self.want_mark(":")?;
            let body = Box::new(self.statement()?);
            return Ok(Statement::Labelled { label, body });
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

        for (word, to_super) in [("super", true), ("this", false)] {
            if self.is_word(word) && matches!(self.ahead(1), Token::Punctuation("(")) {
                self.take();
                let arguments = self.arguments()?;
                self.want_mark(";")?;
                return Ok(Statement::Chain {
                    to_super,
                    arguments,
                });
            }
        }

        if self.eat_word("do") {
            let body = Box::new(self.statement()?);
            if !self.eat_word("while") {
                return Err(at(
                    "EJ109",
                    self.line(),
                    self.column(),
                    "A `do` block is followed by `while`.",
                ));
            }
            self.want_mark("(")?;
            let condition = self.expression()?;
            self.want_mark(")")?;
            self.want_mark(";")?;
            return Ok(Statement::DoWhile { body, condition });
        }

        if self.eat_word("for") {
            return self.for_statement(line, column);
        }

        if self.eat_word("switch") {
            return self.switch_statement(line, column);
        }

        if self.eat_word("try") {
            return self.try_statement(line, column);
        }

        if self.eat_word("throw") {
            let what = self.expression()?;
            self.want_mark(";")?;
            return Ok(Statement::Throw(what));
        }

        // `yield` is a name everywhere except at the head of a statement
        // followed by something that is not a call or an assignment, which is
        // how Java kept it usable as an identifier.
        if matches!(&self.here().token, Token::Identifier(word) if word == "yield")
            && !matches!(
                self.ahead(1),
                Token::Punctuation("(") | Token::Punctuation("=") | Token::Punctuation(".")
            )
        {
            self.take();
            let what = self.expression()?;
            self.want_mark(";")?;
            return Ok(Statement::Yield(what));
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
            let label = self.optional_label()?;
            self.want_mark(";")?;
            return Ok(Statement::Break(label));
        }

        if self.eat_word("continue") {
            let label = self.optional_label()?;
            self.want_mark(";")?;
            return Ok(Statement::Continue(label));
        }

        if self.is_word("final") && self.ahead(1) != &Token::End {
            self.take();
            let statement = self.declaration()?;
            self.want_mark(";")?;
            return Ok(statement);
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
        let base = self.written_type()?;
        let mut declared = Vec::new();
        loop {
            let (line, column) = (self.line(), self.column());
            let name = self.want_name()?;
            let mut what = base.clone();
            while self.is_mark("[") && matches!(self.ahead(1), Token::Punctuation("]")) {
                self.take();
                self.take();
                what = Written::Array(Box::new(what));
            }
            let value = if self.eat_mark("=") {
                Some(self.value_or_array()?)
            } else {
                None
            };
            declared.push(Positioned {
                node: Statement::Declare { what, name, value },
                line,
                column,
            });
            if !self.eat_mark(",") {
                break;
            }
        }
        if declared.len() == 1 {
            return Ok(declared.remove(0).node);
        }
        Ok(Statement::Several(declared))
    }

    fn optional_label(&mut self) -> Result<Option<String>, Diagnostic> {
        if matches!(self.here().token, Token::Identifier(_)) {
            return Ok(Some(self.want_name()?));
        }
        Ok(None)
    }

    /// A `switch`, in either the colon form or the arrow form.
    ///
    /// The two are not mixed, because Java does not allow it and because they
    /// mean different things about falling through: an arm written with `:`
    /// runs into the next one unless something stops it, and an arm written
    /// with `->` never does.
    fn switch_statement(&mut self, line: u32, column: u32) -> Result<Statement, Diagnostic> {
        self.want_mark("(")?;
        let subject = self.expression()?;
        self.want_mark(")")?;
        self.want_mark("{")?;

        let mut arms: Vec<Arm> = Vec::new();
        let mut arrow_form: Option<bool> = None;
        while !self.is_mark("}") {
            if matches!(self.here().token, Token::End) {
                return Err(at("EJ110", line, column, "A `switch` was never closed."));
            }
            let (arm_line, arm_column) = (self.line(), self.column());

            let mut labels = Vec::new();
            if self.eat_word("default") {
                // `default` takes no value, and `case a, b:` takes several.
            } else if self.eat_word("case") {
                loop {
                    labels.push(self.case_label()?);
                    if !self.eat_mark(",") {
                        break;
                    }
                }
            } else {
                return Err(at(
                    "EJ111",
                    arm_line,
                    arm_column,
                    format!(
                        "A `switch` holds `case` and `default` arms, and {} was found.",
                        self.here().token.describe()
                    ),
                ));
            }

            let arrow = if self.eat_mark("->") {
                true
            } else {
                self.want_mark(":")?;
                false
            };
            match arrow_form {
                None => arrow_form = Some(arrow),
                Some(first) if first != arrow => {
                    return Err(at(
                        "EJ112",
                        arm_line,
                        arm_column,
                        "A `switch` uses `->` for every arm or `:` for every arm, not both.",
                    ))
                }
                Some(_) => {}
            }

            let mut body = Vec::new();
            if arrow {
                // One statement, or a block, and nothing runs on into the
                // next arm.
                if self.eat_word("throw") {
                    let what = self.expression()?;
                    self.want_mark(";")?;
                    let (l, c) = (arm_line, arm_column);
                    body.push(Positioned {
                        node: Statement::Throw(what),
                        line: l,
                        column: c,
                    });
                } else {
                    body.push(self.statement()?);
                }
            } else {
                while !self.is_mark("}") && !self.is_word("case") && !self.is_word("default") {
                    if matches!(self.here().token, Token::End) {
                        return Err(at("EJ110", line, column, "A `switch` was never closed."));
                    }
                    body.push(self.statement()?);
                }
            }

            arms.push(Arm {
                labels,
                arrow,
                body,
                line: arm_line,
                column: arm_column,
            });
        }
        self.want_mark("}")?;

        if arms.iter().filter(|arm| arm.labels.is_empty()).count() > 1 {
            return Err(at(
                "EJ113",
                line,
                column,
                "A `switch` has one `default` arm at most.",
            ));
        }

        Ok(Statement::Switch { subject, arms })
    }

    /// What a `case` answers to: a number, a character, a string, or the name
    /// of a constant.
    ///
    /// Not an expression. `case EARTH ->` reads as a lambda if it is handed to
    /// the expression parser, because a name followed by an arrow is exactly
    /// what a lambda looks like.
    fn case_label(&mut self) -> Result<Expression, Diagnostic> {
        let (line, column) = (self.line(), self.column());
        let negated = self.eat_mark("-");
        let found = match self.take() {
            Token::Int(value) => Expression::Int(value),
            Token::Long(value) => Expression::Long(value),
            Token::Char(value) => Expression::Char(value),
            Token::Str(text) => Expression::Str(text),
            Token::True => Expression::Boolean(true),
            Token::False => Expression::Boolean(false),
            Token::Identifier(name) => Expression::Name(name),
            other => {
                return Err(at(
                    "EJ239",
                    line,
                    column,
                    format!(
                        "A `case` answers to a constant, and {} was found.",
                        other.describe()
                    ),
                ))
            }
        };
        if negated {
            return Ok(Expression::Unary {
                operator: Unary::Negate,
                of: Box::new(found),
            });
        }
        Ok(found)
    }

    fn try_statement(&mut self, line: u32, column: u32) -> Result<Statement, Diagnostic> {
        // `try (a; b) { ... }` holds what has to be closed afterwards. Each
        // one is a declaration, and the last may leave off its semicolon.
        let mut resources: Vec<Positioned<Statement>> = Vec::new();
        if self.eat_mark("(") {
            loop {
                if self.is_mark(")") {
                    break;
                }
                let (line, column) = (self.line(), self.column());
                self.eat_word("final");
                let node = self.declaration()?;
                let Statement::Declare { value: Some(_), .. } = &node else {
                    return Err(at(
                        "EJ118",
                        line,
                        column,
                        "Something a `try` closes has to be given a value.",
                    ));
                };
                resources.push(Positioned { node, line, column });
                if !self.eat_mark(";") {
                    break;
                }
            }
            self.want_mark(")")?;
            if resources.is_empty() {
                return Err(at(
                    "EJ118",
                    line,
                    column,
                    "A `try` with brackets holds at least one thing to close.",
                ));
            }
        }

        let body = self.braced_block()?;

        let mut catches = Vec::new();
        while self.is_word("catch") {
            let (catch_line, catch_column) = (self.line(), self.column());
            self.take();
            self.want_mark("(")?;
            self.eat_word("final");
            let mut types = vec![self.written_type()?];
            while self.eat_mark("|") {
                types.push(self.written_type()?);
            }
            let name = self.want_name()?;
            self.want_mark(")")?;
            let body = self.braced_block()?;
            catches.push(Catch {
                types,
                name,
                body,
                line: catch_line,
                column: catch_column,
            });
        }

        let finally = if self.eat_word("finally") {
            Some(self.braced_block()?)
        } else {
            None
        };

        if catches.is_empty() && finally.is_none() && resources.is_empty() {
            return Err(at(
                "EJ114",
                line,
                column,
                "A `try` needs a `catch`, a `finally`, or something to close.",
            ));
        }

        // Each resource becomes its own `try`, innermost last, so that they
        // are closed in the reverse of the order they were opened -- which is
        // what the language says and what anything holding a lock depends on.
        let mut inner = Statement::Try {
            body,
            catches,
            finally,
        };
        for resource in resources.into_iter().rev() {
            let Statement::Declare { name, .. } = &resource.node else {
                unreachable!("a resource is a declaration");
            };
            let close = Positioned {
                node: Statement::Express(Expression::Call {
                    on: Some(Box::new(Expression::Name(name.clone()))),
                    super_call: false,
                    name: "close".to_string(),
                    arguments: Vec::new(),
                }),
                line: resource.line,
                column: resource.column,
            };
            let held = Positioned {
                node: inner,
                line: resource.line,
                column: resource.column,
            };
            inner = Statement::Block(vec![
                resource,
                Positioned {
                    node: Statement::Try {
                        body: vec![held],
                        catches: Vec::new(),
                        finally: Some(vec![close]),
                    },
                    line,
                    column,
                },
            ]);
        }
        Ok(inner)
    }

    fn braced_block(&mut self) -> Result<Vec<Positioned<Statement>>, Diagnostic> {
        let (line, column) = (self.line(), self.column());
        self.want_mark("{")?;
        let mut found = Vec::new();
        while !self.is_mark("}") {
            if matches!(self.here().token, Token::End) {
                return Err(at("EJ108", line, column, "A block was never closed."));
            }
            found.push(self.statement()?);
        }
        self.want_mark("}")?;
        Ok(found)
    }

    fn for_statement(&mut self, line: u32, column: u32) -> Result<Statement, Diagnostic> {
        self.want_mark("(")?;

        // `for (Type name : thing)` is the enhanced form. Which form this is
        // cannot be told from the first token, so the header is scanned for a
        // colon that is not inside anything.
        let enhanced = {
            let mut ahead = 0usize;
            let mut depth = 0usize;
            let mut found = false;
            loop {
                match self.ahead(ahead) {
                    Token::Punctuation("(") => depth += 1,
                    Token::Punctuation(")") if depth == 0 => break,
                    Token::Punctuation(")") => depth -= 1,
                    Token::Punctuation(";") if depth == 0 => break,
                    Token::Punctuation(":") if depth == 0 => {
                        found = true;
                        break;
                    }
                    Token::End => break,
                    _ => {}
                }
                ahead += 1;
            }
            found
        };

        if enhanced {
            self.eat_word("final");
            let what = self.written_type()?;
            let name = self.want_name()?;
            self.want_mark(":")?;
            let over = self.expression()?;
            self.want_mark(")")?;
            let body = Box::new(self.statement()?);
            let _ = (line, column);
            return Ok(Statement::ForEach {
                what,
                name,
                over,
                body,
            });
        }

        let mut start = Vec::new();
        if !self.is_mark(";") {
            let (line, column) = (self.line(), self.column());
            let node = if self.is_word("final") || self.looks_like_declaration() {
                self.eat_word("final");
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
                self.eat_word("final");
                let what = self.written_type()?;
                // `o instanceof String s` names the value once the check has
                // passed, so the cast nobody wants to write is not written.
                let binds = match &self.here().token {
                    Token::Identifier(_) => Some(self.want_name()?),
                    _ => None,
                };
                left = Expression::InstanceOf {
                    of: Box::new(left),
                    what,
                    binds,
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
        let (line, _) = (self.line(), self.column());
        let mut found = self.primary()?;
        loop {
            if self.is_mark("::") {
                self.take();
                found = self.method_reference(found, line)?;
                continue;
            }
            if self.is_mark(".") {
                self.take();
                // `Foo.class` and `Outer.this` name what is in front of the
                // dot rather than reading something out of it.
                if self.is_word("class") || self.is_word("this") {
                    let Some(named) = as_a_written_type(&found) else {
                        return Err(at(
                            "EJ122",
                            self.line(),
                            self.column(),
                            "Only a class can be written in front of this.",
                        ));
                    };
                    let literal = self.is_word("class");
                    self.take();
                    found = if literal {
                        Expression::ClassLiteral { of: named, line }
                    } else {
                        Expression::OuterThis { of: named, line }
                    };
                    continue;
                }
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

    /// A type's name, with any brackets after it left where they are.
    fn type_name_only(&mut self) -> Result<Written, Diagnostic> {
        let base = match self.here().token.clone() {
            Token::Keyword(word) => match Written::of_keyword(word) {
                Some(one) => {
                    self.take();
                    one
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
            _ => Written::Named(self.qualified()?),
        };
        self.skip_type_arguments()?;
        Ok(base)
    }

    /// `{ a, b, c }`, with a trailing comma allowed because Java allows one.
    fn array_values(&mut self) -> Result<Vec<Expression>, Diagnostic> {
        self.want_mark("{")?;
        let mut found = Vec::new();
        while !self.is_mark("}") {
            found.push(self.value_or_array()?);
            if !self.eat_mark(",") {
                break;
            }
        }
        self.want_mark("}")?;
        Ok(found)
    }

    /// A value, or the values of an array written without `new` in front,
    /// which takes what it holds from wherever it is going.
    fn value_or_array(&mut self) -> Result<Expression, Diagnostic> {
        if self.is_mark("{") {
            let line = self.line();
            let values = self.array_values()?;
            return Ok(Expression::ArrayOf {
                of: None,
                values,
                line,
            });
        }
        self.expression()
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

    /// `x -> ...`, `(x, y) -> ...`, `() -> ...`, with or without written
    /// types on the parameters.
    fn lambda(&mut self, line: u32) -> Result<Expression, Diagnostic> {
        let mut parameters: Vec<(Option<Written>, String)> = Vec::new();
        if self.eat_mark("(") {
            while !self.is_mark(")") {
                self.eat_word("final");
                // `(String a)` names the type; `(a)` leaves it to the
                // interface. One token of lookahead tells them apart.
                let written = if matches!(self.here().token, Token::Identifier(_))
                    && matches!(
                        self.ahead(1),
                        Token::Punctuation(",") | Token::Punctuation(")")
                    ) {
                    None
                } else {
                    Some(self.written_type()?)
                };
                parameters.push((written, self.want_name()?));
                if !self.eat_mark(",") {
                    break;
                }
            }
            self.want_mark(")")?;
        } else {
            parameters.push((None, self.want_name()?));
        }
        self.want_mark("->")?;

        if self.is_mark("{") {
            let body = self.braced_block()?;
            return Ok(Expression::Lambda {
                parameters,
                body,
                expression: false,
                line,
            });
        }
        let (value_line, column) = (self.line(), self.column());
        let value = self.expression()?;
        Ok(Expression::Lambda {
            parameters,
            body: vec![Positioned {
                node: Statement::Express(value),
                line: value_line,
                column,
            }],
            expression: true,
            line,
        })
    }

    /// `Type::method`, `value::method` and `Type::new`, which are a lambda
    /// with the parameters left to the interface.
    fn method_reference(&mut self, on: Expression, line: u32) -> Result<Expression, Diagnostic> {
        let name = if self.eat_word("new") {
            "<init>".to_string()
        } else {
            self.want_name()?
        };
        Ok(Expression::MethodRef {
            on: Box::new(on),
            name,
            line,
        })
    }

    fn primary(&mut self) -> Result<Expression, Diagnostic> {
        let (line, column) = (self.line(), self.column());

        // `int.class`. A primitive is the one type that can start an
        // expression, and only in front of this.
        if let Token::Keyword(word) = self.here().token {
            if Written::of_keyword(word).is_some()
                && matches!(self.ahead(1), Token::Punctuation("."))
                && matches!(self.ahead(2), Token::Keyword("class"))
            {
                let of = Written::of_keyword(word).expect("just checked");
                self.take();
                self.take();
                self.take();
                return Ok(Expression::ClassLiteral { of, line });
            }
        }

        if self.is_mark("(") {
            if self.looks_like_lambda() {
                return self.lambda(line);
            }
            self.take();
            let inner = self.expression()?;
            self.want_mark(")")?;
            return Ok(inner);
        }

        // `x -> ...`: one parameter, with no brackets and no type.
        if matches!(self.here().token, Token::Identifier(_))
            && matches!(self.ahead(1), Token::Punctuation("->"))
        {
            return self.lambda(line);
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
            // The type without the brackets that may follow it, because what
            // comes after them decides whether they are a size or a list of
            // values.
            let held = self.at;
            let mut what = self.written_type()?;
            if matches!(what, Written::Array(_)) {
                self.at = held;
                what = self.type_name_only()?;
            }
            if self.is_mark("[") {
                // `new int[3]`, `new int[3][4]`, `new int[3][]`, and
                // `new int[]{ ... }`, which says the values instead of a size.
                let mut lengths = Vec::new();
                let mut empty = 0usize;
                while self.is_mark("[") {
                    self.take();
                    if self.eat_mark("]") {
                        empty += 1;
                        continue;
                    }
                    if empty > 0 {
                        return Err(at(
                            "EJ120",
                            line,
                            column,
                            "A size cannot follow an empty pair of brackets.",
                        ));
                    }
                    lengths.push(self.expression()?);
                    self.want_mark("]")?;
                }
                if lengths.is_empty() {
                    if empty == 0 {
                        return Err(at("EJ120", line, column, "An array needs a size."));
                    }
                    let mut of = what;
                    for _ in 1..empty {
                        of = Written::Array(Box::new(of));
                    }
                    let values = self.array_values()?;
                    return Ok(Expression::ArrayOf {
                        of: Some(of),
                        values,
                        line,
                    });
                }
                return Ok(Expression::NewArray {
                    of: what,
                    lengths,
                    empty,
                });
            }
            let arguments = self.arguments()?;
            if self.is_mark("{") {
                // A class with no name of its own, written where it is used.
                // What it holds is read the same way any class body is; the
                // name and the shape are settled when it is compiled.
                let mut fields = Vec::new();
                let mut methods = Vec::new();
                let mut instance_setup = Vec::new();
                let mut static_setup = Vec::new();
                self.want_mark("{")?;
                while !self.is_mark("}") {
                    if matches!(self.here().token, Token::End) {
                        return Err(at(
                            "EJ105",
                            line,
                            column,
                            "An anonymous class was opened and never closed.",
                        ));
                    }
                    if self.eat_mark(";") {
                        continue;
                    }
                    self.member(
                        Shape::Class,
                        "",
                        &None,
                        &[],
                        &mut fields,
                        &mut methods,
                        &mut instance_setup,
                        &mut static_setup,
                        &mut Vec::new(),
                    )?;
                }
                self.want_mark("}")?;
                if !static_setup.is_empty() {
                    return Err(unsupported(
                        line,
                        column,
                        "A `static` block in an anonymous class",
                    ));
                }
                return Ok(Expression::Anonymous {
                    what,
                    arguments,
                    body: Box::new(Body {
                        fields,
                        methods,
                        instance_setup,
                    }),
                    line,
                    column,
                });
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
            Token::Keyword("switch") => {
                let Statement::Switch { subject, arms } = self.switch_statement(line, column)?
                else {
                    unreachable!("a switch parses as a switch")
                };
                Ok(Expression::Switch {
                    subject: Box::new(subject),
                    arms,
                })
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
pub fn parse(source: &str) -> Result<Vec<Unit>, Diagnostic> {
    let tokens = Lexer::new(source).tokens()?;
    let mut parser = Parser::new(tokens);
    let declared = parser.file()?;
    if !matches!(parser.here().token, Token::End) {
        return Err(at(
            "EJ116",
            parser.line(),
            parser.column(),
            format!(
                "There is something after the last declaration in this file: {}.",
                parser.here().token.describe()
            ),
        ));
    }
    Ok(declared)
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
    /// Whether the last parameter was written with `...`, which means a call
    /// may hand over the elements instead of the array.
    pub variadic: bool,
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
    /// The class an instance of this one belongs to, for a class written
    /// inside another without `static`.
    pub outer: Option<String>,
}

impl Classpath {
    pub fn new() -> Classpath {
        Classpath::default()
    }

    /// Adds what a type declared in this compilation says about itself,
    /// before it has been compiled.
    ///
    /// Two types written in one file can name each other, and neither exists
    /// as a class file yet. What is recorded here is only what the other one
    /// needs: the name, the shape, the fields and the signatures. Types inside
    /// them that do not resolve are found when that type is compiled in its
    /// own right, so nothing is waved through.
    pub fn shell(&mut self, unit: &Unit) {
        let internal = unit.internal_name();
        self.known.insert(
            internal.clone(),
            KnownClass {
                name: internal,
                superclass: Some(match &unit.extends {
                    Some(named) => named.replace('.', "/"),
                    None => "java/lang/Object".to_string(),
                }),
                interface: unit.shape == Shape::Interface,
                outer: unit.outer.clone(),
                ..KnownClass::default()
            },
        );
    }

    /// Fills in what a declared type's members look like.
    ///
    /// Every type in the file has a shell first, so that this can resolve a
    /// name pointing at one of the others whichever order they were written
    /// in. A member whose type does not resolve is left out rather than
    /// recorded as a guess: a descriptor built from a name that stands for
    /// nothing is a field the device cannot find.
    pub fn declare(&mut self, unit: &Unit) {
        let internal = unit.internal_name();
        let mut known = match self.known.get(&internal) {
            Some(found) => found.clone(),
            None => KnownClass {
                name: internal.clone(),
                superclass: Some("java/lang/Object".to_string()),
                interface: unit.shape == Shape::Interface,
                outer: unit.outer.clone(),
                ..KnownClass::default()
            },
        };

        let shallow = |written: &Written| -> Option<Type> {
            fn walk(classpath: &Classpath, unit: &Unit, written: &Written) -> Option<Type> {
                Some(match written {
                    Written::Void => Type::Void,
                    Written::Boolean => Type::Boolean,
                    Written::Byte => Type::Byte,
                    Written::Short => Type::Short,
                    Written::Char => Type::Char,
                    Written::Int => Type::Int,
                    Written::Long => Type::Long,
                    Written::Float => Type::Float,
                    Written::Double => Type::Double,
                    Written::Array(of) => Type::Array(Box::new(walk(classpath, unit, of)?)),
                    Written::Named(name) => Type::Object(resolve_named(classpath, unit, name)?),
                    Written::Inferred => return None,
                })
            }
            walk(self, unit, written)
        };

        for field in &unit.fields {
            if let Some(what) = shallow(&field.what) {
                known
                    .fields
                    .push((field.name.clone(), what, field.modifiers.static_));
            }
        }
        for method in &unit.methods {
            let mut parameters = Vec::new();
            let mut whole = true;
            for (what, _) in &method.parameters {
                match shallow(what) {
                    Some(found) => parameters.push(found),
                    None => whole = false,
                }
            }
            let Some(returns) = shallow(&method.returns) else {
                continue;
            };
            if !whole {
                continue;
            }
            known.methods.push(Signature {
                owner: internal.clone(),
                name: method.name.clone(),
                parameters,
                returns,
                static_: method.modifiers.static_,
                interface: unit.shape == Shape::Interface,
                variadic: false,
            });
        }
        self.known.insert(internal, known);
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
                // ACC_VARARGS, which is the only record a class file keeps of
                // the `...` somebody wrote.
                variadic: method.access_flags & 0x0080 != 0,
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

    /// A class and everything it inherits from, as far as this can see.
    ///
    /// The walk stops where the classpath does, and `java.lang.Object` is
    /// always last, because everything answers its methods whether or not
    /// anybody handed its class file over.
    pub fn ancestors(&self, owner: &str) -> Vec<String> {
        let mut out = vec![owner.to_string()];
        let mut at = self
            .known
            .get(owner)
            .and_then(|held| held.superclass.clone());
        while let Some(current) = at {
            if out.contains(&current) || out.len() > 64 {
                break;
            }
            out.push(current.clone());
            at = self
                .known
                .get(&current)
                .and_then(|held| held.superclass.clone());
        }
        if !out.iter().any(|held| held == "java/lang/Object") {
            out.push("java/lang/Object".to_string());
        }
        out
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

/// What the verifier is told is in a slot.
///
/// Not the same as [`Type`]: the verifier does not distinguish a boolean from
/// an int, because the machine does not. It does distinguish the second half of
/// a long from anything else, which is what `Top` is for here.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verified {
    Top,
    Integer,
    Float,
    Double,
    Long,
    Null,
    /// `this`, inside a constructor, before the superclass constructor has run.
    UninitializedThis,
    /// What `new` leaves behind: an object whose constructor has not run yet,
    /// named by where the `new` is rather than by its class. Until the
    /// constructor runs it is assignable to nothing, so a frame written
    /// between the two -- `new Foo(c ? a : b)` -- has to say this and not the
    /// class.
    Uninitialized(u16),
    Object(String),
}

impl Verified {
    fn of(what: &Type) -> Verified {
        match what {
            Type::Boolean | Type::Byte | Type::Short | Type::Char | Type::Int => Verified::Integer,
            Type::Long => Verified::Long,
            Type::Float => Verified::Float,
            Type::Double => Verified::Double,
            Type::Void => Verified::Top,
            Type::Object(name) => Verified::Object(name.clone()),
            Type::Array(_) => Verified::Object(what.descriptor()),
        }
    }

    fn is_wide(&self) -> bool {
        matches!(self, Verified::Long | Verified::Double)
    }

    fn write(&self, out: &mut Vec<u8>, pool: &mut Pool) {
        match self {
            Verified::Top => out.push(0),
            Verified::Integer => out.push(1),
            Verified::Float => out.push(2),
            Verified::Double => out.push(3),
            Verified::Long => out.push(4),
            Verified::Null => out.push(5),
            Verified::UninitializedThis => out.push(6),
            Verified::Uninitialized(at) => {
                out.push(8);
                out.extend_from_slice(&at.to_be_bytes());
            }
            Verified::Object(name) => {
                out.push(7);
                let index = pool.class(name);
                out.extend_from_slice(&index.to_be_bytes());
            }
        }
    }
}

/// One loop or switch, and where the jumps out of it are waiting to be told
/// where they land.
struct Level {
    /// The name in front of it, when it was written with one.
    label: Option<String>,
    /// True for a loop, which is the only thing `continue` can reach.
    loops: bool,
    breaks: Vec<Pending>,
    continues: Vec<Pending>,
    /// How many `finally` bodies were pending when this was entered, so that
    /// a jump out of it knows how many of them to run.
    finallys: usize,
}

/// One `switch` expression being written.
struct Yielding {
    /// The jumps out of each arm, waiting for the end of the switch.
    pending: Vec<Pending>,
    /// What the first arm produced, which is what all of them must produce.
    produced: Option<Type>,
    /// How many `finally` bodies were pending when the switch was entered.
    finallys: usize,
}

/// One row of a method's exception table.
struct Handler {
    start: usize,
    end: usize,
    target: usize,
    /// The class caught, or `None` for the handler that catches everything so
    /// that a `finally` runs on the way past.
    class: Option<String>,
}

/// What the verifier has to be told at one place a branch can land.
#[derive(Clone, Debug)]
struct Frame {
    at: usize,
    locals: Vec<Verified>,
    stack: Vec<Verified>,
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
    /// The loops and switches this is inside, innermost last.
    levels: Vec<Level>,
    /// The `finally` bodies that have to run before control leaves where it
    /// is, innermost last. A `return`, a `break` or a `continue` that jumps
    /// past one of these runs it on the way out.
    finallys: Vec<Vec<Positioned<Statement>>>,
    /// The exception table this method comes to.
    handlers: Vec<Handler>,
    /// Every stretch of code that is an inlined `finally`, and the outermost
    /// `finally` that stretch ran.
    ///
    /// A handler that protects a `try` must not protect the copy of that
    /// `try`'s own `finally` that a `return` inside it left behind, or a throw
    /// from the `finally` would run the `finally` a second time and swallow
    /// the exception in its own cleanup. It must still protect a copy of some
    /// *inner* `try`'s `finally`, because that one really is inside it -- which
    /// is why the depth is recorded alongside the range.
    inlined: Vec<(usize, usize, usize)>,
    /// A label read but not yet handed to the loop or switch it belongs to.
    pending_label: Option<String>,
    /// The `switch` expressions being written, innermost last, and where each
    /// one's arms jump to once they have their value.
    yields: Vec<Yielding>,
    /// The classes this method's body turned out to need, which are compiled
    /// after it. A class written where it is used has no name until here.
    made: &'a mut Vec<Unit>,
    /// Whether anything in this method wrote an `assert`, which is what
    /// decides whether the class needs the flag they are guarded by.
    wants_assertions: bool,
    /// The type the expression about to be written is being handed to, when
    /// that is known. A lambda has no type of its own: which interface it is
    /// depends entirely on where it is going.
    expecting: Option<Type>,
    static_: bool,
    /// What this method said it returns. A `return` is checked against it, and
    /// the first version of this did not do that -- so `int f() { return
    /// "text"; }` compiled, and produced a class file whose verifier would
    /// have thrown it out on the device.
    returns: Type,
    /// What is in each local slot, as the verifier would say it.
    ///
    /// A local that has been declared but never given a value is `Top`, which
    /// is the verifier's word for "nothing you may read".
    slots: Vec<Verified>,
    /// Everywhere a branch can land, and what is true there.
    frames: Vec<Frame>,
    /// What is on the operand stack, as the verifier would say it.
    ///
    /// Counting it was not enough. A branch inside an expression -- a `?:` in
    /// the middle of a call's arguments, say -- lands somewhere that already
    /// has the earlier arguments on the stack, and a frame that says the stack
    /// is empty there is a claim the verifier checks and rejects.
    stack: Vec<Verified>,
}

impl<'a> Emitter<'a> {
    fn new(
        pool: &'a mut Pool,
        classpath: &'a Classpath,
        unit: &'a Unit,
        this_class: String,
        static_: bool,
        made: &'a mut Vec<Unit>,
    ) -> Emitter<'a> {
        Emitter {
            made,
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
            levels: Vec::new(),
            finallys: Vec::new(),
            handlers: Vec::new(),
            inlined: Vec::new(),
            pending_label: None,
            yields: Vec::new(),
            wants_assertions: false,
            expecting: None,
            static_,
            returns: Type::Void,
            slots: Vec::new(),
            frames: Vec::new(),
            stack: Vec::new(),
        }
    }

    // -- the stack, tracked as it goes, because a class file has to declare
    // -- how deep it gets and getting that wrong is a class that will not load.

    /// Puts a value of this type on the stack.
    fn pushes(&mut self, what: &Type) {
        // A method that returns nothing leaves nothing. Writing a `top` for it
        // would describe a stack one slot deeper than the one that is there,
        // and every frame after the call would say so.
        if *what == Type::Void {
            return;
        }
        let verified = Verified::of(what);
        if verified.is_wide() {
            self.stack.push(verified);
            self.stack.push(Verified::Top);
        } else {
            self.stack.push(verified);
        }
        self.deepest();
    }

    /// Puts something on the stack whose type the verifier knows by name
    /// rather than by the language: `null`, or a `this` not yet constructed.
    fn pushes_raw(&mut self, what: Verified) {
        let wide = what.is_wide();
        self.stack.push(what);
        if wide {
            self.stack.push(Verified::Top);
        }
        self.deepest();
    }

    /// `dup`, which is whatever is on top a second time.
    fn duplicates(&mut self) {
        self.op(0x59);
        if let Some(top) = self.stack.last().cloned() {
            self.stack.push(top);
        }
        self.deepest();
    }

    /// `dup2`, which is the top two slots again.
    fn duplicates_two(&mut self) {
        self.op(0x5c);
        let held = self.stack.len();
        if held >= 2 {
            let two = self.stack[held - 2..].to_vec();
            self.stack.extend(two);
        }
        self.deepest();
    }

    /// Takes this many slots off.
    fn pops(&mut self, slots: i32) {
        for _ in 0..slots.max(0) {
            self.stack.pop();
        }
    }

    /// Says the stack is exactly this, which is what entering an exception
    /// handler does however deep it was before.
    fn stack_is(&mut self, what: Vec<Verified>) {
        self.stack = what;
        self.deepest();
    }

    fn deepest(&mut self) {
        self.depth = self.stack.len() as i32;
        if self.depth > self.max_depth {
            self.max_depth = self.depth;
        }
    }

    /// Records what is in a local slot, for the frames to report.
    fn slot_holds(&mut self, slot: u16, what: &Type) {
        let verified = Verified::of(what);
        let wide = verified.is_wide();
        let at = usize::from(slot);
        while self.slots.len() <= at + usize::from(wide) {
            self.slots.push(Verified::Top);
        }
        self.slots[at] = verified;
        if wide {
            self.slots[at + 1] = Verified::Top;
        }
    }

    /// Notes that a branch can land at the current position, and what is on the
    /// stack when it does.
    ///
    /// Every one of these becomes a frame. The verifier does not work out what
    /// is true at a branch target; it is told, and it checks that every path
    /// arriving agrees with what it was told. So a frame that is wrong is worse
    /// than no frame at all, because it is a claim rather than a gap.
    ///
    /// The locals are read from what has been declared. The stack is passed in,
    /// because every place a branch lands here is a place this compiler wrote
    /// the branch and therefore knows exactly what is left on it -- and that is
    /// a shorter road to being right than threading a type through every
    /// arithmetic instruction and hoping the two counts never drift apart.
    fn a_branch_lands_here(&mut self) {
        let at = self.code.len();
        let mut locals = Self::as_a_frame_says_it(&self.slots);
        while matches!(locals.last(), Some(Verified::Top)) {
            locals.pop();
        }
        let frame = Frame {
            at,
            locals,
            stack: Self::as_a_frame_says_it(&self.stack),
        };
        match self.frames.iter().position(|held| held.at == at) {
            // Two branches landing on one instruction have to agree, and where
            // this compiler writes them they do: one is the jump over an else
            // and the other is the end of the then. Keeping the wider of the
            // two stacks would be inventing something neither said.
            Some(index) => self.frames[index] = frame,
            None => self.frames.push(frame),
        }
    }

    /// The same slots, written the way a frame writes them.
    ///
    /// This compiler keeps a long or a double as the two slots it really
    /// occupies, because that is what `max_stack` and the local numbering are
    /// counted in. A frame says it once: one `verification_type_info` covers
    /// both halves, and writing the second half as a `top` of its own would
    /// describe a stack one slot deeper than the one that is there.
    fn as_a_frame_says_it(held: &[Verified]) -> Vec<Verified> {
        let mut said = Vec::with_capacity(held.len());
        let mut at = 0;
        while at < held.len() {
            let one = held[at].clone();
            let wide = one.is_wide();
            said.push(one);
            at += if wide && held.get(at + 1) == Some(&Verified::Top) {
                2
            } else {
                1
            };
        }
        said
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
        let to = self.code.len();
        self.land_at(pending, to);
    }

    /// Where a jump goes, when that is somewhere other than here.
    fn land_at(&mut self, pending: Pending, to: usize) {
        let offset = (to as i64 - pending.from as i64) as i16;
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
        // A local that has gone out of scope is nothing the verifier may read,
        // and a frame written after this must not claim otherwise.
        self.slots.truncate(usize::from(self.next_slot));
    }

    fn declare(&mut self, name: &str, what: Type) -> u16 {
        let slot = self.next_slot;
        self.slot_holds(slot, &what);
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
            Written::Inferred => {
                return Err(at(
                    "EJ234",
                    line,
                    1,
                    "A `var` takes its type from the value given to it, and there is none here.",
                )
                .with_suggestion("Write the type, or give it a value."))
            }
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
        resolve_named(self.classpath, self.unit, name).ok_or_else(|| {
            at(
                "EJ200",
                line,
                1,
                format!("`{name}` is not a type this compilation knows."),
            )
            .with_suggestion(
                "Import it, write it out in full, or hand the class file that declares it \
                 over as a dependency. Nothing is guessed: a name that resolves to nothing \
                 here would become a class file the device refuses.",
            )
        })
    }
}

/// Signatures from the runtime library that this compiler knows without being
/// handed a class file.
///
/// This is not a model of the JDK, and it is not trying to become one. It is
/// the handful of things ordinary Java touches in almost every method -- the
/// exceptions people throw, printing, string work, boxing, arithmetic -- and
/// without them a compiler that has never been given `android.jar` cannot
/// compile `throw new IllegalStateException("...")`. The implementation comes
/// from the device at run time; what is wanted here is only the descriptor,
/// because the descriptor is what the constant pool records.
///
/// Anything not here still works the moment the class file that declares it is
/// handed over as a dependency, which is the way anything outside this list is
/// meant to arrive.
const BUILT_IN_METHODS: &[(&str, &str, &str, bool)] = &[
    // -- Object
    ("java/lang/Object", "<init>", "()V", false),
    (
        "java/lang/Object",
        "toString",
        "()Ljava/lang/String;",
        false,
    ),
    ("java/lang/Object", "hashCode", "()I", false),
    ("java/lang/Object", "equals", "(Ljava/lang/Object;)Z", false),
    ("java/lang/Object", "getClass", "()Ljava/lang/Class;", false),
    ("java/lang/Class", "desiredAssertionStatus", "()Z", false),
    ("java/lang/Class", "getName", "()Ljava/lang/String;", false),
    (
        "java/lang/Class",
        "getSimpleName",
        "()Ljava/lang/String;",
        false,
    ),
    // -- Enum, which every enum extends
    ("java/lang/Enum", "<init>", "(Ljava/lang/String;I)V", false),
    ("java/lang/Enum", "name", "()Ljava/lang/String;", false),
    ("java/lang/Enum", "ordinal", "()I", false),
    ("java/lang/Enum", "toString", "()Ljava/lang/String;", false),
    ("java/lang/Enum", "equals", "(Ljava/lang/Object;)Z", false),
    ("java/lang/Enum", "hashCode", "()I", false),
    // -- Throwable, and the exceptions that get thrown by hand
    (
        "java/lang/Throwable",
        "getMessage",
        "()Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/Throwable",
        "getLocalizedMessage",
        "()Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/Throwable",
        "getCause",
        "()Ljava/lang/Throwable;",
        false,
    ),
    ("java/lang/Throwable", "printStackTrace", "()V", false),
    (
        "java/lang/Throwable",
        "toString",
        "()Ljava/lang/String;",
        false,
    ),
    // -- String
    ("java/lang/String", "length", "()I", false),
    ("java/lang/String", "isEmpty", "()Z", false),
    ("java/lang/String", "isBlank", "()Z", false),
    ("java/lang/String", "charAt", "(I)C", false),
    (
        "java/lang/String",
        "indexOf",
        "(Ljava/lang/String;)I",
        false,
    ),
    (
        "java/lang/String",
        "lastIndexOf",
        "(Ljava/lang/String;)I",
        false,
    ),
    (
        "java/lang/String",
        "substring",
        "(I)Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/String",
        "substring",
        "(II)Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/String",
        "concat",
        "(Ljava/lang/String;)Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/String",
        "contains",
        "(Ljava/lang/CharSequence;)Z",
        false,
    ),
    (
        "java/lang/String",
        "startsWith",
        "(Ljava/lang/String;)Z",
        false,
    ),
    (
        "java/lang/String",
        "endsWith",
        "(Ljava/lang/String;)Z",
        false,
    ),
    (
        "java/lang/String",
        "equalsIgnoreCase",
        "(Ljava/lang/String;)Z",
        false,
    ),
    (
        "java/lang/String",
        "compareTo",
        "(Ljava/lang/String;)I",
        false,
    ),
    (
        "java/lang/String",
        "replace",
        "(CC)Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/String",
        "toUpperCase",
        "()Ljava/lang/String;",
        false,
    ),
    (
        "java/lang/String",
        "toLowerCase",
        "()Ljava/lang/String;",
        false,
    ),
    ("java/lang/String", "trim", "()Ljava/lang/String;", false),
    ("java/lang/String", "strip", "()Ljava/lang/String;", false),
    ("java/lang/String", "repeat", "(I)Ljava/lang/String;", false),
    (
        "java/lang/String",
        "split",
        "(Ljava/lang/String;)[Ljava/lang/String;",
        false,
    ),
    ("java/lang/String", "toCharArray", "()[C", false),
    ("java/lang/String", "<init>", "()V", false),
    ("java/lang/String", "<init>", "(Ljava/lang/String;)V", false),
    ("java/lang/String", "<init>", "([C)V", false),
    ("java/lang/String", "<init>", "([B)V", false),
    ("java/lang/String", "valueOf", "(I)Ljava/lang/String;", true),
    ("java/lang/String", "valueOf", "(J)Ljava/lang/String;", true),
    ("java/lang/String", "valueOf", "(D)Ljava/lang/String;", true),
    ("java/lang/String", "valueOf", "(Z)Ljava/lang/String;", true),
    ("java/lang/String", "valueOf", "(C)Ljava/lang/String;", true),
    (
        "java/lang/String",
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
        true,
    ),
    (
        "java/lang/String",
        "join",
        "(Ljava/lang/CharSequence;[Ljava/lang/CharSequence;)Ljava/lang/String;",
        true,
    ),
    (
        "java/util/Arrays",
        "asList",
        "([Ljava/lang/Object;)Ljava/util/List;",
        true,
    ),
    (
        "java/util/Arrays",
        "toString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        true,
    ),
    ("java/util/Arrays", "sort", "([I)V", true),
    ("java/util/Arrays", "fill", "([II)V", true),
    (
        "java/lang/String",
        "valueOf",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        true,
    ),
    // -- StringBuilder, which is also what `+` on strings comes to
    ("java/lang/StringBuilder", "<init>", "()V", false),
    (
        "java/lang/StringBuilder",
        "<init>",
        "(Ljava/lang/String;)V",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(I)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(J)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(F)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(D)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(C)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "append",
        "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        false,
    ),
    (
        "java/lang/StringBuilder",
        "toString",
        "()Ljava/lang/String;",
        false,
    ),
    ("java/lang/StringBuilder", "length", "()I", false),
    // -- boxing, and reading numbers out of text
    (
        "java/lang/Integer",
        "parseInt",
        "(Ljava/lang/String;)I",
        true,
    ),
    (
        "java/lang/Integer",
        "valueOf",
        "(I)Ljava/lang/Integer;",
        true,
    ),
    (
        "java/lang/Integer",
        "toString",
        "(I)Ljava/lang/String;",
        true,
    ),
    ("java/lang/Integer", "intValue", "()I", false),
    ("java/lang/Long", "parseLong", "(Ljava/lang/String;)J", true),
    ("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;", true),
    ("java/lang/Long", "toString", "(J)Ljava/lang/String;", true),
    ("java/lang/Long", "longValue", "()J", false),
    (
        "java/lang/Double",
        "parseDouble",
        "(Ljava/lang/String;)D",
        true,
    ),
    ("java/lang/Double", "valueOf", "(D)Ljava/lang/Double;", true),
    ("java/lang/Double", "doubleValue", "()D", false),
    (
        "java/lang/Float",
        "parseFloat",
        "(Ljava/lang/String;)F",
        true,
    ),
    ("java/lang/Float", "valueOf", "(F)Ljava/lang/Float;", true),
    ("java/lang/Float", "floatValue", "()F", false),
    (
        "java/lang/Boolean",
        "parseBoolean",
        "(Ljava/lang/String;)Z",
        true,
    ),
    (
        "java/lang/Boolean",
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        true,
    ),
    ("java/lang/Boolean", "booleanValue", "()Z", false),
    (
        "java/lang/Character",
        "valueOf",
        "(C)Ljava/lang/Character;",
        true,
    ),
    ("java/lang/Character", "charValue", "()C", false),
    ("java/lang/Character", "isDigit", "(C)Z", true),
    ("java/lang/Character", "isLetter", "(C)Z", true),
    ("java/lang/Character", "isWhitespace", "(C)Z", true),
    ("java/lang/Character", "toUpperCase", "(C)C", true),
    ("java/lang/Character", "toLowerCase", "(C)C", true),
    // -- arithmetic
    ("java/lang/Math", "abs", "(I)I", true),
    ("java/lang/Math", "abs", "(J)J", true),
    ("java/lang/Math", "abs", "(F)F", true),
    ("java/lang/Math", "abs", "(D)D", true),
    ("java/lang/Math", "max", "(II)I", true),
    ("java/lang/Math", "max", "(JJ)J", true),
    ("java/lang/Math", "max", "(DD)D", true),
    ("java/lang/Math", "min", "(II)I", true),
    ("java/lang/Math", "min", "(JJ)J", true),
    ("java/lang/Math", "min", "(DD)D", true),
    ("java/lang/Math", "sqrt", "(D)D", true),
    ("java/lang/Math", "pow", "(DD)D", true),
    ("java/lang/Math", "floor", "(D)D", true),
    ("java/lang/Math", "ceil", "(D)D", true),
    ("java/lang/Math", "round", "(D)J", true),
    ("java/lang/Math", "random", "()D", true),
    // -- the collections, which is what most Java holds things in. Every one
    // of these is an interface at run time except the two classes named, and
    // the erased signatures are what `javac` writes after erasure too.
    (
        "java/lang/Iterable",
        "iterator",
        "()Ljava/util/Iterator;",
        false,
    ),
    ("java/util/Iterator", "hasNext", "()Z", false),
    ("java/util/Iterator", "next", "()Ljava/lang/Object;", false),
    ("java/util/Collection", "size", "()I", false),
    ("java/util/Collection", "isEmpty", "()Z", false),
    ("java/util/Collection", "clear", "()V", false),
    (
        "java/util/Collection",
        "add",
        "(Ljava/lang/Object;)Z",
        false,
    ),
    (
        "java/util/Collection",
        "remove",
        "(Ljava/lang/Object;)Z",
        false,
    ),
    (
        "java/util/Collection",
        "contains",
        "(Ljava/lang/Object;)Z",
        false,
    ),
    (
        "java/util/Collection",
        "iterator",
        "()Ljava/util/Iterator;",
        false,
    ),
    ("java/util/List", "get", "(I)Ljava/lang/Object;", false),
    (
        "java/util/List",
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        false,
    ),
    ("java/util/List", "add", "(ILjava/lang/Object;)V", false),
    ("java/util/List", "indexOf", "(Ljava/lang/Object;)I", false),
    ("java/util/List", "remove", "(I)Ljava/lang/Object;", false),
    ("java/util/ArrayList", "<init>", "()V", false),
    ("java/util/ArrayList", "<init>", "(I)V", false),
    ("java/util/Map", "size", "()I", false),
    ("java/util/Map", "isEmpty", "()Z", false),
    ("java/util/Map", "clear", "()V", false),
    (
        "java/util/Map",
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        false,
    ),
    (
        "java/util/Map",
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        false,
    ),
    (
        "java/util/Map",
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        false,
    ),
    (
        "java/util/Map",
        "containsKey",
        "(Ljava/lang/Object;)Z",
        false,
    ),
    ("java/util/Map", "keySet", "()Ljava/util/Set;", false),
    ("java/util/Map", "values", "()Ljava/util/Collection;", false),
    ("java/util/HashMap", "<init>", "()V", false),
    (
        "java/util/Objects",
        "requireNonNull",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        true,
    ),
    (
        "java/util/Objects",
        "equals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        true,
    ),
    (
        "java/util/Objects",
        "hashCode",
        "(Ljava/lang/Object;)I",
        true,
    ),
    (
        "java/util/Objects",
        "toString",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        true,
    ),
    ("java/lang/AutoCloseable", "close", "()V", false),
    ("java/lang/AssertionError", "<init>", "()V", false),
    (
        "java/lang/AssertionError",
        "<init>",
        "(Ljava/lang/Object;)V",
        false,
    ),
    // -- Android, as far as an application's own code touches it.
    //
    // There is no `android.jar` on a phone and there never will be, so the
    // signatures an application needs to name the platform are here. The
    // platform provides the implementation; what is wanted at build time is
    // only the descriptor, because the descriptor is what the dex records and
    // what the runtime resolves against.
    (
        "android/content/Context",
        "getString",
        "(I)Ljava/lang/String;",
        false,
    ),
    (
        "android/content/Context",
        "getPackageName",
        "()Ljava/lang/String;",
        false,
    ),
    (
        "android/content/Context",
        "startActivity",
        "(Landroid/content/Intent;)V",
        false,
    ),
    ("android/content/Intent", "<init>", "()V", false),
    (
        "android/content/Intent",
        "<init>",
        "(Ljava/lang/String;)V",
        false,
    ),
    (
        "android/content/Intent",
        "<init>",
        "(Landroid/content/Context;Ljava/lang/Class;)V",
        false,
    ),
    (
        "android/content/Intent",
        "putExtra",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
        false,
    ),
    (
        "android/content/Intent",
        "getStringExtra",
        "(Ljava/lang/String;)Ljava/lang/String;",
        false,
    ),
    ("android/app/Activity", "<init>", "()V", false),
    (
        "android/app/Activity",
        "onCreate",
        "(Landroid/os/Bundle;)V",
        false,
    ),
    ("android/app/Activity", "onStart", "()V", false),
    ("android/app/Activity", "onResume", "()V", false),
    ("android/app/Activity", "onPause", "()V", false),
    ("android/app/Activity", "onStop", "()V", false),
    ("android/app/Activity", "onDestroy", "()V", false),
    ("android/app/Activity", "setContentView", "(I)V", false),
    (
        "android/app/Activity",
        "setContentView",
        "(Landroid/view/View;)V",
        false,
    ),
    (
        "android/app/Activity",
        "findViewById",
        "(I)Landroid/view/View;",
        false,
    ),
    ("android/app/Activity", "finish", "()V", false),
    (
        "android/app/Activity",
        "setTitle",
        "(Ljava/lang/CharSequence;)V",
        false,
    ),
    ("android/os/Bundle", "<init>", "()V", false),
    (
        "android/os/Bundle",
        "getInt",
        "(Ljava/lang/String;)I",
        false,
    ),
    (
        "android/os/Bundle",
        "putInt",
        "(Ljava/lang/String;I)V",
        false,
    ),
    (
        "android/os/Bundle",
        "getString",
        "(Ljava/lang/String;)Ljava/lang/String;",
        false,
    ),
    (
        "android/os/Bundle",
        "putString",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        false,
    ),
    ("android/view/View", "getId", "()I", false),
    ("android/view/View", "setVisibility", "(I)V", false),
    ("android/view/View", "getVisibility", "()I", false),
    ("android/view/View", "setEnabled", "(Z)V", false),
    (
        "android/view/View",
        "setOnClickListener",
        "(Landroid/view/View$OnClickListener;)V",
        false,
    ),
    (
        "android/view/View$OnClickListener",
        "onClick",
        "(Landroid/view/View;)V",
        false,
    ),
    ("java/lang/Runnable", "run", "()V", false),
    (
        "android/widget/TextView",
        "setText",
        "(Ljava/lang/CharSequence;)V",
        false,
    ),
    (
        "android/widget/TextView",
        "getText",
        "()Ljava/lang/CharSequence;",
        false,
    ),
    ("android/widget/TextView", "setTextSize", "(F)V", false),
    (
        "android/widget/Button",
        "setText",
        "(Ljava/lang/CharSequence;)V",
        false,
    ),
    (
        "android/widget/LinearLayout",
        "<init>",
        "(Landroid/content/Context;)V",
        false,
    ),
    (
        "android/widget/LinearLayout",
        "setOrientation",
        "(I)V",
        false,
    ),
    (
        "android/widget/LinearLayout",
        "addView",
        "(Landroid/view/View;)V",
        false,
    ),
    (
        "android/widget/TextView",
        "<init>",
        "(Landroid/content/Context;)V",
        false,
    ),
    (
        "android/widget/Button",
        "<init>",
        "(Landroid/content/Context;)V",
        false,
    ),
    (
        "android/widget/Toast",
        "makeText",
        "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;",
        true,
    ),
    ("android/widget/Toast", "show", "()V", false),
    (
        "android/util/Log",
        "d",
        "(Ljava/lang/String;Ljava/lang/String;)I",
        true,
    ),
    (
        "android/util/Log",
        "i",
        "(Ljava/lang/String;Ljava/lang/String;)I",
        true,
    ),
    (
        "android/util/Log",
        "w",
        "(Ljava/lang/String;Ljava/lang/String;)I",
        true,
    ),
    (
        "android/util/Log",
        "e",
        "(Ljava/lang/String;Ljava/lang/String;)I",
        true,
    ),
    // -- the clock, and somewhere to print
    ("java/lang/System", "currentTimeMillis", "()J", true),
    ("java/lang/System", "nanoTime", "()J", true),
    ("java/io/PrintStream", "println", "()V", false),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/String;)V",
        false,
    ),
    ("java/io/PrintStream", "println", "(I)V", false),
    ("java/io/PrintStream", "println", "(J)V", false),
    ("java/io/PrintStream", "println", "(D)V", false),
    ("java/io/PrintStream", "println", "(Z)V", false),
    ("java/io/PrintStream", "println", "(C)V", false),
    (
        "java/io/PrintStream",
        "println",
        "(Ljava/lang/Object;)V",
        false,
    ),
    (
        "java/io/PrintStream",
        "print",
        "(Ljava/lang/String;)V",
        false,
    ),
    ("java/io/PrintStream", "print", "(I)V", false),
];

/// What each built-in class inherits from, where the classpath cannot say.
///
/// A `List` answers `Collection`'s methods and a `Collection` answers
/// `Iterable`'s, and none of those class files has been handed over. Without
/// this, walking a list with a `for` is refused for want of an `iterator()`
/// that is two steps up.
const BUILT_IN_ABOVE: &[(&str, &str)] = &[
    ("java/lang/String", "java/lang/CharSequence"),
    ("android/app/Activity", "android/content/Context"),
    ("android/widget/TextView", "android/view/View"),
    ("android/widget/Button", "android/widget/TextView"),
    ("android/widget/LinearLayout", "android/view/View"),
    ("java/util/Collection", "java/lang/Iterable"),
    ("java/util/List", "java/util/Collection"),
    ("java/util/Set", "java/util/Collection"),
    ("java/util/ArrayList", "java/util/List"),
    ("java/util/HashMap", "java/util/Map"),
    ("java/lang/Integer", "java/lang/Number"),
    ("java/lang/Long", "java/lang/Number"),
    ("java/lang/Double", "java/lang/Number"),
    ("java/lang/Float", "java/lang/Number"),
    ("java/lang/Short", "java/lang/Number"),
    ("java/lang/Byte", "java/lang/Number"),
];

/// The built-in methods whose last parameter was written with `...`, so that a
/// call may hand over the elements instead of the array.
const BUILT_IN_VARIADIC: &[(&str, &str)] = &[
    ("java/lang/String", "format"),
    ("java/lang/String", "join"),
    ("java/util/Arrays", "asList"),
    ("java/util/Arrays", "toString"),
];

/// Which of the built-in classes are interfaces, which decides whether a call
/// on one is `invokevirtual` or `invokeinterface`. Getting it wrong produces a
/// class file that verifies and then fails to link on the device.
const BUILT_IN_INTERFACES: &[&str] = &[
    "android/view/View$OnClickListener",
    "java/lang/Runnable",
    "java/lang/CharSequence",
    "java/lang/Iterable",
    "java/lang/AutoCloseable",
    "java/util/Iterator",
    "java/util/Collection",
    "java/util/List",
    "java/util/Set",
    "java/util/Map",
];

/// The exceptions this compiler knows how to construct without being handed
/// their class files. Each takes nothing or a message, which is how they are
/// almost always thrown.
const BUILT_IN_THROWABLES: &[&str] = &[
    "java/lang/AssertionError",
    "java/lang/Throwable",
    "java/lang/Exception",
    "java/lang/RuntimeException",
    "java/lang/Error",
    "java/lang/ArithmeticException",
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/ClassCastException",
    "java/lang/CloneNotSupportedException",
    "java/lang/IllegalArgumentException",
    "java/lang/IllegalStateException",
    "java/lang/IndexOutOfBoundsException",
    "java/lang/InterruptedException",
    "java/lang/NullPointerException",
    "java/lang/NumberFormatException",
    "java/lang/UnsupportedOperationException",
    "java/lang/AssertionError",
    "java/io/IOException",
];

/// The fields of the runtime library this compiler knows, which is `System.out`
/// and `System.err` and the limits of the number types.
const BUILT_IN_FIELDS: &[(&str, &str, &str)] = &[
    ("java/lang/System", "out", "Ljava/io/PrintStream;"),
    ("java/lang/System", "err", "Ljava/io/PrintStream;"),
    ("java/lang/Integer", "MAX_VALUE", "I"),
    ("java/lang/Integer", "MIN_VALUE", "I"),
    ("java/lang/Long", "MAX_VALUE", "J"),
    ("java/lang/Long", "MIN_VALUE", "J"),
    ("java/lang/Double", "MAX_VALUE", "D"),
    ("java/lang/Double", "MIN_VALUE", "D"),
    ("java/lang/Float", "MAX_VALUE", "F"),
    ("java/lang/Float", "MIN_VALUE", "F"),
    ("java/lang/Short", "MAX_VALUE", "S"),
    ("java/lang/Short", "MIN_VALUE", "S"),
    ("java/lang/Byte", "MAX_VALUE", "B"),
    ("java/lang/Byte", "MIN_VALUE", "B"),
    ("java/lang/Character", "MAX_VALUE", "C"),
    ("java/lang/Character", "MIN_VALUE", "C"),
];

/// A signature from [`BUILT_IN_METHODS`], if one of them is what was asked
/// about.
fn built_in_method(owner: &str, name: &str, count: usize) -> Option<Signature> {
    built_in_overloads(owner, name, count).into_iter().next()
}

/// Every built-in signature of this name that takes this many arguments.
fn built_in_overloads(owner: &str, name: &str, count: usize) -> Vec<Signature> {
    if name == "<init>" && count <= 1 && BUILT_IN_THROWABLES.contains(&owner) {
        let parameters = if count == 1 {
            vec![Type::Object("java/lang/String".to_string())]
        } else {
            Vec::new()
        };
        return vec![Signature {
            owner: owner.to_string(),
            name: name.to_string(),
            parameters,
            returns: Type::Void,
            static_: false,
            interface: false,
            variadic: false,
        }];
    }
    // Everything that can be thrown answers Throwable's methods, everything
    // answers what it inherits from, and every class answers Object's.
    let mut owners = vec![owner.to_string()];
    if BUILT_IN_THROWABLES.contains(&owner) {
        owners.push("java/lang/Throwable".to_string());
    }
    let mut at = owner.to_string();
    while let Some((_, above)) = BUILT_IN_ABOVE.iter().find(|(below, _)| *below == at) {
        if owners.iter().any(|held| held == above) {
            break;
        }
        owners.push((*above).to_string());
        at = (*above).to_string();
    }
    owners.push("java/lang/Object".to_string());

    let mut found = Vec::new();
    for held in owners {
        for (class, held_name, descriptor, static_) in BUILT_IN_METHODS {
            if *class != held || *held_name != name {
                continue;
            }
            let Some((parameters, returns)) = read_descriptor(descriptor) else {
                continue;
            };
            if parameters.len() != count {
                continue;
            }
            found.push(Signature {
                owner: class.to_string(),
                name: name.to_string(),
                parameters,
                returns,
                static_: *static_,
                interface: BUILT_IN_INTERFACES.contains(class),
                variadic: BUILT_IN_VARIADIC.contains(&(*class, *held_name)),
            });
        }
        if !found.is_empty() {
            // A class answers with its own before anything it inherits.
            break;
        }
    }
    found
}

/// The type of one of the fields in [`BUILT_IN_FIELDS`].
fn built_in_field(owner: &str, name: &str) -> Option<Type> {
    let (_, _, descriptor) = BUILT_IN_FIELDS
        .iter()
        .find(|(class, held, _)| *class == owner && *held == name)?;
    let mut at = 0usize;
    read_type(descriptor, &mut at)
}

/// The classes that can be named without anything on the classpath, because a
/// compiler that cannot say `String` cannot compile anything at all.
/// The internal name a written one stands for, as seen from one unit.
///
/// In order: the type being compiled, a name already written out in full, a
/// type in the same package, a type an import names, a wildcard import, and
/// `java.lang`. An import says where a class would live if it exists; it is not
/// proof that it does, which is why every step also asks whether the class is
/// actually there. Taking an import as proof is how a call to a class nobody
/// handed over got as far as being written into a class file.
/// The dotted name an expression is written as, where it is written as one.
///
/// `R.string` is a chain of `Field`s over a `Name`, and so is `a.b` where `a`
/// is a variable. This says what was written; whether it names anything is a
/// separate question.
fn written_as_a_path(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Name(name) => Some(name.clone()),
        Expression::Field { of, name } => Some(format!("{}.{name}", written_as_a_path(of)?)),
        _ => None,
    }
}

fn resolve_named(classpath: &Classpath, unit: &Unit, name: &str) -> Option<String> {
    let exists =
        |internal: &str| classpath.get(internal).is_some() || WELL_KNOWN.contains(&internal);
    if name == unit.name {
        return Some(unit.internal_name());
    }
    // A type declared inside this one, or beside it inside the same holder:
    // `Inner` written in `Outer` is `Outer$Inner`, and written in
    // `Outer$Other` it is still `Outer$Inner`.
    let mut holder = Some(unit.internal_name());
    while let Some(current) = holder {
        let nested = format!("{current}${name}");
        if exists(&nested) {
            return Some(nested);
        }
        holder = current
            .rsplit_once('$')
            .map(|(before, _)| before.to_string());
    }
    if name.contains('.') {
        let internal = name.replace('.', "/");
        if exists(&internal) {
            return Some(internal);
        }
    }
    // A nested class is written with a dot and named with a dollar, and what
    // comes before the dot is itself a name to resolve: `View.OnClickListener`
    // where `View` was imported is `android/view/View$OnClickListener`.
    if let Some((before, last)) = name.rsplit_once('.') {
        if let Some(holder) = resolve_named(classpath, unit, before) {
            let nested = format!("{holder}${last}");
            if exists(&nested) {
                return Some(nested);
            }
        }
    }
    if let Some(package) = &unit.package {
        let beside = format!("{}/{name}", package.replace('.', "/"));
        if exists(&beside) {
            return Some(beside);
        }
    } else if exists(name) {
        // No package, so a bare name and an internal name are the same thing.
        return Some(name.to_string());
    }
    for import in &unit.imports {
        if import.rsplit('.').next() == Some(name) {
            let internal = import.replace('.', "/");
            if exists(&internal) {
                return Some(internal);
            }
        }
    }
    for import in &unit.imports {
        if let Some(prefix) = import.strip_suffix(".*") {
            let candidate = format!("{}/{name}", prefix.replace('.', "/"));
            if exists(&candidate) {
                return Some(candidate);
            }
        }
    }
    let in_lang = format!("java/lang/{name}");
    exists(&in_lang).then_some(in_lang)
}

const WELL_KNOWN: &[&str] = &[
    "java/lang/Object",
    "java/lang/String",
    "java/lang/Enum",
    "java/lang/Record",
    "java/lang/Runnable",
    "java/lang/CharSequence",
    "java/lang/StringBuilder",
    "java/lang/System",
    "java/lang/Math",
    "java/lang/Integer",
    "java/lang/Long",
    "java/lang/Double",
    "java/lang/Float",
    "java/lang/Boolean",
    "java/lang/Character",
    "java/lang/Byte",
    "java/lang/Short",
    "java/lang/Number",
    "java/lang/Class",
    "java/io/PrintStream",
    "java/lang/Iterable",
    "java/lang/AutoCloseable",
    "java/util/Iterator",
    "java/util/Collection",
    "java/util/List",
    "java/util/Set",
    "java/util/Map",
    "java/util/ArrayList",
    "java/util/HashMap",
    "java/util/Objects",
    "java/util/Arrays",
    "android/app/Activity",
    "android/content/Context",
    "android/content/Intent",
    "android/os/Bundle",
    "android/view/View",
    "android/view/View$OnClickListener",
    "android/widget/TextView",
    "android/widget/Button",
    "android/widget/LinearLayout",
    "android/widget/Toast",
    "android/util/Log",
    "java/lang/Throwable",
    "java/lang/Exception",
    "java/lang/RuntimeException",
    "java/lang/Error",
    "java/lang/ArithmeticException",
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/ClassCastException",
    "java/lang/CloneNotSupportedException",
    "java/lang/IllegalArgumentException",
    "java/lang/IllegalStateException",
    "java/lang/IndexOutOfBoundsException",
    "java/lang/InterruptedException",
    "java/lang/NullPointerException",
    "java/lang/NumberFormatException",
    "java/lang/UnsupportedOperationException",
    "java/lang/AssertionError",
    "java/io/IOException",
];

impl Emitter<'_> {
    /// Puts a constant on the stack, using the narrowest instruction that
    /// holds it. `iconst_1` is one byte where `ldc` is two and a pool entry.
    fn push_int(&mut self, value: i64) {
        match value {
            // iconst_m1 is 0x02 and iconst_0 is 0x03, so the opcode for n is
            // 0x03 + n. The first version of this added one too many and every
            // small constant came out as the next one up: `value * 2` compiled
            // to `value * 3`. Nothing that checked the shape of a class file
            // could see it; reading the bytes back is what found it.
            -1..=5 => self.op((0x03 + value) as u8),
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
        self.pushes(&Type::Int);
    }

    fn push_string(&mut self, text: &str) {
        let index = self.pool.string(text);
        if index <= 255 {
            self.op1(0x12, index as u8);
        } else {
            self.op2(0x13, index);
        }
        self.pushes(&Type::Object("java/lang/String".to_string()));
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
        self.pushes(what);
    }

    /// Writes a value out of the stack and into a slot.
    ///
    /// The first version of this had every opcode one too high -- `istore` as
    /// 0x37, which is `lstore` -- and the compact forms were derived from
    /// those, which put them back where they belonged. So a method using four
    /// slots or fewer was correct and a method using five was not, and nothing
    /// short of a real verifier could tell: our own reader and `javap` both
    /// print what the bytes say without asking whether it makes sense.
    fn store(&mut self, slot: u16, what: &Type) {
        let (base, compact) = match what {
            Type::Long => (0x37u8, 0x3fu8),
            Type::Float => (0x38, 0x43),
            Type::Double => (0x39, 0x47),
            other if other.is_reference() => (0x3a, 0x4b),
            _ => (0x36, 0x3b),
        };
        if slot <= 3 {
            self.op(compact + slot as u8);
        } else if slot <= 255 {
            self.op1(base, slot as u8);
        } else {
            self.op(0xc4);
            self.op2(base, slot);
        }
        self.pops(i32::from(what.width()));
    }

    /// Widens what is on the stack, when Java says it happens on its own.
    /// Makes what is on the stack fit where it is going.
    ///
    /// Widening, boxing and unboxing all happen here, and all of them are
    /// instructions. What comes back is whether it fits at all; the place that
    /// asked says what is wrong when it does not, because it is the one that
    /// knows what it was doing.
    fn fit(&mut self, found: &Type, wanted: &Type, line: u32) -> Result<bool, Diagnostic> {
        if found.may_be_given_to(wanted) {
            if !found.is_reference() && !wanted.is_reference() {
                self.convert(found, wanted, line)?;
            }
            return Ok(true);
        }
        let Some(now) = self.box_or_unbox(found, wanted) else {
            return Ok(false);
        };
        if !now.is_reference() && !wanted.is_reference() {
            self.convert(&now, wanted, line)?;
        }
        Ok(true)
    }

    /// Puts a primitive in its box, or takes one out, when that is what the
    /// place it is going wants.
    ///
    /// Java calls this autoboxing and does it silently. Doing it silently is
    /// the whole point: `list.add(1)` is what people write, and `1` is not an
    /// object. The instruction is a call -- `Integer.valueOf` one way,
    /// `intValue` the other -- so it costs what it costs, and nothing here
    /// pretends otherwise.
    fn box_or_unbox(&mut self, from: &Type, to: &Type) -> Option<Type> {
        // A primitive going where an object is wanted.
        if !from.is_reference() && *from != Type::Void && to.is_reference() {
            let boxed = boxed_name(from)?;
            // The box has to be one the place will take: an Integer fits
            // where an Object or a Number or an Integer is wanted, and
            // nowhere else.
            if let Type::Object(named) = to {
                let fits = named == boxed
                    || named == "java/lang/Object"
                    || named == "java/lang/Number"
                    || named == "java/lang/Comparable"
                    || named == "java/io/Serializable";
                if !fits {
                    return None;
                }
            } else {
                return None;
            }
            let descriptor = format!(
                "({}){}",
                from.descriptor(),
                Type::Object(boxed.to_string()).descriptor()
            );
            let index = self.pool.method(boxed, "valueOf", &descriptor, false);
            self.op2(0xb8, index);
            self.pops(i32::from(from.width()));
            self.pushes(&Type::Object(boxed.to_string()));
            return Some(Type::Object(boxed.to_string()));
        }

        // An object going where a primitive is wanted.
        if from.is_reference() && !to.is_reference() && *to != Type::Void {
            let Type::Object(named) = from else {
                return None;
            };
            // What comes out is decided by the box, not by what is wanted:
            // an Integer unboxes to an int, and the int is then widened the
            // ordinary way.
            let (holder, inner) = match named.as_str() {
                "java/lang/Boolean" => ("booleanValue", Type::Boolean),
                "java/lang/Byte" => ("byteValue", Type::Byte),
                "java/lang/Short" => ("shortValue", Type::Short),
                "java/lang/Character" => ("charValue", Type::Char),
                "java/lang/Integer" => ("intValue", Type::Int),
                "java/lang/Long" => ("longValue", Type::Long),
                "java/lang/Float" => ("floatValue", Type::Float),
                "java/lang/Double" => ("doubleValue", Type::Double),
                _ => return None,
            };
            let descriptor = format!("(){}", inner.descriptor());
            let index = self.pool.method(named, holder, &descriptor, false);
            self.op2(0xb6, index);
            self.pops(1);
            self.pushes(&inner);
            return Some(inner);
        }
        None
    }

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
        self.pops(i32::from(from.width()));
        self.pushes(to);
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
                self.pushes(&Type::Long);
                Ok(Type::Long)
            }
            Expression::Float(value) => {
                let index = self.pool.float(*value as f32);
                if index <= 255 {
                    self.op1(0x12, index as u8);
                } else {
                    self.op2(0x13, index);
                }
                self.pushes(&Type::Float);
                Ok(Type::Float)
            }
            Expression::Double(value) => {
                let index = self.pool.double(*value);
                self.op2(0x14, index);
                self.pushes(&Type::Double);
                Ok(Type::Double)
            }
            Expression::Str(text) => {
                self.push_string(text);
                Ok(Type::Object("java/lang/String".to_string()))
            }
            Expression::Null => {
                self.op(0x01);
                self.pushes_raw(Verified::Null);
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
                // `System.out` is a static field of a class, not a field of a
                // value called `System`. Reading the left side as a value first
                // would report that `System` is not visible, which is true and
                // tells nobody anything.
                if let Expression::Name(maybe_class) = of.as_ref() {
                    if self.meant_as_a_class(maybe_class) {
                        let owner = self.resolve_class(maybe_class, line)?;
                        return self.read_static_field(&owner, name, line);
                    }
                }
                // `R.string.app_name` is a field of a class written inside a
                // class, and the dots between them look exactly like fields of
                // fields. What is in front of the last dot is tried as a class
                // before it is tried as a value.
                if let Some(path) = written_as_a_path(of) {
                    let head = path.split('.').next().unwrap_or_default();
                    if self.meant_as_a_class(head) {
                        if let Some(owner) = resolve_named(self.classpath, self.unit, &path) {
                            return self.read_static_field(&owner, name, line);
                        }
                    }
                }
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
                self.op(array_load(&element));
                self.pops(2);
                self.pushes(&element);
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
                    self.pops(1);
                    self.pushes(&target);
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
                            self.pushes(&Type::Long);
                            self.op(0x83);
                            self.pops(2);
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
                        self.pops(1);
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
                        self.pops(1);
                        let jump = self.jump(0x99);
                        let beneath = self.stack.clone();
                        self.push_int(0);
                        let over = self.jump(0xa7);
                        self.land(jump);
                        self.stack_is(beneath.clone());
                        self.a_branch_lands_here();
                        self.push_int(1);
                        self.land(over);
                        let mut settled = beneath;
                        settled.push(Verified::Integer);
                        self.stack_is(settled);
                        self.a_branch_lands_here();
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
                self.pops(1);
                let to_else = self.jump(0x99);
                // What is under the two sides. A `?:` written among a call's
                // arguments has those arguments beneath it, and a frame that
                // says otherwise is a claim the verifier throws the class out
                // for.
                let beneath = self.stack.clone();
                let taken = self.value(then, line)?;
                let over = self.jump(0xa7);
                self.land(to_else);
                self.stack_is(beneath.clone());
                self.a_branch_lands_here();
                let other = self.value(otherwise, line)?;
                self.land(over);
                let landed = if taken == other || taken.is_reference() {
                    taken.clone()
                } else {
                    taken.promoted_with(&other).unwrap_or(Type::Int)
                };
                let mut settled = beneath;
                let held = Verified::of(&landed);
                let wide = held.is_wide();
                settled.push(held);
                if wide {
                    settled.push(Verified::Top);
                }
                self.stack_is(settled);
                self.a_branch_lands_here();
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
            Expression::InstanceOf { of, what, binds } => {
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
                let named = match &target {
                    Type::Object(name) => name.clone(),
                    other => other.descriptor(),
                };
                if let Some(binding) = binds {
                    // The value is wanted twice: once to test and once to keep
                    // if the test passed. It goes into a slot of its own first,
                    // because working the expression out a second time would
                    // be wrong the moment it had an effect.
                    let object = Type::Object("java/lang/Object".to_string());
                    let held = self.declare("$tested", object.clone());
                    self.store(held, &object);

                    // The name is given `null` before the test, so that both
                    // ways out of the test agree on what is in its slot. Null
                    // fits anywhere, which is what lets the verifier accept
                    // one frame for both paths -- and the name is only in
                    // scope where the test passed, so nothing reads it.
                    self.op(0x01);
                    self.pushes_raw(Verified::Null);
                    let slot = self.declare(binding, target.clone());
                    self.store(slot, &target);

                    self.load(held, &object);
                    let index = self.pool.class(&named);
                    self.op2(0xc1, index);
                    // `instanceof` takes the object and leaves an int, so what
                    // the frames say is on the stack has to change with it.
                    self.pops(1);
                    self.pushes(&Type::Boolean);
                    self.duplicates();
                    self.pops(1);
                    let over = self.jump(0x99);

                    self.load(held, &object);
                    let index = self.pool.class(&named);
                    self.op2(0xc0, index);
                    self.pops(1);
                    self.pushes(&target);
                    self.store(slot, &target);

                    self.land(over);
                    self.a_branch_lands_here();
                    return Ok(Type::Boolean);
                }
                let index = self.pool.class(&named);
                self.op2(0xc1, index);
                self.pops(1);
                self.pushes(&Type::Boolean);
                Ok(Type::Boolean)
            }
            Expression::Switch { subject, arms } => self.switch_value(subject, arms, line),
            Expression::Anonymous {
                what,
                arguments,
                body,
                line: written,
                ..
            } => self.anonymous(what, arguments, body, *written),
            Expression::Lambda {
                parameters,
                body,
                expression,
                line: written,
            } => self.lambda(parameters, body, *expression, *written),
            Expression::MethodRef {
                on,
                name,
                line: written,
            } => self.method_reference(on, name, *written),
            Expression::NewArray { of, lengths, empty } => {
                self.new_array(of, lengths, *empty, line)
            }
            Expression::ArrayOf {
                of,
                values,
                line: written,
            } => {
                let Some(named) = of else {
                    return Err(at(
                        "EJ121",
                        *written,
                        1,
                        "There is nothing here saying what this array holds.",
                    )
                    .with_suggestion(
                        "Write `new` and the type in front of it, or give it to something \
                         declared as an array.",
                    ));
                };
                let element = self.resolve(named, *written)?;
                self.array_of(&Type::Array(Box::new(element)), values, *written)
            }
            Expression::ClassLiteral { of, line: written } => {
                let what = self.resolve(of, *written)?;
                let named = match &what {
                    Type::Object(name) => name.clone(),
                    Type::Array(_) => what.descriptor(),
                    // A primitive's class is a static field of its box, which
                    // is what `int.class` means.
                    other => {
                        let boxed = boxed_name(other).ok_or_else(|| {
                            at("EJ122", *written, 1, "That has no class to name.")
                        })?;
                        let index = self.pool.field(boxed, "TYPE", "Ljava/lang/Class;");
                        self.op2(0xb2, index);
                        self.pushes(&Type::Object("java/lang/Class".to_string()));
                        return Ok(Type::Object("java/lang/Class".to_string()));
                    }
                };
                let index = self.pool.class(&named);
                if index <= 255 {
                    self.op1(0x12, index as u8);
                } else {
                    self.op2(0x13, index);
                }
                self.pushes(&Type::Object("java/lang/Class".to_string()));
                Ok(Type::Object("java/lang/Class".to_string()))
            }
            Expression::OuterThis { of, line: written } => self.outer_this(of, *written),
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
                self.pushes(&what);
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
                self.pops(1);
                self.pushes(&what);
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
                    self.pushes(&what);
                } else {
                    self.load(0, &Type::Object(self.this_class.clone()));
                    self.op2(0xb4, index);
                    self.pops(1);
                    self.pushes(&what);
                }
                return Ok(what);
            }
        }
        // A class written inside another can read what that one holds. Its
        // instance is in a field, so the road is one field access longer.
        if let Some(enclosing) = self.unit.outer.clone() {
            if let Some((holder, (_, what, static_))) = self.classpath.find_field(&enclosing, name)
            {
                let (holder, what, static_) = (holder.name.clone(), what.clone(), *static_);
                let descriptor = what.descriptor();
                if static_ {
                    let index = self.pool.field(&holder, name, &descriptor);
                    self.op2(0xb2, index);
                    self.pushes(&what);
                    return Ok(what);
                }
                self.reach_the_enclosing_instance(&enclosing)?;
                let index = self.pool.field(&holder, name, &descriptor);
                self.op2(0xb4, index);
                self.pops(1);
                self.pushes(&what);
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

    /// Whether a bare name in front of a dot is meant as a class rather than
    /// as something holding a value.
    ///
    /// A local, a parameter or a field wins: Java lets a variable shadow a
    /// class name, and code that does it means the variable. Everything else
    /// is meant as a class -- whether or not it turns out to be one, because
    /// saying what is wrong with it as a class is the useful thing. Falling
    /// through to read it as a value would report that a name is not visible,
    /// which is true and tells nobody anything.
    fn meant_as_a_class(&self, name: &str) -> bool {
        if self.local(name).is_some() {
            return false;
        }
        if self.unit.fields.iter().any(|held| held.name == name) {
            return false;
        }
        let inherited = self
            .unit
            .extends
            .clone()
            .and_then(|parent| self.resolve_class(&parent, 1).ok())
            .and_then(|owner| self.classpath.find_field(&owner, name).map(|_| ()));
        if inherited.is_some() {
            return false;
        }
        // A class written inside another reads what that one holds, so a name
        // it holds is a value here even though nothing here declares it.
        if let Some(enclosing) = &self.unit.outer {
            if self.classpath.find_field(enclosing, name).is_some() {
                return false;
            }
        }
        true
    }

    /// Writes a value into a field of the class this one was written inside.
    fn assign_through_the_enclosing_instance(
        &mut self,
        enclosing: &str,
        name: &str,
        operator: Option<Binary>,
        value: &Expression,
        line: u32,
        wanted: bool,
    ) -> Result<Type, Diagnostic> {
        let Some((holder, (_, what, static_))) = self.classpath.find_field(enclosing, name) else {
            return Err(at(
                "EJ229",
                line,
                1,
                format!("`{name}` is nothing that can be assigned to."),
            ));
        };
        let (holder, what, static_) = (holder.name.clone(), what.clone(), *static_);
        if wanted {
            return Err(unsupported(
                line,
                1,
                "Using the value of an assignment into the class around this one",
            ));
        }
        if operator.is_some() {
            return Err(unsupported(
                line,
                1,
                "A compound assignment into the class around this one",
            ));
        }
        if !static_ {
            self.reach_the_enclosing_instance(enclosing)?;
        }
        let found = self.value_for(value, &what, line)?;
        if !self.fit(&found, &what, line)? {
            return Err(at(
                "EJ228",
                line,
                1,
                format!(
                    "A {} cannot be put in `{name}`, which is a {}.",
                    found.readable(),
                    what.readable()
                ),
            ));
        }
        let descriptor = what.descriptor();
        let index = self.pool.field(&holder, name, &descriptor);
        if static_ {
            self.op2(0xb3, index);
            self.pops(i32::from(what.width()));
        } else {
            self.op2(0xb5, index);
            self.pops(i32::from(what.width()) + 1);
        }
        Ok(what)
    }

    /// Puts the instance of the class this one was written inside on the
    /// stack.
    fn reach_the_enclosing_instance(&mut self, enclosing: &str) -> Result<(), Diagnostic> {
        let this = Type::Object(self.this_class.clone());
        self.load(0, &this);
        let held = Type::Object(enclosing.to_string());
        let descriptor = held.descriptor();
        let index = self
            .pool
            .field(&self.this_class.clone(), OUTER, &descriptor);
        self.op2(0xb4, index);
        self.pops(1);
        self.pushes(&held);
        Ok(())
    }

    /// A static field named through its class.
    fn read_static_field(
        &mut self,
        owner: &str,
        name: &str,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let known = self
            .classpath
            .find_field(owner, name)
            .map(|(holder, (_, what, static_))| (holder.name.clone(), what.clone(), *static_));
        let (holder, what) = match known {
            Some((holder, what, true)) => (holder, what),
            Some((_, _, false)) => {
                return Err(at(
                    "EJ214",
                    line,
                    1,
                    format!(
                        "`{name}` belongs to an instance of `{}`, and this names the class.",
                        owner.replace('/', ".")
                    ),
                ))
            }
            None => match built_in_field(owner, name) {
                Some(what) => (owner.to_string(), what),
                None => {
                    return Err(at(
                        "EJ213",
                        line,
                        1,
                        format!(
                            "`{}` has no field called `{name}` that this compilation knows.",
                            owner.replace('/', ".")
                        ),
                    )
                    .with_suggestion(
                        "Hand the class file that declares it over as a dependency. Nothing \
                         is emitted for a field nobody has seen.",
                    ))
                }
            },
        };
        let descriptor = what.descriptor();
        let index = self.pool.field(&holder, name, &descriptor);
        self.op2(0xb2, index);
        self.pushes(&what);
        Ok(what)
    }

    fn read_field(&mut self, owner: &Type, name: &str, line: u32) -> Result<Type, Diagnostic> {
        if let Type::Array(_) = owner {
            if name == "length" {
                self.op(0xbe);
                self.pops(1);
                self.pushes(&Type::Int);
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
            // A field nobody handed a class file for may still be one of the
            // few this compiler knows on its own, and every one of those is
            // static.
            if let Some(what) = built_in_field(class, name) {
                let descriptor = what.descriptor();
                let index = self.pool.field(class, name, &descriptor);
                self.op(0x57);
                self.pops(1);
                self.op2(0xb2, index);
                self.pushes(&what);
                return Ok(what);
            }
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
            self.pops(1);
            self.op2(0xb2, index);
            self.pushes(&what);
        } else {
            self.op2(0xb4, index);
            self.pops(1);
            self.pushes(&what);
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
            self.pops(1);
            let beneath = self.stack.clone();
            let second = self.value(right, line)?;
            if second != Type::Boolean {
                return Err(at("EJ214", line, 1, "`&&` and `||` want booleans."));
            }
            let over = self.jump(0xa7);
            self.land(shortcut);
            self.stack_is(beneath.clone());
            self.a_branch_lands_here();
            self.push_int(i64::from(operator == Binary::OrElse));
            self.land(over);
            let mut settled = beneath;
            settled.push(Verified::Integer);
            self.stack_is(settled);
            self.a_branch_lands_here();
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
        self.binary_with(operator, &left_type, right, line)
    }

    /// The arithmetic half of a binary operator, with the left side already on
    /// the stack.
    ///
    /// `a[i] += x` works the element out once and writes it back, so the left
    /// side is read before this is reached and cannot be read again. The
    /// short-circuit operators and string concatenation never get here,
    /// because neither of them is arithmetic.
    fn binary_with(
        &mut self,
        operator: Binary,
        left_type: &Type,
        right: &Expression,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let left_type = left_type.clone();

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
            self.pops(1);
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
                self.pops(i32::from(common.width()));
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
                    self.pops(1);
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
                    self.pops(2);
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
                self.pops(3);
                None
            }
            (Type::Float, _) => {
                self.op(0x95);
                self.pops(1);
                None
            }
            (Type::Double, _) => {
                self.op(0x97);
                self.pops(3);
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
                self.pops(2);
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
                self.pops(1);
                self.jump(code)
            }
        };
        // Both ways out of the comparison start from the same stack and end
        // with one more thing on it.
        let beneath = self.stack.clone();
        self.push_int(0);
        let over = self.jump(0xa7);
        self.land(jump);
        self.stack_is(beneath.clone());
        self.a_branch_lands_here();
        self.push_int(1);
        self.land(over);
        let mut settled = beneath;
        settled.push(Verified::Integer);
        self.stack_is(settled);
        self.a_branch_lands_here();
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
        self.type_of(expression, line)
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
        self.pushes(&Type::Object(builder.to_string()));
        self.duplicates();
        let init = self.pool.method(builder, "<init>", "()V", false);
        self.op2(0xb7, init);
        self.pops(1);

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
            self.pops(i32::from(what.width()));
        }

        let finish = self
            .pool
            .method(builder, "toString", "()Ljava/lang/String;", false);
        self.op2(0xb6, finish);
        let text = Type::Object("java/lang/String".to_string());
        self.pops(1);
        self.pushes(&text);
        Ok(text)
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
        // Where the `new` is, because that is the name the verifier knows the
        // object by until its constructor has run.
        let made_at = self.code.len() as u16;
        self.op2(0xbb, index);
        self.pushes_raw(Verified::Uninitialized(made_at));
        self.duplicates();

        // A class that belongs to an instance is made from one, and the one it
        // is made from is whichever is here. Nobody writes that down.
        let belongs = self
            .classpath
            .get(&class)
            .and_then(|known| known.outer.clone());
        if let Some(enclosing) = &belongs {
            // In a static method there is no instance here, and slot zero
            // holds the first parameter rather than `this`.
            if self.this_class == *enclosing && !self.static_ {
                self.load(0, &Type::Object(self.this_class.clone()));
            } else if !self.static_ && self.unit.outer.as_deref() == Some(enclosing.as_str()) {
                self.reach_the_enclosing_instance(enclosing)?;
            } else {
                return Err(at(
                    "EJ252",
                    line,
                    1,
                    format!(
                        "`{}` belongs to an instance of `{}`, and there is none here.",
                        class.replace('/', "."),
                        enclosing.replace('/', ".")
                    ),
                )
                .with_suggestion(
                    "Write it `static` if it does not need one, or make it from inside \
                     the class it belongs to.",
                ));
            }
        }

        let signature = if belongs.is_some() {
            // The instance it belongs to is a parameter nobody wrote, so the
            // constructor being looked for takes one more than was handed
            // over.
            self.constructor_taking(&class, arguments.len() + 1)
        } else {
            self.signature_for(&class, "<init>", arguments, line)?
        };
        let descriptor = match signature {
            Some(found) => {
                // The instance it belongs to is already on the stack, so the
                // parameter that holds it is not one of the arguments.
                let wanted = if belongs.is_some() && !found.parameters.is_empty() {
                    &found.parameters[1..]
                } else {
                    &found.parameters[..]
                };
                self.arguments_for(wanted, arguments, line)?;
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
        self.pops(taken + 1);
        // The constructor has run, so every copy of that object -- the one
        // left on the stack, and any that was stored away -- is the class it
        // was made as rather than something waiting to become it.
        self.now_initialized(made_at, &class);
        Ok(target)
    }

    /// Says the object `new` left at this offset has had its constructor run.
    fn now_initialized(&mut self, made_at: u16, class: &str) {
        let settled = Verified::Object(class.to_string());
        for held in self.stack.iter_mut().chain(self.slots.iter_mut()) {
            if *held == Verified::Uninitialized(made_at) {
                *held = settled.clone();
            }
        }
    }

    /// Puts arguments on the stack, each converted to what the method wants.
    /// Writes the arguments a call takes, packing the trailing ones into an
    /// array where the method was written with `...`.
    ///
    /// `String.format("%d", n)` hands over two things and the method takes
    /// two: a String and an array. Making that array is the whole of what
    /// varargs is, and it happens here rather than at run time.
    fn arguments_for_signature(
        &mut self,
        signature: &Signature,
        given: &[Expression],
        line: u32,
    ) -> Result<(), Diagnostic> {
        let wanted = &signature.parameters;
        if !signature.variadic || wanted.is_empty() {
            return self.arguments_for(wanted, given, line);
        }
        let fixed = wanted.len() - 1;
        let Some(Type::Array(element)) = wanted.last().cloned() else {
            return self.arguments_for(wanted, given, line);
        };

        // Handing over the array itself is still allowed, and is what a call
        // passing exactly one thing of the right shape means.
        if given.len() == wanted.len() {
            if let Some(last) = given.last() {
                let peeked = self.type_of(last, line)?;
                if peeked.may_be_given_to(&Type::Array(element.clone())) {
                    return self.arguments_for(wanted, given, line);
                }
            }
        }
        if given.len() < fixed {
            return Err(at(
                "EJ220",
                line,
                1,
                format!(
                    "That takes at least {fixed} argument(s) and was given {}.",
                    given.len()
                ),
            ));
        }

        self.arguments_for(&wanted[..fixed], &given[..fixed], line)?;
        self.array_of(&Type::Array(element), &given[fixed..], line)?;
        Ok(())
    }

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
            let found = self.value_for(expression, want, line)?;
            if !self.fit(&found, want, line)? {
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
            let Some(signature) = self.signature_for(&owner, name, arguments, line)? else {
                return Err(self.no_such_method(&owner, name, arguments.len(), line));
            };
            self.load(0, &Type::Object(self.this_class.clone()));
            self.arguments_for_signature(&signature, arguments, line)?;
            let descriptor = signature.descriptor();
            let index = self.pool.method(&signature.owner, name, &descriptor, false);
            self.op2(0xb7, index);
            let taken: i32 = signature
                .parameters
                .iter()
                .map(|p| i32::from(p.width()))
                .sum();
            self.pops(taken + 1);
            self.pushes(&signature.returns);
            return Ok(signature.returns);
        }

        // A bare `name(...)`, or `this.name(...)`: a method of the class being
        // compiled. The two mean the same thing and used to take different
        // roads, only one of which arrived.
        let on = match on {
            Some(Expression::This) => None,
            other => other,
        };
        let Some(on) = on else {
            let own = self
                .unit
                .methods
                .iter()
                .find(|held| held.name == name && held.parameters.len() == arguments.len())
                .cloned();
            let Some(own) = own else {
                // Not written here, so it comes from above: `setContentView`
                // inside an activity is the activity's. Looking only at what
                // this class declares is what made a bare call to an inherited
                // method a mistake rather than a call.
                if let Some(parent) = self.unit.extends.clone() {
                    let owner = self.resolve_class(&parent, line)?;
                    if let Some(signature) = self.signature_for(&owner, name, arguments, line)? {
                        if !signature.static_ {
                            if self.static_ {
                                return Err(at(
                                    "EJ225",
                                    line,
                                    1,
                                    format!(
                                        "`{name}` belongs to an instance and this method \
                                         is static."
                                    ),
                                ));
                            }
                            self.load(0, &Type::Object(self.this_class.clone()));
                        }
                        self.arguments_for_signature(&signature, arguments, line)?;
                        // The call is written against this class, not the one
                        // that declares it, which is what `javac` does and
                        // what lets a subclass override it.
                        let owner = if signature.static_ {
                            signature.owner.clone()
                        } else {
                            self.this_class.clone()
                        };
                        let descriptor = signature.descriptor();
                        let index = self.pool.method(&owner, name, &descriptor, false);
                        self.op2(if signature.static_ { 0xb8 } else { 0xb6 }, index);
                        let taken: i32 = signature
                            .parameters
                            .iter()
                            .map(|one| i32::from(one.width()))
                            .sum();
                        let popped = taken + i32::from(!signature.static_);
                        self.pops(popped);
                        self.pushes(&signature.returns);
                        return Ok(signature.returns);
                    }
                }
                if let Some(enclosing) = self.unit.outer.clone() {
                    if let Some(signature) =
                        self.signature_for(&enclosing, name, arguments, line)?
                    {
                        if !signature.static_ {
                            self.reach_the_enclosing_instance(&enclosing)?;
                        }
                        self.arguments_for_signature(&signature, arguments, line)?;
                        let descriptor = signature.descriptor();
                        let index = self.pool.method(
                            &signature.owner,
                            name,
                            &descriptor,
                            signature.interface,
                        );
                        self.op2(if signature.static_ { 0xb8 } else { 0xb6 }, index);
                        let taken: i32 = signature
                            .parameters
                            .iter()
                            .map(|one| i32::from(one.width()))
                            .sum();
                        let popped = taken + i32::from(!signature.static_);
                        self.pops(popped);
                        self.pushes(&signature.returns);
                        return Ok(signature.returns);
                    }
                }
                return Err(at(
                    "EJ224",
                    line,
                    1,
                    format!(
                        "`{name}` is not a method of this class or of one above it taking \
                         {} argument(s).",
                        arguments.len()
                    ),
                )
                .with_suggestion(
                    "Hand the class file that declares it over as a dependency, or write \
                     the method here.",
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
                variadic: false,
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
            self.pops(popped);
            self.pushes(&returns);
            return Ok(returns);
        };

        // `something.name(...)`. If `something` is a bare name that is a class
        // rather than a value, this is a static call.
        if let Expression::Name(maybe_class) = on {
            if self.meant_as_a_class(maybe_class) {
                // It is not a value, so it is meant to be a class, and saying
                // what is wrong with it as a class is the useful thing. Falling
                // through to read it as a value would report that a name is not
                // visible, which is true and tells nobody anything.
                let owner = self.resolve_class(maybe_class, line)?;
                let Some(signature) = self.signature_for(&owner, name, arguments, line)? else {
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
                self.arguments_for_signature(&signature, arguments, line)?;
                let descriptor = signature.descriptor();
                let index = self.pool.method(&signature.owner, name, &descriptor, false);
                self.op2(0xb8, index);
                let taken: i32 = signature
                    .parameters
                    .iter()
                    .map(|p| i32::from(p.width()))
                    .sum();
                self.pops(taken);
                self.pushes(&signature.returns);
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
        let Some(signature) = self.signature_for(&owner, name, arguments, line)? else {
            return Err(self.no_such_method(&owner, name, arguments.len(), line));
        };
        self.arguments_for_signature(&signature, arguments, line)?;
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
            self.pops(taken + 1);
            self.pushes(&signature.returns);
        } else {
            self.op2(0xb6, index);
            let taken: i32 = signature
                .parameters
                .iter()
                .map(|p| i32::from(p.width()))
                .sum();
            self.pops(taken + 1);
            self.pushes(&signature.returns);
        }
        Ok(signature.returns)
    }

    /// Writes an expression that is going somewhere known.
    ///
    /// A lambda has no type until it has a target, so the target has to reach
    /// it. Everything else ignores this.
    fn value_for(
        &mut self,
        expression: &Expression,
        wanted: &Type,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        // `{1, 2}` is a list of values until something says what it holds.
        if let Expression::ArrayOf {
            of: None,
            values,
            line: written,
        } = expression
        {
            let values = values.clone();
            return self.array_of(wanted, &values, *written);
        }
        self.expecting = Some(wanted.clone());
        let found = self.value(expression, line);
        self.expecting = None;
        found
    }

    /// What an expression's type is, without keeping the code that works it
    /// out.
    ///
    /// Choosing between `println(String)` and `println(int)` needs to know
    /// what is being printed, and what is being printed is only known by
    /// working it out -- which writes instructions. So it is written, read,
    /// and then unwritten. Everything the emitter tracks is put back; the
    /// constant pool is not, because entries there are shared and the real
    /// emission that follows would have interned the same ones anyway.
    fn type_of(&mut self, expression: &Expression, line: u32) -> Result<Type, Diagnostic> {
        let code = self.code.len();
        let depth = self.depth;
        let max_depth = self.max_depth;
        let stack = self.stack.clone();
        let frames = self.frames.len();
        let locals = self.locals.len();
        let next_slot = self.next_slot;
        let max_slot = self.max_slot;
        // Cloned rather than counted: working the type out may have declared a
        // local in a slot an outer one already held, and putting the length
        // back would leave the wrong type named there.
        let slots = self.slots.clone();
        let handlers = self.handlers.len();
        let inlined = self.inlined.len();

        let found = self.value(expression, line);

        self.code.truncate(code);
        self.depth = depth;
        self.max_depth = max_depth;
        self.stack = stack;
        self.frames.truncate(frames);
        self.locals.truncate(locals);
        self.next_slot = next_slot;
        self.max_slot = max_slot;
        self.slots = slots;
        self.handlers.truncate(handlers);
        self.inlined.truncate(inlined);
        found
    }

    /// The signature of a method, from a class file handed over if there is
    /// one and from the built-in table if there is not.
    ///
    /// A dependency wins, always. What is handed over is the truth about the
    /// version being built against; the table is what to fall back on when
    /// nothing was handed over at all.
    fn find_signature(&self, owner: &str, name: &str, count: usize) -> Option<Signature> {
        if let Some(found) = self.classpath.find_method(owner, name, count) {
            return Some(found.clone());
        }
        built_in_method(owner, name, count)
    }

    /// The signature of a method, chosen by what it is being handed.
    ///
    /// `println` is nine methods with one argument each. Picking by the count
    /// alone gets the first of them, which is the String one, and then
    /// printing a number is refused. So the arguments are typed first and the
    /// candidate that accepts them is the one written.
    fn signature_for(
        &mut self,
        owner: &str,
        name: &str,
        arguments: &[Expression],
        line: u32,
    ) -> Result<Option<Signature>, Diagnostic> {
        let candidates = self.candidates(owner, name, arguments.len());
        if candidates.len() <= 1 {
            return Ok(candidates.into_iter().next());
        }

        // A lambda has no type until it has a target, and the target is what
        // is being chosen here. Where one is handed over, the count is all
        // there is to go on.
        if arguments
            .iter()
            .any(|one| matches!(one, Expression::Lambda { .. }))
        {
            return Ok(candidates.into_iter().next());
        }
        let mut given = Vec::with_capacity(arguments.len());
        for expression in arguments {
            given.push(self.type_of(expression, line)?);
        }

        // An exact match first, because `println(1)` means the int one even
        // though an int can be widened to a long.
        for exact in [true, false] {
            for candidate in &candidates {
                let fits = candidate
                    .parameters
                    .iter()
                    .zip(given.iter())
                    .all(|(want, have)| {
                        if exact {
                            want == have
                        } else {
                            have.may_be_given_to(want)
                        }
                    });
                if fits {
                    return Ok(Some(candidate.clone()));
                }
            }
        }
        Ok(candidates.into_iter().next())
    }

    /// Every signature of this name and shape that could be meant.
    ///
    /// A method written with `...` answers to any number of arguments from its
    /// fixed ones upwards, so counting is not enough to find it.
    fn candidates(&self, owner: &str, name: &str, count: usize) -> Vec<Signature> {
        let exact = self.candidates_taking(owner, name, count);
        if !exact.is_empty() {
            return exact;
        }
        self.variadic_taking(owner, name, count)
    }

    /// Every variadic signature of this name that could take this many.
    fn variadic_taking(&self, owner: &str, name: &str, count: usize) -> Vec<Signature> {
        let mut found = Vec::new();
        for ancestor in self.classpath.ancestors(owner) {
            if let Some(known) = self.classpath.get(&ancestor) {
                found.extend(
                    known
                        .methods
                        .iter()
                        .filter(|one| one.name == name && one.variadic)
                        .cloned(),
                );
            }
            for (class, held, descriptor, static_) in BUILT_IN_METHODS {
                if *class != ancestor
                    || *held != name
                    || !BUILT_IN_VARIADIC.contains(&(*class, *held))
                {
                    continue;
                }
                let Some((parameters, returns)) = read_descriptor(descriptor) else {
                    continue;
                };
                found.push(Signature {
                    owner: class.to_string(),
                    name: name.to_string(),
                    parameters,
                    returns,
                    static_: *static_,
                    interface: BUILT_IN_INTERFACES.contains(class),
                    variadic: true,
                });
            }
            if !found.is_empty() {
                break;
            }
        }
        // The fixed ones have to be handed over; the rest go in the array.
        found.retain(|one| !one.parameters.is_empty() && one.parameters.len() - 1 <= count);
        found
    }

    fn candidates_taking(&self, owner: &str, name: &str, count: usize) -> Vec<Signature> {
        // A class file handed over is the truth, and `find_method` already
        // climbs as far as the classpath can see.
        if let Some(found) = self.classpath.find_method(owner, name, count) {
            return vec![found.clone()];
        }
        // Where the classpath runs out, the built-in table takes over -- for
        // the class and for everything it inherits from. An enum handed over
        // as a class file stops at `java.lang.Enum`, which is not on the
        // classpath and is where `name()` and `ordinal()` live.
        for ancestor in self.classpath.ancestors(owner) {
            let found = built_in_overloads(&ancestor, name, count);
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
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
                    None => self.value_for(value, &local.what, line)?,
                    Some(operator) => {
                        self.binary(operator, &Expression::Name(name.clone()), value, line)?
                    }
                };
                if !self.fit(&found, &local.what, line)? {
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
                if wanted {
                    self.op(if local.what.width() == 2 { 0x5c } else { 0x59 });
                    self.pushes(&local.what);
                }
                self.store(local.slot, &local.what);
                Ok(local.what)
            }
            Expression::Name(name) => {
                let own = self
                    .unit
                    .fields
                    .iter()
                    .find(|held| held.name == *name)
                    .cloned();
                let Some(field) = own else {
                    // A class written inside another can write what that one
                    // holds, the same way it reads it.
                    if let Some(enclosing) = self.unit.outer.clone() {
                        if self.classpath.find_field(&enclosing, name).is_some() {
                            return self.assign_through_the_enclosing_instance(
                                &enclosing, name, operator, value, line, wanted,
                            );
                        }
                    }
                    return Err(at(
                        "EJ229",
                        line,
                        1,
                        format!("`{name}` is nothing that can be assigned to."),
                    ));
                };
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
                    None => self.value_for(value, &what, line)?,
                    Some(operator) => {
                        self.binary(operator, &Expression::Name(name.clone()), value, line)?
                    }
                };
                if !self.fit(&found, &what, line)? {
                    return Err(at(
                        "EJ228",
                        line,
                        1,
                        format!(
                            "A {} cannot be put in `{name}`, which is a {}.",
                            found.readable(),
                            what.readable()
                        ),
                    ));
                }
                if wanted {
                    self.op(if what.width() == 2 { 0x5c } else { 0x59 });
                    self.pushes(&what);
                }
                let descriptor = what.descriptor();
                let index = self.pool.field(&owner, name, &descriptor);
                if field.modifiers.static_ {
                    self.op2(0xb3, index);
                    self.pops(i32::from(what.width()));
                } else {
                    self.op2(0xb5, index);
                    self.pops(i32::from(what.width()) + 1);
                }
                Ok(what)
            }
            // `this.name = ...` and `other.name = ...`.
            Expression::Field { of, name } => {
                if matches!(of.as_ref(), Expression::This)
                    && self.unit.fields.iter().any(|held| held.name == *name)
                {
                    return self.assign(
                        &Expression::Name(name.clone()),
                        operator,
                        value,
                        line,
                        wanted,
                    );
                }
                let owner = self.value(of, line)?;
                let Type::Object(class) = owner.clone() else {
                    return Err(at(
                        "EJ212",
                        line,
                        1,
                        format!("A {} has no fields.", owner.readable()),
                    ));
                };
                let Some((holder, (_, what, static_))) = self.classpath.find_field(&class, name)
                else {
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
                        "Hand the class file that declares it over as a dependency.",
                    ));
                };
                let (holder, what, static_) = (holder.name.clone(), what.clone(), *static_);
                if static_ {
                    // The object it was named through is not wanted.
                    self.op(0x57);
                    self.pops(1);
                }
                if operator.is_some() {
                    return Err(unsupported(
                        line,
                        1,
                        "A compound assignment into a field of another object",
                    ));
                }
                let found = self.value(value, line)?;
                if !found.is_reference() {
                    self.convert(&found, &what, line)?;
                }
                if wanted {
                    return Err(unsupported(
                        line,
                        1,
                        "Using the value of an assignment into another object's field",
                    ));
                }
                let descriptor = what.descriptor();
                let index = self.pool.field(&holder, name, &descriptor);
                if static_ {
                    self.op2(0xb3, index);
                    self.pops(i32::from(what.width()));
                } else {
                    self.op2(0xb5, index);
                    self.pops(i32::from(what.width()) + 1);
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
                if let Some(operator) = operator {
                    // `a[i] += x` reads the element and writes it back, and
                    // the array and the index are worked out once. `dup2`
                    // keeps them for the write while the read uses them.
                    self.duplicates_two();
                    self.op(array_load(&element));
                    self.pops(2);
                    self.pushes(&element);
                    let now = self.binary_with(operator, &element, value, line)?;
                    if !now.is_reference() {
                        self.convert(&now, &element, line)?;
                    }
                    if wanted {
                        return Err(unsupported(
                            line,
                            1,
                            "Using the value of a compound assignment into an array",
                        ));
                    }
                    self.op(array_store(&element));
                    self.pops(2 + i32::from(element.width()));
                    return Ok(*element);
                }
                let given = self.value_for(value, &element, line)?;
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
                self.pops(2 + i32::from(element.width()));
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
            if let Expression::Index { .. } = target {
                if wanted {
                    return Err(unsupported(
                        line,
                        1,
                        "Using the value of a step on an array element",
                    ));
                }
                return self.assign(
                    target,
                    Some(if by > 0 {
                        Binary::Add
                    } else {
                        Binary::Subtract
                    }),
                    &Expression::Int(i64::from(by.abs())),
                    line,
                    false,
                );
            }
            if let Expression::Field { .. } = target {
                if wanted {
                    return Err(unsupported(
                        line,
                        1,
                        "Using the value of a step on another object's field",
                    ));
                }
                return self.assign(
                    target,
                    Some(if by > 0 {
                        Binary::Add
                    } else {
                        Binary::Subtract
                    }),
                    &Expression::Int(i64::from(by.abs())),
                    line,
                    false,
                );
            }
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
    // -- the loops and switches around here, and the `finally` bodies that
    // -- have to run on the way out of them.

    fn enter(&mut self, loops: bool) {
        let label = self.pending_label.take();
        self.levels.push(Level {
            label,
            loops,
            breaks: Vec::new(),
            continues: Vec::new(),
            finallys: self.finallys.len(),
        });
    }

    fn leave(&mut self) -> Level {
        self.levels.pop().expect("a level was entered")
    }

    /// Which level a `break` or a `continue` is talking about.
    fn level_for(&self, label: Option<&str>, loops: bool) -> Option<usize> {
        self.levels.iter().rposition(|level| match label {
            Some(name) => level.label.as_deref() == Some(name) && (!loops || level.loops),
            None => !loops || level.loops,
        })
    }

    fn no_such_level(&self, label: Option<&str>, line: u32, word: &str) -> Diagnostic {
        match label {
            Some(name) => at(
                "EJ231",
                line,
                1,
                format!("`{word} {name};` names a label that is not around this."),
            ),
            None if word == "continue" => at("EJ231", line, 1, "`continue` is not inside a loop."),
            None => at(
                "EJ231",
                line,
                1,
                "`break` is not inside a loop or a `switch`.",
            ),
        }
    }

    /// Writes out the `finally` bodies between here and where control is going.
    ///
    /// Innermost first, and while one runs it is not itself pending -- a
    /// `return` inside a `finally` must not send that same `finally` round
    /// again. Where the copy landed is recorded, because a handler that
    /// protects the `try` must not protect this copy of its `finally`.
    fn run_finallys(&mut self, down_to: usize) -> Result<(), Diagnostic> {
        if self.finallys.len() <= down_to {
            return Ok(());
        }
        let pending = self.finallys.clone();
        let began = self.code.len();
        for index in (down_to..pending.len()).rev() {
            self.finallys.truncate(index);
            for one in &pending[index] {
                self.statement(one)?;
            }
        }
        self.finallys = pending;
        if self.code.len() > began {
            self.inlined.push((began, self.code.len(), down_to));
        }
        Ok(())
    }

    /// Says that the stack is exactly this deep, which is what entering an
    /// exception handler does regardless of how deep it was before.
    fn set_depth(&mut self, depth: i32) {
        // Only ever used to say the stack is empty; anything else has to say
        // what is on it.
        debug_assert_eq!(depth, 0);
        self.stack.clear();
        self.deepest();
    }

    /// Adds the exception-table rows protecting one stretch of code, leaving
    /// out any inlined `finally` inside it.
    fn protect(
        &mut self,
        start: usize,
        end: usize,
        target: usize,
        class: Option<String>,
        depth: usize,
    ) {
        let mut pieces = vec![(start, end)];
        let inlined = self.inlined.clone();
        for (from, to, ran_from) in inlined {
            // A copy that ran this `try`'s own `finally` is not inside this
            // `try` any more; a copy of something further in still is.
            if ran_from > depth {
                continue;
            }
            let mut next = Vec::new();
            for (piece_start, piece_end) in pieces {
                if to <= piece_start || from >= piece_end {
                    next.push((piece_start, piece_end));
                    continue;
                }
                if piece_start < from {
                    next.push((piece_start, from));
                }
                if to < piece_end {
                    next.push((to, piece_end));
                }
            }
            pieces = next;
        }
        for (piece_start, piece_end) in pieces {
            if piece_start >= piece_end {
                continue;
            }
            self.handlers.push(Handler {
                start: piece_start,
                end: piece_end,
                target,
                class: class.clone(),
            });
        }
    }

    fn statement(&mut self, statement: &Positioned<Statement>) -> Result<(), Diagnostic> {
        let line = statement.line;
        match &statement.node {
            Statement::Nothing => Ok(()),
            Statement::Several(inside) => {
                // Written as one declaration and meaning several, which is not
                // the same as a block: `int a = 1, b = 2;` puts both names in
                // the scope around it.
                for one in inside {
                    self.statement(one)?;
                }
                Ok(())
            }
            Statement::Chain { .. } => Err(at(
                "EJ242",
                line,
                1,
                "`super(...)` and `this(...)` may only be the first statement of a constructor.",
            )),
            Statement::Block(inside) => {
                self.open();
                for one in inside {
                    self.statement(one)?;
                }
                self.close();
                Ok(())
            }
            Statement::Declare { what, name, value } => {
                // `var` has no type of its own, so the value has to be worked
                // out before there is anything to check it against.
                if matches!(what, Written::Inferred) {
                    let Some(expression) = value else {
                        return Err(self.resolve(what, line).unwrap_err());
                    };
                    let found = self.value(expression, line)?;
                    if found == Type::Void {
                        return Err(at(
                            "EJ234",
                            line,
                            1,
                            format!(
                                "`{name}` was given the result of something that returns nothing."
                            ),
                        ));
                    }
                    if matches!(expression, Expression::Null) {
                        return Err(at(
                            "EJ234",
                            line,
                            1,
                            format!("`var {name} = null` says nothing about what it holds."),
                        )
                        .with_suggestion("Write the type."));
                    }
                    let slot = self.declare(name, found.clone());
                    self.store(slot, &found);
                    return Ok(());
                }
                let target = self.resolve(what, line)?;
                match value {
                    Some(expression) => {
                        let found = self.value_for(expression, &target, line)?;
                        if !self.fit(&found, &target, line)? {
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
                        self.pops(1);
                    }
                    _ => {
                        self.op(0x58);
                        self.pops(2);
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
                self.pops(1);
                let to_else = self.jump(0x99);
                self.statement(then)?;
                match otherwise {
                    Some(otherwise) => {
                        // Where the `then` side always returns, the jump over
                        // the `else` can never be taken. Writing it anyway
                        // leaves an instruction nothing reaches, and an
                        // instruction nothing reaches begins a block the
                        // verifier wants a frame for -- so a version 69 class
                        // file is refused with "expecting a stack map frame"
                        // over three bytes that do nothing.
                        let over = (!never_completes(&then.node)).then(|| self.jump(0xa7));
                        self.land(to_else);
                        self.a_branch_lands_here();
                        self.statement(otherwise)?;
                        if let Some(over) = over {
                            self.land(over);
                            self.a_branch_lands_here();
                        }
                    }
                    None => {
                        self.land(to_else);
                        self.a_branch_lands_here();
                    }
                }
                Ok(())
            }
            Statement::While { condition, body } => {
                let top = self.code.len();
                // A loop jumps back here, so the verifier has to be told what
                // is true at the top as well as after the end.
                self.a_branch_lands_here();
                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at("EJ206", line, 1, "A `while` wants a boolean."));
                }
                self.pops(1);
                let out = self.jump(0x99);
                self.enter(true);
                self.statement(body)?;
                let level = self.leave();
                for pending in level.continues {
                    self.land(pending);
                }
                self.a_branch_lands_here();
                self.jump_back(0xa7, top);
                self.land(out);
                for pending in level.breaks {
                    self.land(pending);
                }
                self.a_branch_lands_here();
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
                self.a_branch_lands_here();
                let out = match condition {
                    Some(condition) => {
                        let found = self.value(condition, line)?;
                        if found != Type::Boolean {
                            return Err(at("EJ206", line, 1, "A `for` condition is a boolean."));
                        }
                        self.pops(1);
                        let jump = self.jump(0x99);
                        Some(jump)
                    }
                    None => None,
                };
                self.enter(true);
                self.statement(body)?;
                let level = self.leave();
                for pending in level.continues {
                    self.land(pending);
                }
                self.a_branch_lands_here();
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
                                    self.pops(1);
                                }
                                _ => {
                                    self.op(0x58);
                                    self.pops(2);
                                }
                            }
                        }
                    }
                }
                self.jump_back(0xa7, top);
                if let Some(out) = out {
                    self.land(out);
                }
                for pending in level.breaks {
                    self.land(pending);
                }
                self.a_branch_lands_here();
                self.close();
                Ok(())
            }
            Statement::DoWhile { body, condition } => {
                let top = self.code.len();
                self.a_branch_lands_here();
                self.enter(true);
                self.statement(body)?;
                // `continue` in a `do` block goes to the test, not to the top:
                // the body has already run once and the question is whether it
                // runs again.
                let level = self.leave();
                for pending in level.continues {
                    self.land(pending);
                }
                self.a_branch_lands_here();
                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at("EJ206", line, 1, "A `do`/`while` wants a boolean."));
                }
                // ifne, so the loop runs again when the condition holds.
                self.jump_back(0x9a, top);
                self.pops(1);
                for pending in level.breaks {
                    self.land(pending);
                }
                self.a_branch_lands_here();
                Ok(())
            }
            Statement::ForEach {
                what,
                name,
                over,
                body,
            } => self.for_each(what, name, over, body, line),
            Statement::Switch { subject, arms } => self.switch(subject, arms, line),
            Statement::Try {
                body,
                catches,
                finally,
            } => self.try_catch(body, catches, finally.as_deref(), line),
            Statement::Yield(what) => {
                let Some(level) = self.yields.len().checked_sub(1) else {
                    return Err(at(
                        "EJ244",
                        line,
                        1,
                        "`yield` is not inside a `switch` used for its value.",
                    ));
                };
                let found = self.value(what, line)?;
                self.agree_on_yield(level, &found, line)?;
                let down_to = self.yields[level].finallys;
                self.run_finallys(down_to)?;
                let jump = self.jump(0xa7);
                self.yields[level].pending.push(jump);
                Ok(())
            }
            Statement::Synchronized { on, body } => {
                // The lock is taken, the block runs, and the lock is given
                // back however the block is left -- including by throwing,
                // which is what the handler covering the whole of it is for.
                // A lock not given back is a device that stops.
                let found = self.value(on, line)?;
                if !found.is_reference() {
                    return Err(at(
                        "EJ253",
                        line,
                        1,
                        format!(
                            "`synchronized` locks an object, and was given a {}.",
                            found.readable()
                        ),
                    ));
                }
                self.open();
                let held = self.declare("$locked", found.clone());
                self.duplicates();
                self.store(held, &found);
                self.op(0xc2); // monitorenter
                self.pops(1);

                let begun = self.code.len();
                for one in body {
                    self.statement(one)?;
                }
                let leaves = body.iter().any(|one| never_completes(&one.node));
                if !leaves {
                    self.load(held, &found);
                    self.op(0xc3); // monitorexit
                    self.pops(1);
                }
                let ended = self.code.len();

                let over = (!leaves).then(|| self.jump(0xa7));

                // Whatever was thrown, the lock is given back and the throw
                // carries on.
                let target = self.code.len();
                self.protect(begun, ended, target, None, self.finallys.len());
                let throwable = Type::Object("java/lang/Throwable".to_string());
                self.stack_is(vec![Verified::of(&throwable)]);
                self.a_branch_lands_here();
                let thrown = self.declare("$thrown", throwable.clone());
                self.store(thrown, &throwable);
                self.load(held, &found);
                self.op(0xc3);
                self.pops(1);
                self.load(thrown, &throwable);
                self.op(0xbf);
                self.set_depth(0);

                self.close();
                if let Some(over) = over {
                    self.land(over);
                    self.set_depth(0);
                    self.a_branch_lands_here();
                }
                Ok(())
            }
            Statement::Assert { condition, said } => {
                // `assert` runs only where the runtime was asked for it, which
                // is what the flag the class initialiser works out is for. On
                // Android nobody asks, so this is a field read and a branch
                // that is never taken -- which is exactly what `javac` writes,
                // and exactly what the person expects.
                let index = self
                    .pool
                    .field(&self.this_class.clone(), ASSERTIONS_OFF, "Z");
                self.op2(0xb2, index);
                self.pushes(&Type::Boolean);
                self.pops(1);
                let over = self.jump(0x9a); // ifne: off, so skip

                let found = self.value(condition, line)?;
                if found != Type::Boolean {
                    return Err(at(
                        "EJ206",
                        line,
                        1,
                        format!(
                            "An `assert` wants a boolean and was given a {}.",
                            found.readable()
                        ),
                    ));
                }
                self.pops(1);
                let held = self.jump(0x9a); // ifne: it held, so nothing to do

                let error = self.pool.class("java/lang/AssertionError");
                self.op2(0xbb, error);
                self.pushes(&Type::Object("java/lang/AssertionError".to_string()));
                self.duplicates();
                let descriptor = match said {
                    Some(expression) => {
                        let what = self.value(expression, line)?;
                        // Every shape of AssertionError's constructor takes
                        // one thing; which one depends on what was written.
                        match &what {
                            Type::Boolean => "(Z)V",
                            Type::Char => "(C)V",
                            Type::Int | Type::Byte | Type::Short => {
                                self.convert(&what, &Type::Int, line)?;
                                "(I)V"
                            }
                            Type::Long => "(J)V",
                            Type::Float => "(F)V",
                            Type::Double => "(D)V",
                            _ => "(Ljava/lang/Object;)V",
                        }
                    }
                    None => "()V",
                };
                let init =
                    self.pool
                        .method("java/lang/AssertionError", "<init>", descriptor, false);
                self.op2(0xb7, init);
                let taken: i32 = read_descriptor(descriptor)
                    .map(|(parameters, _)| {
                        parameters.iter().map(|one| i32::from(one.width())).sum()
                    })
                    .unwrap_or(0);
                self.pops(taken + 1);
                self.op(0xbf);
                self.set_depth(0);

                self.land(over);
                self.land(held);
                self.set_depth(0);
                self.a_branch_lands_here();
                self.wants_assertions = true;
                Ok(())
            }
            Statement::Throw(what) => {
                let found = self.value(what, line)?;
                if !found.is_reference() {
                    return Err(at(
                        "EJ235",
                        line,
                        1,
                        format!(
                            "`throw` wants a Throwable and was given a {}.",
                            found.readable()
                        ),
                    ));
                }
                self.op(0xbf);
                // Nothing after an athrow is reached from here, and the stack
                // it left behind is not the stack anything else will find.
                self.set_depth(0);
                Ok(())
            }
            Statement::Labelled { label, body } => {
                // A label on a loop or a switch belongs to it, so that
                // `continue name` has somewhere to land. A label on anything
                // else gets a level of its own that only `break` can reach.
                if matches!(
                    &body.node,
                    Statement::While { .. }
                        | Statement::DoWhile { .. }
                        | Statement::For { .. }
                        | Statement::ForEach { .. }
                        | Statement::Switch { .. }
                ) {
                    self.pending_label = Some(label.clone());
                    return self.statement(body);
                }
                self.pending_label = Some(label.clone());
                self.enter(false);
                self.statement(body)?;
                let level = self.leave();
                if !level.breaks.is_empty() {
                    for pending in level.breaks {
                        self.land(pending);
                    }
                    self.a_branch_lands_here();
                }
                Ok(())
            }
            Statement::Break(label) => {
                let Some(index) = self.level_for(label.as_deref(), false) else {
                    return Err(self.no_such_level(label.as_deref(), line, "break"));
                };
                self.run_finallys(self.levels[index].finallys)?;
                let jump = self.jump(0xa7);
                self.levels[index].breaks.push(jump);
                Ok(())
            }
            Statement::Continue(label) => {
                let Some(index) = self.level_for(label.as_deref(), true) else {
                    return Err(self.no_such_level(label.as_deref(), line, "continue"));
                };
                self.run_finallys(self.levels[index].finallys)?;
                let jump = self.jump(0xa7);
                self.levels[index].continues.push(jump);
                Ok(())
            }
            Statement::Return(value) => {
                let wanted = self.returns.clone();
                match value {
                    None => {
                        // Nothing is on the stack, so the pending `finally`
                        // blocks can simply run here.
                        self.run_finallys(0)?;
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
                        let found = self.value_for(expression, &wanted, line)?;
                        if !self.fit(&found, &wanted, line)? {
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
                        // The value is worked out before the `finally` runs,
                        // because that is when the expression was written --
                        // but the `finally` needs the stack, so the value
                        // waits in a slot of its own until it is time to go.
                        if !self.finallys.is_empty() {
                            let held = self.declare("$returning", wanted.clone());
                            self.store(held, &wanted);
                            self.run_finallys(0)?;
                            self.load(held, &wanted);
                        }
                        let opcode = match &wanted {
                            Type::Long => 0xadu8,
                            Type::Float => 0xae,
                            Type::Double => 0xaf,
                            other if other.is_reference() => 0xb0,
                            _ => 0xac,
                        };
                        self.op(opcode);
                        self.pops(i32::from(wanted.width()));
                    }
                }
                Ok(())
            }
        }
    }
}

// ------------------------------------------------------ the class file

/// The StackMapTable a method's frames come to.
///
/// Every frame is written as a `full_frame`, which spells out every local and
/// every stack slot rather than saying how this one differs from the last. The
/// compact forms save a few bytes and each one is a separate chance to describe
/// a state that is not the state; a verifier that rejects a class file gives no
/// hint which frame it disbelieved. The bytes are cheap and being right is not.
fn stack_map_table(frames: &[Frame], pool: &mut Pool) -> Option<Vec<u8>> {
    if frames.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Frame> = frames.iter().collect();
    sorted.sort_by_key(|frame| frame.at);

    let mut body = Vec::new();
    body.extend_from_slice(&(sorted.len() as u16).to_be_bytes());

    // Offsets are written as the gap since the last frame, and the first gap is
    // measured from the start rather than from one before it -- so every frame
    // after the first is one less than the plain difference. Getting that wrong
    // shifts every frame by one instruction and the verifier rejects the lot.
    let mut previous: Option<usize> = None;
    for frame in sorted {
        let delta = match previous {
            None => frame.at,
            Some(before) => frame.at - before - 1,
        };
        previous = Some(frame.at);

        body.push(255); // full_frame
        body.extend_from_slice(&(delta as u16).to_be_bytes());
        body.extend_from_slice(&(frame.locals.len() as u16).to_be_bytes());
        for local in &frame.locals {
            local.write(&mut body, pool);
        }
        body.extend_from_slice(&(frame.stack.len() as u16).to_be_bytes());
        for held in &frame.stack {
            held.write(&mut body, pool);
        }
    }
    Some(body)
}

/// Whether every way out of this statement is a `return`.
///
/// The first version of this asked only whether the last statement in a method
/// was one. That is true of most methods and false of every method ending in an
/// `if` with a `return` down both sides -- so `if (x) return a; else return b;`
/// was refused for reaching its end without returning, which it cannot do.
///
/// This is the specification's "can complete normally", narrowed to what is
/// compiled here. Where it is not sure it says no, and the worst that costs is
/// a `return` written after code that cannot reach it.
fn never_completes(statement: &Statement) -> bool {
    match statement {
        Statement::Return(_) | Statement::Throw(_) => true,
        // A `break` or a `continue` does not fall through to what is written
        // after it either. Whether it is allowed at all is the emitter's
        // question, and it answers it.
        Statement::Break(_) | Statement::Continue(_) => true,
        Statement::Block(inside) => inside.iter().any(|one| never_completes(&one.node)),
        Statement::If {
            then,
            otherwise: Some(otherwise),
            ..
        } => never_completes(&then.node) && never_completes(&otherwise.node),
        // A loop with no way to fail its test runs until something inside
        // leaves it, and a `break` is the only way out that is not a `return`
        // or a `throw`.
        Statement::For {
            condition: None,
            body,
            ..
        } => !holds_a_break(&body.node),
        Statement::For {
            condition: Some(condition),
            body,
            ..
        }
        | Statement::While { condition, body } => {
            matches!(condition, Expression::Boolean(true)) && !holds_a_break(&body.node)
        }
        Statement::DoWhile { body, condition } => {
            matches!(condition, Expression::Boolean(true)) && !holds_a_break(&body.node)
        }
        // A lock is taken and given back around the block; what the block does
        // is what decides whether anything gets past it.
        Statement::Synchronized { body, .. } => body.iter().any(|one| never_completes(&one.node)),
        // `assert` is a check, and a check that holds carries on.
        Statement::Assert { .. } => false,
        // A `try` gets past only if something that could get past it does: the
        // body or one of the handlers. A `finally` that never completes ends
        // it whatever the rest did.
        Statement::Try {
            body,
            catches,
            finally,
        } => {
            if finally
                .as_ref()
                .is_some_and(|inside| inside.iter().any(|one| never_completes(&one.node)))
            {
                return true;
            }
            body.iter().any(|one| never_completes(&one.node))
                && catches
                    .iter()
                    .all(|catch| catch.body.iter().any(|one| never_completes(&one.node)))
        }
        // A `switch` gets past its own end if there is a value nothing
        // answers to, if a `break` leaves it, or if the last thing it can run
        // reaches the end.
        //
        // Which arm that is depends on the form. With `:` an arm runs into the
        // next one, so the only one that can reach the end is the last -- and
        // an empty arm in the middle is a label on the one after it, not a way
        // out. With `->` every arm ends by jumping past the rest, so every one
        // of them has to be checked.
        Statement::Switch { arms, .. } => {
            if arms.is_empty() || !arms.iter().any(|arm| arm.labels.is_empty()) {
                return false;
            }
            if arms
                .iter()
                .any(|arm| arm.body.iter().any(|one| holds_a_break(&one.node)))
            {
                return false;
            }
            if arms.iter().any(|arm| arm.arrow) {
                return arms
                    .iter()
                    .all(|arm| arm.body.iter().any(|one| never_completes(&one.node)));
            }
            arms.last()
                .is_some_and(|arm| arm.body.iter().any(|one| never_completes(&one.node)))
        }
        _ => false,
    }
}

/// Whether a `break` inside this statement could leave the loop or switch
/// around it.
///
/// A `break` in a nested loop belongs to that loop, which is why this stops
/// descending at one. A labelled `break` is counted whatever it names, because
/// working out where it lands would need the labels around it and being wrong
/// in this direction only costs an instruction nothing reaches.
/// Whether a `yield` inside this statement belongs to the switch around it.
///
/// A `yield` in a nested switch expression belongs to that one, so this stops
/// descending at one -- the same rule `break` follows for loops.
fn holds_a_yield(statement: &Statement) -> bool {
    match statement {
        Statement::Yield(_) => true,
        Statement::Block(inside) | Statement::Several(inside) => {
            inside.iter().any(|one| holds_a_yield(&one.node))
        }
        Statement::Labelled { body, .. } => holds_a_yield(&body.node),
        Statement::If {
            then, otherwise, ..
        } => {
            holds_a_yield(&then.node)
                || otherwise
                    .as_ref()
                    .is_some_and(|otherwise| holds_a_yield(&otherwise.node))
        }
        Statement::While { body, .. }
        | Statement::DoWhile { body, .. }
        | Statement::For { body, .. }
        | Statement::ForEach { body, .. } => holds_a_yield(&body.node),
        Statement::Try {
            body,
            catches,
            finally,
        } => {
            body.iter().any(|one| holds_a_yield(&one.node))
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(|one| holds_a_yield(&one.node)))
                || finally
                    .as_ref()
                    .is_some_and(|inside| inside.iter().any(|one| holds_a_yield(&one.node)))
        }
        _ => false,
    }
}

fn holds_a_break(statement: &Statement) -> bool {
    match statement {
        Statement::Break(_) => true,
        Statement::Block(inside) => inside.iter().any(|one| holds_a_break(&one.node)),
        Statement::Labelled { body, .. } => holds_a_break(&body.node),
        Statement::If {
            then, otherwise, ..
        } => {
            holds_a_break(&then.node)
                || otherwise
                    .as_ref()
                    .is_some_and(|otherwise| holds_a_break(&otherwise.node))
        }
        Statement::Try {
            body,
            catches,
            finally,
        } => {
            body.iter().any(|one| holds_a_break(&one.node))
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(|one| holds_a_break(&one.node)))
                || finally
                    .as_ref()
                    .is_some_and(|inside| inside.iter().any(|one| holds_a_break(&one.node)))
        }
        _ => false,
    }
}

impl Emitter<'_> {
    /// `for (T name : array)`.
    ///
    /// The array and the index live in two slots of their own so that the body
    /// cannot reach them and nothing it does to `name` can be seen by the next
    /// turn. That is what Java says this means, and it is also the only way to
    /// write it without evaluating the array once per element.
    fn for_each(
        &mut self,
        what: &Written,
        name: &str,
        over: &Expression,
        body: &Positioned<Statement>,
        line: u32,
    ) -> Result<(), Diagnostic> {
        // Which of the two loops this is depends on what is being walked
        // over, and that is known only by working it out. It is worked out
        // here and the code for it thrown away, so that whichever loop is
        // chosen writes it once -- writing it here and again inside was how
        // the first version of this pushed the collection twice.
        let found = self.type_of(over, line)?;
        let Type::Array(element) = found.clone() else {
            // Not an array, so it has to hand over an iterator.
            return self.for_each_over_an_iterator(what, name, over, body, &found, line);
        };

        self.open();
        let found = self.value(over, line)?;

        let declared = match what {
            Written::Inferred => (*element).clone(),
            other => self.resolve(other, line)?,
        };
        if !element.may_be_given_to(&declared) {
            return Err(at(
                "EJ237",
                line,
                1,
                format!(
                    "`{name}` is a {} and the array holds {}.",
                    declared.readable(),
                    element.readable()
                ),
            ));
        }

        let array = self.declare("$over", found.clone());
        self.store(array, &found);
        self.op(0x03);
        self.pushes(&Type::Int);
        let index = self.declare("$at", Type::Int);
        self.store(index, &Type::Int);

        let top = self.code.len();
        self.a_branch_lands_here();
        self.load(index, &Type::Int);
        self.load(array, &found);
        self.op(0xbe); // arraylength
        self.pops(2);
        let out = self.jump(0xa2); // if_icmpge

        self.open();
        self.load(array, &found);
        self.load(index, &Type::Int);
        self.op(array_load(&element));
        self.pops(2);
        self.pushes(&element);
        if declared != *element {
            self.convert(&element, &declared, line)?;
        }
        let held = self.declare(name, declared.clone());
        self.store(held, &declared);

        self.enter(true);
        self.statement(body)?;
        let level = self.leave();
        self.close();

        for pending in level.continues {
            self.land(pending);
        }
        self.a_branch_lands_here();
        self.bump_local(index, 1);
        self.jump_back(0xa7, top);
        self.land(out);
        for pending in level.breaks {
            self.land(pending);
        }
        self.close();
        self.a_branch_lands_here();
        Ok(())
    }

    /// A `switch`, over an integer or over a String.
    ///
    /// The integer form becomes a real switch instruction. Which of the two
    /// the JVM has is decided by how tightly the labels are packed:
    /// `tableswitch` is a jump table, so it costs four bytes per value in the
    /// range whether or not anything answers to it, and `lookupswitch` is a
    /// sorted list the JVM searches. Dense labels take the table; scattered
    /// ones take the list.
    ///
    /// The String form becomes a chain of `equals`. It is what a switch over
    /// strings means, it throws where Java says it throws -- on a null subject
    /// -- and it does not need the two-pass hashCode dance `javac` writes to
    /// save comparisons in switches far larger than anybody writes by hand.
    fn switch(&mut self, subject: &Expression, arms: &[Arm], line: u32) -> Result<(), Diagnostic> {
        let found = self.value(subject, line)?;
        let over = self.what_a_switch_is_over(&found, arms, line)?;

        self.open();
        self.enter(false);
        let mut ends: Vec<Pending> = Vec::new();
        let mut targets: Vec<usize> = Vec::new();

        let dispatch = self.dispatch_for(over, arms, line)?;

        // The arms, in the order they were written, because that is the order
        // one falls into the next.
        let mut default_at: Option<usize> = None;
        for arm in arms {
            let at_here = self.code.len();
            targets.push(at_here);
            if arm.labels.is_empty() {
                default_at = Some(at_here);
            }
            self.set_depth(0);
            self.a_branch_lands_here();
            self.open();
            for one in &arm.body {
                self.statement(one)?;
            }
            self.close();
            // An arrow arm never runs into the next one.
            if arm.arrow && !arm.body.iter().any(|one| never_completes(&one.node)) {
                ends.push(self.jump(0xa7));
            }
        }

        let after = self.code.len();
        dispatch.settle(self, &targets, default_at.unwrap_or(after));
        for pending in ends {
            self.land(pending);
        }
        let level = self.leave();
        for pending in level.breaks {
            self.land(pending);
        }
        self.close();
        self.set_depth(0);
        self.a_branch_lands_here();
        Ok(())
    }

    /// A `switch` used for its value.
    ///
    /// The difference from the statement is what is on the stack at the end.
    /// Every arm has to leave exactly one value there and jump past the rest,
    /// or leave by throwing; anything else and the verifier would find two
    /// paths arriving at the same instruction disagreeing about what is on the
    /// stack, which is the one thing a frame cannot describe.
    fn switch_value(
        &mut self,
        subject: &Expression,
        arms: &[Arm],
        line: u32,
    ) -> Result<Type, Diagnostic> {
        if !arms.iter().any(|arm| arm.labels.is_empty()) {
            return Err(at(
                "EJ245",
                line,
                1,
                "A `switch` used for its value needs a `default`, because it has to have \
                 an answer for anything.",
            ));
        }

        let found = self.value(subject, line)?;
        let over = self.what_a_switch_is_over(&found, arms, line)?;

        self.open();
        self.yields.push(Yielding {
            pending: Vec::new(),
            produced: None,
            finallys: self.finallys.len(),
        });
        let level = self.yields.len() - 1;

        let dispatch = self.dispatch_for(over, arms, line)?;

        // What is under the switch, read after the dispatch rather than
        // before: dispatching is what takes the subject back off the stack,
        // and an arm starts from what is left once it has gone.
        let beneath = self.stack.clone();

        let mut targets: Vec<usize> = Vec::new();
        let mut default_at: Option<usize> = None;
        for arm in arms {
            let at_here = self.code.len();
            targets.push(at_here);
            if arm.labels.is_empty() {
                default_at = Some(at_here);
            }
            self.stack_is(beneath.clone());
            self.a_branch_lands_here();
            self.open();

            // `case 1 -> 2;` is the value; anything else has to say `yield`.
            let bare = match (arm.arrow, arm.body.len(), arm.body.first()) {
                (true, 1, Some(one)) => match &one.node {
                    Statement::Express(expression) => Some(expression.clone()),
                    _ => None,
                },
                _ => None,
            };
            match bare {
                Some(expression) => {
                    let produced = self.value(&expression, arm.line)?;
                    self.agree_on_yield(level, &produced, arm.line)?;
                    let down_to = self.yields[level].finallys;
                    self.run_finallys(down_to)?;
                    let jump = self.jump(0xa7);
                    self.yields[level].pending.push(jump);
                }
                None => {
                    for one in &arm.body {
                        self.statement(one)?;
                    }
                    if !arm.body.iter().any(|one| never_completes(&one.node))
                        && !arm.body.iter().any(|one| holds_a_yield(&one.node))
                    {
                        return Err(at(
                            "EJ246",
                            arm.line,
                            arm.column,
                            "Every arm of a `switch` used for its value has to produce one.",
                        )
                        .with_suggestion("Write `yield`, or `throw`."));
                    }
                }
            }
            self.close();
        }

        let after = self.code.len();
        dispatch.settle(self, &targets, default_at.unwrap_or(after));

        let level = self.yields.pop().expect("the level was pushed");
        for pending in level.pending {
            self.land(pending);
        }
        self.close();

        let Some(produced) = level.produced else {
            // Every arm left by throwing, so nothing arrives here at all.
            return Err(at(
                "EJ246",
                line,
                1,
                "No arm of this `switch` produces a value.",
            ));
        };
        let mut settled = beneath;
        let held = Verified::of(&produced);
        let wide = held.is_wide();
        settled.push(held);
        if wide {
            settled.push(Verified::Top);
        }
        self.stack_is(settled);
        self.a_branch_lands_here();
        Ok(produced)
    }

    /// Checks that every arm of a switch expression produces the same type,
    /// and remembers what that type is.
    fn agree_on_yield(&mut self, level: usize, found: &Type, line: u32) -> Result<(), Diagnostic> {
        match self.yields[level].produced.clone() {
            None => {
                self.yields[level].produced = Some(found.clone());
                Ok(())
            }
            Some(wanted) if *found == wanted => Ok(()),
            Some(wanted) if found.may_be_given_to(&wanted) => {
                if !found.is_reference() {
                    self.convert(found, &wanted, line)?;
                }
                Ok(())
            }
            Some(wanted) => Err(at(
                "EJ247",
                line,
                1,
                format!(
                    "One arm of this `switch` produces a {} and another a {}.",
                    wanted.readable(),
                    found.readable()
                ),
            )
            .with_suggestion("Every arm has to produce the same type. A cast settles it.")),
        }
    }

    /// What kind of thing a `switch` is choosing on.
    fn what_a_switch_is_over(
        &mut self,
        found: &Type,
        arms: &[Arm],
        line: u32,
    ) -> Result<Chooser, Diagnostic> {
        if found.is_int_like() {
            return Ok(Chooser::Integer);
        }
        if *found == Type::Object("java/lang/String".to_string()) {
            return Ok(Chooser::Text);
        }
        // An enum's constants are static fields of the enum's own type, so a
        // label naming one is a name the class holds. That is true of an enum
        // written here and of one handed over as a class file, which is why it
        // is asked of the classpath rather than of a flag.
        if let Type::Object(class) = found {
            let every =
                arms.iter().flat_map(|arm| arm.labels.iter()).all(|label| {
                    let Expression::Name(named) = label else {
                        return false;
                    };
                    self.classpath.find_field(class, named).is_some_and(
                        |(_, (_, what, static_))| *static_ && *what == Type::Object(class.clone()),
                    )
                });
            if every {
                return Ok(Chooser::Constant(class.clone()));
            }
        }
        Err(at(
            "EJ238",
            line,
            1,
            format!(
                "A `switch` takes an integer, a String or an enum, and was given a {}.",
                found.readable()
            ),
        ))
    }

    fn dispatch_for(
        &mut self,
        over: Chooser,
        arms: &[Arm],
        line: u32,
    ) -> Result<Dispatch, Diagnostic> {
        match over {
            Chooser::Integer => self.integer_dispatch(arms, line),
            Chooser::Text => self.text_dispatch(arms, line),
            Chooser::Constant(class) => self.constant_dispatch(&class, arms, line),
        }
    }

    /// A chain of `==`, one per constant.
    ///
    /// An enum's constants are the only instances of their class that exist,
    /// so identity is what a `case` means. `javac` writes a table of ordinals
    /// instead, which is faster for a switch far larger than anybody writes by
    /// hand and needs a synthetic class of its own to hold the table.
    fn constant_dispatch(
        &mut self,
        class: &str,
        arms: &[Arm],
        line: u32,
    ) -> Result<Dispatch, Diagnostic> {
        let what = Type::Object(class.to_string());
        let held = self.declare("$switch", what.clone());
        self.store(held, &what);

        let mut seen: Vec<String> = Vec::new();
        let mut waiting: Vec<(Pending, usize)> = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            for label in &arm.labels {
                let Expression::Name(named) = label else {
                    return Err(at(
                        "EJ239",
                        arm.line,
                        arm.column,
                        "A `case` in a `switch` over an enum names one of its constants.",
                    ));
                };
                if seen.contains(named) {
                    return Err(at(
                        "EJ240",
                        arm.line,
                        arm.column,
                        format!("`case {named}` is written twice in one `switch`."),
                    ));
                }
                seen.push(named.clone());

                self.load(held, &what);
                self.read_static_field(class, named, arm.line)?;
                // if_acmpeq
                self.pops(2);
                waiting.push((self.jump(0xa5), index));
            }
        }
        let _ = line;
        let fallthrough = self.jump(0xa7);
        Ok(Dispatch::Chain {
            waiting,
            fallthrough,
        })
    }

    /// Writes the switch instruction, with the offsets left to be filled in
    /// once the arms have been written and their positions are known.
    fn integer_dispatch(&mut self, arms: &[Arm], line: u32) -> Result<Dispatch, Diagnostic> {
        // Which arm answers to which value.
        let mut keys: Vec<(i32, usize)> = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            for label in &arm.labels {
                let value = constant_int(label).ok_or_else(|| {
                    at(
                        "EJ239",
                        arm.line,
                        arm.column,
                        "A `case` over an integer wants a constant this compiler can read.",
                    )
                    .with_suggestion("A number, a character, or a `-` in front of one.")
                })?;
                if keys.iter().any(|(held, _)| *held == value) {
                    return Err(at(
                        "EJ240",
                        arm.line,
                        arm.column,
                        format!("`case {value}` is written twice in one `switch`."),
                    ));
                }
                keys.push((value, index));
            }
        }
        keys.sort_by_key(|(value, _)| *value);
        let _ = line;

        let opcode_at = self.code.len();
        let low = keys.first().map(|(value, _)| *value).unwrap_or(0);
        let high = keys.last().map(|(value, _)| *value).unwrap_or(0);
        let span = i64::from(high) - i64::from(low) + 1;
        // A jump table costs four bytes for every value in the range. It is
        // worth that when most of them are used and wasteful when they are
        // not.
        let table = !keys.is_empty() && span <= 2 * keys.len() as i64 + 8;

        self.op(if table { 0xaa } else { 0xab });
        self.pops(1);
        while !self.code.len().is_multiple_of(4) {
            self.code.push(0);
        }

        let default_slot = self.code.len();
        self.code.extend_from_slice(&[0; 4]);
        let mut slots: Vec<(usize, usize)> = Vec::new();
        if table {
            self.code.extend_from_slice(&low.to_be_bytes());
            self.code.extend_from_slice(&high.to_be_bytes());
            for value in low..=high {
                let slot = self.code.len();
                self.code.extend_from_slice(&[0; 4]);
                match keys.iter().find(|(held, _)| *held == value) {
                    Some((_, arm)) => slots.push((slot, *arm)),
                    // A hole in the range goes to the default, and which
                    // instruction that is is not known yet.
                    None => slots.push((slot, usize::MAX)),
                }
                if value == high {
                    break;
                }
            }
        } else {
            self.code
                .extend_from_slice(&(keys.len() as u32).to_be_bytes());
            for (value, arm) in &keys {
                self.code.extend_from_slice(&value.to_be_bytes());
                let slot = self.code.len();
                self.code.extend_from_slice(&[0; 4]);
                slots.push((slot, *arm));
            }
        }

        Ok(Dispatch::Instruction {
            opcode_at,
            default_slot,
            slots,
        })
    }

    /// A chain of `equals`, one per label, and a jump to the default at the
    /// end of it.
    fn text_dispatch(&mut self, arms: &[Arm], line: u32) -> Result<Dispatch, Diagnostic> {
        let text = Type::Object("java/lang/String".to_string());
        let held = self.declare("$switch", text.clone());
        self.store(held, &text);

        let mut seen: Vec<String> = Vec::new();
        let mut waiting: Vec<(Pending, usize)> = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            for label in &arm.labels {
                let Expression::Str(value) = label else {
                    return Err(at(
                        "EJ239",
                        arm.line,
                        arm.column,
                        "A `case` in a `switch` over a String wants a String constant.",
                    ));
                };
                if seen.contains(value) {
                    return Err(at(
                        "EJ240",
                        arm.line,
                        arm.column,
                        format!("`case {value:?}` is written twice in one `switch`."),
                    ));
                }
                seen.push(value.clone());

                self.load(held, &text);
                self.push_string(value);
                let equals =
                    self.pool
                        .method("java/lang/String", "equals", "(Ljava/lang/Object;)Z", false);
                self.op2(0xb6, equals);
                self.pops(1);
                self.pops(1);
                waiting.push((self.jump(0x9a), index));
            }
        }

        if seen.is_empty() {
            // Java throws on a null subject whether or not there is anything
            // to compare it against, so with nothing to compare it against the
            // check is written out.
            self.load(held, &text);
            let class_of =
                self.pool
                    .method("java/lang/Object", "getClass", "()Ljava/lang/Class;", false);
            self.op2(0xb6, class_of);
            self.op(0x57);
            self.pops(1);
        }
        let _ = line;

        let fallthrough = self.jump(0xa7);
        Ok(Dispatch::Chain {
            waiting,
            fallthrough,
        })
    }

    /// `try`, its `catch` clauses, and its `finally`.
    ///
    /// The `finally` is written out again at every way out: once after the
    /// `try` completes, once after each `catch` completes, once at a handler
    /// that catches everything and rethrows, and once at every `return`,
    /// `break` or `continue` that jumps past it. That is four or more copies
    /// of the same block, and it is what `javac` does, because the alternative
    /// -- a subroutine the code jumps into and back out of -- is the `jsr`
    /// instruction, which the type-checking verifier will not accept.
    ///
    /// The handler that rethrows must not protect the copies. A `finally` that
    /// throws would otherwise run its own copy again, and then again, and the
    /// exception would be swallowed by its own cleanup. So the ranges written
    /// into the exception table have the inlined copies cut out of them.
    fn try_catch(
        &mut self,
        body: &[Positioned<Statement>],
        catches: &[Catch],
        finally: Option<&[Positioned<Statement>]>,
        line: u32,
    ) -> Result<(), Diagnostic> {
        let outer = self.finallys.len();
        if let Some(finally) = finally {
            self.finallys.push(finally.to_vec());
        }

        let begun = self.code.len();
        self.open();
        for one in body {
            self.statement(one)?;
        }
        self.close();
        let ended = self.code.len();

        let mut outs: Vec<Pending> = Vec::new();
        if !body.iter().any(|one| never_completes(&one.node)) {
            self.run_finallys(outer)?;
            outs.push(self.jump(0xa7));
        }

        // Every catch clause, in the order written, because the first one that
        // matches is the one that runs.
        let throwable = Type::Object("java/lang/Throwable".to_string());
        let mut caught: Vec<(usize, usize)> = Vec::new();
        for catch in catches {
            let mut classes = Vec::new();
            for written in &catch.types {
                let resolved = self.resolve(written, catch.line)?;
                let Type::Object(name) = resolved else {
                    return Err(at(
                        "EJ241",
                        catch.line,
                        catch.column,
                        format!(
                            "`catch` wants a class, and {} is not one.",
                            resolved.readable()
                        ),
                    ));
                };
                classes.push(name);
            }

            // With one type caught, the slot holds exactly that type. With
            // several, the slot holds what they have in common -- and working
            // out what that is needs a class hierarchy this compiler does not
            // have, so it holds a Throwable and says so if that is not enough.
            let held = if classes.len() == 1 {
                Type::Object(classes[0].clone())
            } else {
                throwable.clone()
            };

            let target = self.code.len();
            for name in &classes {
                self.protect(begun, ended, target, Some(name.clone()), outer);
            }

            self.stack_is(vec![Verified::of(&held)]);
            self.a_branch_lands_here();
            self.open();
            let slot = self.declare(&catch.name, held.clone());
            self.store(slot, &held);

            let body_begun = self.code.len();
            for one in &catch.body {
                self.statement(one)?;
            }
            let body_ended = self.code.len();
            self.close();
            caught.push((body_begun, body_ended));

            if !catch.body.iter().any(|one| never_completes(&one.node)) {
                self.run_finallys(outer)?;
                outs.push(self.jump(0xa7));
            }
        }

        // The handler that exists only so the `finally` runs when something
        // leaves by throwing.
        if let Some(finally) = finally {
            let target = self.code.len();
            self.protect(begun, ended, target, None, outer);
            for (from, to) in caught {
                self.protect(from, to, target, None, outer);
            }

            self.stack_is(vec![Verified::of(&throwable)]);
            self.a_branch_lands_here();
            self.open();
            let slot = self.declare("$thrown", throwable.clone());
            self.store(slot, &throwable);

            let held = std::mem::take(&mut self.finallys);
            self.finallys = held[..outer].to_vec();
            for one in finally {
                self.statement(one)?;
            }
            self.finallys = held;

            self.load(slot, &throwable);
            self.op(0xbf);
            self.set_depth(0);
            self.close();
        }

        self.finallys.truncate(outer);
        let _ = line;

        if outs.is_empty() {
            // Nothing arrives after this, so there is nothing to land and no
            // frame to write: a frame at a place nothing reaches is a claim
            // about a state that never happens.
            return Ok(());
        }
        for pending in outs {
            self.land(pending);
        }
        self.set_depth(0);
        self.a_branch_lands_here();
        Ok(())
    }

    /// A class written where it is used.
    ///
    /// It gets a name -- the enclosing class, a dollar, and a number -- and
    /// becomes an ordinary class compiled after this one. What it reads from
    /// around it becomes what it is built with: the enclosing instance, when
    /// there is one, and every local of the method it was written in that its
    /// body names. Those are copied in, which is why Java insists they do not
    /// change afterwards: a copy that could go stale is worse than a rule.
    fn anonymous(
        &mut self,
        what: &Written,
        arguments: &[Expression],
        body: &Body,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let target = self.resolve(what, line)?;
        self.synthesise(target, arguments, body, line)
    }

    /// `x -> ...`: the one method of an interface, written where it is
    /// wanted.
    ///
    /// A lambda has no type of its own. Which interface it is depends
    /// entirely on where it is going, so what it is being handed to has to be
    /// known before it can be written at all -- and where that is not known,
    /// saying so is the only honest answer.
    fn lambda(
        &mut self,
        parameters: &[(Option<Written>, String)],
        body: &[Positioned<Statement>],
        expression: bool,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let Some(target) = self.expecting.take() else {
            return Err(at(
                "EJ250",
                line,
                1,
                "There is nothing here saying which interface this stands for.",
            )
            .with_suggestion(
                "A lambda takes its type from what it is handed to: an argument, a \
                 declared variable, or what a method returns. Write the type, or write \
                 the class out.",
            ));
        };
        let Type::Object(named) = target.clone() else {
            return Err(at(
                "EJ250",
                line,
                1,
                format!("A lambda cannot stand for a {}.", target.readable()),
            ));
        };

        let single = self.the_one_method_of(&named, line)?;
        if single.parameters.len() != parameters.len() {
            return Err(at(
                "EJ251",
                line,
                1,
                format!(
                    "`{}.{}` takes {} argument(s) and this was written with {}.",
                    named.replace('/', "."),
                    single.name,
                    single.parameters.len(),
                    parameters.len()
                ),
            ));
        }

        // The types come from the interface. A type written on a parameter has
        // to agree with it rather than replace it.
        let mut written = Vec::new();
        for ((given, name), wanted) in parameters.iter().zip(single.parameters.iter()) {
            if let Some(given) = given {
                let found = self.resolve(given, line)?;
                if found != *wanted {
                    return Err(at(
                        "EJ251",
                        line,
                        1,
                        format!(
                            "`{name}` is written as a {} and `{}` hands over a {}.",
                            found.readable(),
                            single.name,
                            wanted.readable()
                        ),
                    ));
                }
            }
            let Some(shape) = written_for(wanted) else {
                return Err(at(
                    "EJ251",
                    line,
                    1,
                    format!("`{}` hands over something this cannot name.", single.name),
                ));
            };
            written.push((shape, name.clone()));
        }

        // An expression body is the value the method hands back, unless it
        // hands back nothing -- in which case it is just something that
        // happens.
        let mut inside = body.to_vec();
        if expression && single.returns != Type::Void {
            if let Some(last) = inside.pop() {
                let Statement::Express(value) = last.node else {
                    return Err(at(
                        "EJ251",
                        line,
                        1,
                        "A lambda body has to produce a value.",
                    ));
                };
                inside.push(Positioned {
                    node: Statement::Return(Some(value)),
                    line: last.line,
                    column: last.column,
                });
            }
        }

        let Some(returns) = written_for(&single.returns).or(Some(Written::Void)) else {
            unreachable!("void is nameable")
        };
        let made = Body {
            fields: Vec::new(),
            methods: vec![Method {
                modifiers: Modifiers {
                    public: true,
                    ..Modifiers::default()
                },
                returns: if single.returns == Type::Void {
                    Written::Void
                } else {
                    returns
                },
                name: single.name.clone(),
                parameters: written,
                body: Some(inside),
                constructor: false,
                variadic: false,
                line,
            }],
            instance_setup: Vec::new(),
        };
        self.synthesise(target, &[], &made, line)
    }

    /// `Type::method`, `value::method` and `Type::new`.
    ///
    /// All three are a lambda whose body is one call, so all three are written
    /// as one: the parameters come from the interface, and the call is handed
    /// exactly those. What differs is only where the call goes.
    fn method_reference(
        &mut self,
        on: &Expression,
        name: &str,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let Some(target) = self.expecting.clone() else {
            return Err(at(
                "EJ250",
                line,
                1,
                "There is nothing here saying which interface this stands for.",
            )
            .with_suggestion(
                "A method reference takes its type from what it is handed to: an \
                 argument, a declared variable, or what a method returns.",
            ));
        };
        let Type::Object(named) = target else {
            return Err(at(
                "EJ250",
                line,
                1,
                "A method reference cannot stand for something that is not a class.",
            ));
        };
        let single = self.the_one_method_of(&named, line)?;

        // One name per thing the interface hands over, and the call is handed
        // exactly those, in order.
        let mut parameters = Vec::new();
        let mut arguments = Vec::new();
        for (position, wanted) in single.parameters.iter().enumerate() {
            let held = format!("$given{position}");
            let Some(shape) = written_for(wanted) else {
                return Err(at(
                    "EJ251",
                    line,
                    1,
                    format!("`{}` hands over something this cannot name.", single.name),
                ));
            };
            parameters.push((Some(shape), held.clone()));
            arguments.push(Expression::Name(held));
        }

        // `Type::new` makes one; everything else calls one.
        let call = if name == "<init>" {
            let Expression::Name(class) = on else {
                return Err(at(
                    "EJ251",
                    line,
                    1,
                    "`::new` is written on the name of a class.",
                ));
            };
            Expression::New {
                what: Written::Named(class.clone()),
                arguments,
            }
        } else {
            Expression::Call {
                on: Some(Box::new(on.clone())),
                super_call: false,
                name: name.to_string(),
                arguments,
            }
        };

        let body = vec![Positioned {
            node: Statement::Express(call),
            line,
            column: 1,
        }];
        self.expecting = Some(Type::Object(named));
        self.lambda(&parameters, &body, true, line)
    }

    /// The one method an interface has, which is what a lambda stands for.
    fn the_one_method_of(&self, named: &str, line: u32) -> Result<Signature, Diagnostic> {
        let mut found: Vec<Signature> = Vec::new();
        if let Some(known) = self.classpath.get(named) {
            found.extend(known.methods.iter().cloned());
        }
        if found.is_empty() {
            for (class, name, descriptor, static_) in BUILT_IN_METHODS {
                if *class != named || *static_ {
                    continue;
                }
                let Some((parameters, returns)) = read_descriptor(descriptor) else {
                    continue;
                };
                found.push(Signature {
                    owner: named.to_string(),
                    name: (*name).to_string(),
                    parameters,
                    returns,
                    static_: false,
                    interface: true,
                    variadic: false,
                });
            }
        }
        found.retain(|one| !one.static_ && one.name != "<init>");
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(at(
                "EJ250",
                line,
                1,
                format!(
                    "`{}` has no method this compilation knows, so there is nothing for a \
                     lambda to be.",
                    named.replace('/', ".")
                ),
            )
            .with_suggestion("Hand the class file that declares it over as a dependency.")),
            many => Err(at(
                "EJ250",
                line,
                1,
                format!(
                    "`{}` has {many} methods, so a lambda cannot say which one it is.",
                    named.replace('/', ".")
                ),
            )
            .with_suggestion("Write the class out, naming the method.")),
        }
    }

    /// The constructor of a class that takes exactly this many arguments.
    fn constructor_taking(&self, class: &str, count: usize) -> Option<Signature> {
        self.classpath
            .find_method(class, "<init>", count)
            .cloned()
            .or_else(|| built_in_method(class, "<init>", count))
    }

    /// Makes one class out of a body and a type it stands in for.
    fn synthesise(
        &mut self,
        target: Type,
        arguments: &[Expression],
        body: &Body,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let Type::Object(named) = target.clone() else {
            return Err(at(
                "EJ248",
                line,
                1,
                format!(
                    "A class written where it is used has to name a class or an \
                     interface, and {} is neither.",
                    target.readable()
                ),
            ));
        };
        let is_interface = match self.classpath.get(&named) {
            Some(known) => known.interface,
            None => BUILT_IN_INTERFACES.contains(&named.as_str()),
        };
        if is_interface && !arguments.is_empty() {
            return Err(at(
                "EJ248",
                line,
                1,
                "An interface has no constructor, so there is nothing to hand it.",
            ));
        }

        // What the body reads that belongs to the method around it.
        let mut wanted: Vec<String> = Vec::new();
        names_read(body, &mut wanted);
        let mut captured: Vec<Local> = Vec::new();
        for name in &wanted {
            let Some(local) = self.local(name) else {
                continue;
            };
            if captured.iter().any(|held| held.slot == local.slot) {
                continue;
            }
            captured.push(local);
        }
        captured.sort_by_key(|held| held.slot);

        let mut assigned: Vec<String> = Vec::new();
        names_assigned(body, &mut assigned);
        for name in &assigned {
            if captured.iter().any(|held| held.name == *name) {
                return Err(at(
                    "EJ249",
                    line,
                    1,
                    format!(
                        "`{name}` belongs to the method around this class, and this \
                         class writes to it."
                    ),
                )
                .with_suggestion(
                    "What a class written here reads from around it is copied in, so a \
                     write would not be seen outside. Use a field, or an array of one.",
                ));
            }
        }

        // What the superclass constructor is handed, worked out here because
        // these are expressions of the method around this class.
        let mut given = Vec::new();
        for argument in arguments {
            given.push(self.type_of(argument, line)?);
        }

        let outer = (!self.static_).then(|| self.this_class.clone());
        let index = self.made.len() + 1;
        let name = format!("{}${index}", self.unit.name);

        // The fields it is built with, and the constructor that fills them.
        let mut fields = Vec::new();
        let mut parameters: Vec<(Written, String)> = Vec::new();
        let mut filling: Vec<Positioned<Statement>> = Vec::new();
        let held = |what: Written, name: &str, line: u32| Field {
            modifiers: Modifiers {
                private: true,
                final_: true,
                ..Modifiers::default()
            },
            what,
            name: name.to_string(),
            value: None,
            line,
        };
        let fill = |name: &str, line: u32| Positioned {
            node: Statement::Express(Expression::Assign {
                target: Box::new(Expression::Field {
                    of: Box::new(Expression::This),
                    name: name.to_string(),
                }),
                operator: None,
                value: Box::new(Expression::Name(name.to_string())),
            }),
            line,
            column: 1,
        };

        if let Some(enclosing) = &outer {
            let written = Written::Named(enclosing.replace('/', "."));
            fields.push(held(written.clone(), OUTER, line));
            parameters.push((written, OUTER.to_string()));
            filling.push(fill(OUTER, line));
        }
        for local in &captured {
            let Some(written) = written_for(&local.what) else {
                return Err(at(
                    "EJ249",
                    line,
                    1,
                    format!(
                        "`{}` is a {} and a class written here cannot hold one.",
                        local.name,
                        local.what.readable()
                    ),
                ));
            };
            fields.push(held(written.clone(), &local.name, line));
            parameters.push((written, local.name.clone()));
            filling.push(fill(&local.name, line));
        }

        let mut chain = Vec::new();
        for (position, one) in given.iter().enumerate() {
            let Some(written) = written_for(one) else {
                return Err(at(
                    "EJ248",
                    line,
                    1,
                    format!(
                        "A {} cannot be handed to a superclass here.",
                        one.readable()
                    ),
                ));
            };
            let argument = format!("$arg{position}");
            parameters.push((written, argument.clone()));
            chain.push(Expression::Name(argument));
        }

        let mut constructor_body = vec![Positioned {
            node: Statement::Chain {
                to_super: true,
                arguments: chain,
            },
            line,
            column: 1,
        }];
        constructor_body.extend(filling);

        let mut methods = vec![Method {
            modifiers: Modifiers::default(),
            returns: Written::Void,
            name: "<init>".to_string(),
            parameters: parameters.clone(),
            body: Some(constructor_body),
            constructor: true,
            variadic: false,
            line,
        }];
        methods.extend(body.methods.iter().cloned());
        fields.extend(body.fields.iter().cloned());

        let dotted = named.replace('/', ".");
        self.made.push(Unit {
            shape: Shape::Class,
            package: self.unit.package.clone(),
            imports: self.unit.imports.clone(),
            modifiers: Modifiers {
                final_: true,
                ..Modifiers::default()
            },
            name: name.clone(),
            extends: (!is_interface).then(|| dotted.clone()),
            implements: if is_interface {
                vec![dotted]
            } else {
                Vec::new()
            },
            fields,
            methods,
            instance_setup: body.instance_setup.clone(),
            static_setup: Vec::new(),
            outer: outer.clone(),
        });

        // And the making of it, here.
        let made = match &self.unit.package {
            Some(package) => format!("{}/{name}", package.replace('.', "/")),
            None => name,
        };
        let index = self.pool.class(&made);
        self.op2(0xbb, index);
        self.pushes(&Type::Object(made.clone()));
        self.duplicates();

        let mut descriptor = String::from("(");
        if outer.is_some() {
            let this = Type::Object(self.this_class.clone());
            self.load(0, &this);
            descriptor.push_str(&this.descriptor());
        }
        for local in &captured {
            self.load(local.slot, &local.what);
            descriptor.push_str(&local.what.descriptor());
        }
        for (argument, want) in arguments.iter().zip(given.iter()) {
            let found = self.value(argument, line)?;
            if !found.is_reference() {
                self.convert(&found, want, line)?;
            }
            descriptor.push_str(&want.descriptor());
        }
        descriptor.push_str(")V");

        let init = self.pool.method(&made, "<init>", &descriptor, false);
        self.op2(0xb7, init);
        let taken: i32 = read_descriptor(&descriptor)
            .map(|(parameters, _)| parameters.iter().map(|one| i32::from(one.width())).sum())
            .unwrap_or(0);
        self.pops(taken + 1);
        Ok(target)
    }

    /// `super(...)` or `this(...)`, at the top of a constructor.
    fn chain_to(
        &mut self,
        owner: &str,
        arguments: &[Expression],
        line: u32,
    ) -> Result<(), Diagnostic> {
        let signature = if owner == self.this_class {
            let own = self
                .unit
                .methods
                .iter()
                .find(|held| held.constructor && held.parameters.len() == arguments.len())
                .cloned();
            let Some(own) = own else {
                return Err(at(
                    "EJ243",
                    line,
                    1,
                    format!(
                        "This class has no constructor taking {} argument(s).",
                        arguments.len()
                    ),
                ));
            };
            let mut parameters = Vec::new();
            for (what, _) in &own.parameters {
                parameters.push(self.resolve(what, line)?);
            }
            Signature {
                owner: owner.to_string(),
                name: "<init>".to_string(),
                parameters,
                returns: Type::Void,
                static_: false,
                interface: false,
                variadic: false,
            }
        } else {
            match self.find_signature(owner, "<init>", arguments.len()) {
                Some(found) => found,
                None if arguments.is_empty() => Signature {
                    owner: owner.to_string(),
                    name: "<init>".to_string(),
                    parameters: Vec::new(),
                    returns: Type::Void,
                    static_: false,
                    interface: false,
                    variadic: false,
                },
                None => {
                    return Err(at(
                        "EJ243",
                        line,
                        1,
                        format!(
                            "`{}` has no constructor taking {} argument(s) that this \
                             compilation knows.",
                            owner.replace('/', "."),
                            arguments.len()
                        ),
                    )
                    .with_suggestion("Hand the class file that declares it over as a dependency."))
                }
            }
        };

        self.load(0, &Type::Object(self.this_class.clone()));
        self.arguments_for_signature(&signature, arguments, line)?;
        let descriptor = signature.descriptor();
        let index = self.pool.method(owner, "<init>", &descriptor, false);
        self.op2(0xb7, index);
        let taken: i32 = signature
            .parameters
            .iter()
            .map(|one| i32::from(one.width()))
            .sum();
        self.pops(taken + 1);
        Ok(())
    }

    /// `for (T name : thing)`, where `thing` hands over an iterator.
    ///
    /// This is the loop the language says it is: ask for an iterator, ask it
    /// whether there is another, take it, run the body. The iterator lives in
    /// a slot of its own so the body cannot reach it.
    fn for_each_over_an_iterator(
        &mut self,
        what: &Written,
        name: &str,
        over: &Expression,
        body: &Positioned<Statement>,
        found: &Type,
        line: u32,
    ) -> Result<(), Diagnostic> {
        let Type::Object(class) = found.clone() else {
            return Err(at(
                "EJ236",
                line,
                1,
                format!("A `for` cannot walk over a {}.", found.readable()),
            )
            .with_suggestion("An array works, and so does anything with an `iterator()`."));
        };
        let Some(iterator) = self.signature_for(&class, "iterator", &[], line)? else {
            return Err(at(
                "EJ236",
                line,
                1,
                format!(
                    "`{}` has no `iterator()` that this compilation knows.",
                    class.replace('/', ".")
                ),
            )
            .with_suggestion("Hand the class file that declares it over as a dependency."));
        };

        let object = Type::Object("java/lang/Object".to_string());
        let declared = match what {
            // `var` over an iterator gets Object, because erasure is all there
            // is to go on and guessing would be worse than saying so.
            Written::Inferred => object.clone(),
            other => self.resolve(other, line)?,
        };

        self.open();
        let over_type = self.value(over, line)?;
        let _ = over_type;
        let held = self.declare("$walking", iterator.returns.clone());
        self.call_signature(&iterator, line);
        self.store(held, &iterator.returns);

        let Type::Object(iterator_class) = iterator.returns.clone() else {
            return Err(at(
                "EJ236",
                line,
                1,
                "An `iterator()` has to hand over an object.",
            ));
        };
        let Some(has_next) = self.signature_for(&iterator_class, "hasNext", &[], line)? else {
            return Err(at("EJ236", line, 1, "An iterator has to have `hasNext()`."));
        };
        let Some(next) = self.signature_for(&iterator_class, "next", &[], line)? else {
            return Err(at("EJ236", line, 1, "An iterator has to have `next()`."));
        };

        let top = self.code.len();
        self.a_branch_lands_here();
        self.load(held, &iterator.returns);
        self.call_signature(&has_next, line);
        self.pops(1);
        let out = self.jump(0x99);

        self.open();
        self.load(held, &iterator.returns);
        self.call_signature(&next, line);
        if declared != object {
            let Type::Object(named) = &declared else {
                return Err(at(
                    "EJ237",
                    line,
                    1,
                    format!(
                        "`{name}` is a {}, and an iterator hands over objects.",
                        declared.readable()
                    ),
                )
                .with_suggestion("Write the loop over a reference type, or over an array."));
            };
            let index = self.pool.class(named);
            self.op2(0xc0, index);
            self.pops(1);
            self.pushes(&declared);
        }
        let slot = self.declare(name, declared.clone());
        self.store(slot, &declared);

        self.enter(true);
        self.statement(body)?;
        let level = self.leave();
        self.close();

        for pending in level.continues {
            self.land(pending);
        }
        self.a_branch_lands_here();
        self.jump_back(0xa7, top);
        self.land(out);
        for pending in level.breaks {
            self.land(pending);
        }
        self.close();
        self.a_branch_lands_here();
        Ok(())
    }

    /// Writes the call one signature describes, with everything it takes
    /// already on the stack.
    fn call_signature(&mut self, signature: &Signature, line: u32) {
        let _ = line;
        let descriptor = signature.descriptor();
        let index = self.pool.method(
            &signature.owner,
            &signature.name,
            &descriptor,
            signature.interface,
        );
        let taken: i32 = signature
            .parameters
            .iter()
            .map(|one| i32::from(one.width()))
            .sum();
        if signature.interface {
            self.code.push(0xb9);
            self.code.extend_from_slice(&index.to_be_bytes());
            self.code.push((taken + 1) as u8);
            self.code.push(0);
        } else {
            self.op2(0xb6, index);
        }
        self.pops(taken + 1);
        self.pushes(&signature.returns);
    }

    /// `Outer.this`, which is the instance a class written inside another
    /// belongs to.
    fn outer_this(&mut self, of: &Written, line: u32) -> Result<Type, Diagnostic> {
        let wanted = self.resolve(of, line)?;
        let Type::Object(named) = wanted.clone() else {
            return Err(at("EJ252", line, 1, "`this` belongs to a class."));
        };
        if self.this_class == named {
            if self.static_ {
                return Err(at(
                    "EJ222",
                    line,
                    1,
                    "`this` has no meaning in a static method.",
                ));
            }
            self.load(0, &Type::Object(self.this_class.clone()));
            return Ok(wanted);
        }
        let Some(enclosing) = self.unit.outer.clone() else {
            return Err(at(
                "EJ252",
                line,
                1,
                format!(
                    "This class does not belong to an instance of `{}`.",
                    named.replace('/', ".")
                ),
            ));
        };
        if enclosing != named {
            return Err(at(
                "EJ252",
                line,
                1,
                format!(
                    "This class belongs to an instance of `{}`, not of `{}`.",
                    enclosing.replace('/', "."),
                    named.replace('/', ".")
                ),
            ));
        }
        self.reach_the_enclosing_instance(&enclosing)?;
        Ok(wanted)
    }

    /// `new int[3]`, `new int[3][4]`, `new String[2][]`.
    ///
    /// One dimension is `newarray` or `anewarray`; more than one is
    /// `multianewarray`, which takes every length at once and is the only
    /// instruction that does.
    fn new_array(
        &mut self,
        of: &Written,
        lengths: &[Expression],
        empty: usize,
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let base = self.resolve(of, line)?;
        let mut whole = base;
        for _ in 0..lengths.len() + empty {
            whole = Type::Array(Box::new(whole));
        }

        // `new int[3][4]` is written as an array of arrays, each made in
        // turn. `multianewarray` would say it in one instruction, and Dalvik
        // has no such instruction -- so the loop the JVM would have run
        // internally is written out, which is also what makes `new int[3][]`
        // and `new int[3][4]` the same road.
        if lengths.len() > 1 {
            return self.array_of_arrays(&whole, lengths, line);
        }

        for length in lengths {
            let found = self.value(length, line)?;
            self.convert(&found, &Type::Int, line)?;
        }

        let Type::Array(element) = whole.clone() else {
            unreachable!("an array was just built")
        };
        match element.as_ref() {
            Type::Object(name) => {
                let index = self.pool.class(name);
                self.op2(0xbd, index);
            }
            held @ Type::Array(_) => {
                let index = self.pool.class(&held.descriptor());
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
                    _ => return Err(at("EJ209", line, 1, "An array of void is not a thing.")),
                };
                self.op1(0xbc, code);
            }
        }
        // The length goes and the array arrives, which is one slot either way
        // and two different types -- and a frame written after this has to say
        // the second of them.
        if !lengths.is_empty() {
            self.pops(1);
            self.pushes(&whole);
        }
        Ok(whole)
    }

    /// `new int[3][4]`: an array of three arrays, each of four.
    ///
    /// The outer one is made, then filled in a loop with the inner ones. Doing
    /// it here rather than with one instruction is not a workaround: Dalvik
    /// has no instruction for it, and this is what the JVM does anyway.
    fn array_of_arrays(
        &mut self,
        whole: &Type,
        lengths: &[Expression],
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let Type::Array(inner) = whole.clone() else {
            unreachable!("an array of arrays is an array")
        };
        let Some(of_inner) = written_for(&inner) else {
            return Err(at(
                "EJ121",
                line,
                1,
                "This array holds something that cannot be named.",
            ));
        };

        self.open();
        // How many, worked out once, because the expression may have an
        // effect and Java runs it once.
        let counted = self.value(&lengths[0], line)?;
        self.convert(&counted, &Type::Int, line)?;
        let how_many = self.declare("$many", Type::Int);
        self.store(how_many, &Type::Int);

        self.load(how_many, &Type::Int);
        self.new_array(&of_inner, &[], 1, line)?;
        // `new_array` with no lengths does not write the size, so it is
        // written here: the count is already on the stack under it.
        let made = self.declare("$made", whole.clone());
        self.store(made, whole);

        self.op(0x03);
        self.pushes(&Type::Int);
        let at = self.declare("$at", Type::Int);
        self.store(at, &Type::Int);

        let top = self.code.len();
        self.a_branch_lands_here();
        self.load(at, &Type::Int);
        self.load(how_many, &Type::Int);
        self.pops(2);
        let out = self.jump(0xa2); // if_icmpge

        self.load(made, whole);
        self.load(at, &Type::Int);
        self.new_array(&of_inner, &lengths[1..], 0, line)?;
        self.op(array_store(&inner));
        self.pops(3);

        self.bump_local(at, 1);
        self.jump_back(0xa7, top);
        self.land(out);
        self.a_branch_lands_here();

        self.load(made, whole);
        self.close();
        Ok(whole.clone())
    }

    /// `{ a, b, c }`: an array of the right size, filled in.
    fn array_of(
        &mut self,
        whole: &Type,
        values: &[Expression],
        line: u32,
    ) -> Result<Type, Diagnostic> {
        let Type::Array(element) = whole.clone() else {
            return Err(at(
                "EJ121",
                line,
                1,
                format!("A {} is not an array.", whole.readable()),
            ));
        };

        let of = written_for(&element).ok_or_else(|| {
            at(
                "EJ121",
                line,
                1,
                "This array holds something that cannot be named.",
            )
        })?;
        self.new_array(&of, &[Expression::Int(values.len() as i64)], 0, line)?;

        let store = array_store(&element);
        for (position, value) in values.iter().enumerate() {
            self.duplicates();
            self.push_int(position as i64);
            // An array written inside an array takes what it holds from the
            // one around it, which is only known here.
            let found = match value {
                Expression::ArrayOf {
                    of: None,
                    values,
                    line: written,
                } => self.array_of(&element, values, *written)?,
                other => self.value_for(other, &element, line)?,
            };
            if !found.may_be_given_to(&element) {
                return Err(at(
                    "EJ121",
                    line,
                    1,
                    format!(
                        "This array holds {} and was given a {}.",
                        element.readable(),
                        found.readable()
                    ),
                ));
            }
            if !found.is_reference() {
                self.convert(&found, &element, line)?;
            }
            self.op(store);
            self.pops(2 + i32::from(element.width()));
        }
        Ok(whole.clone())
    }

    /// `iinc`, on a slot this compiler owns.
    fn bump_local(&mut self, slot: u16, by: i8) {
        if slot <= 255 {
            self.op(0x84);
            self.code.push(slot as u8);
            self.code.push(by as u8);
            return;
        }
        self.op(0xc4);
        self.op(0x84);
        self.code.extend_from_slice(&slot.to_be_bytes());
        self.code.extend_from_slice(&i16::from(by).to_be_bytes());
    }
}

/// How a `switch` gets from its subject to an arm, once the arms have been
/// written and their positions are known.
/// What a `switch` is choosing on, which decides how it gets from the subject
/// to an arm.
enum Chooser {
    Integer,
    Text,
    /// An enum, named by its class.
    Constant(String),
}

enum Dispatch {
    /// A `tableswitch` or a `lookupswitch`, whose offsets are still zero.
    Instruction {
        opcode_at: usize,
        default_slot: usize,
        /// Where each offset goes, and which arm it points at. `usize::MAX`
        /// stands for a hole in a jump table's range, which goes to the
        /// default.
        slots: Vec<(usize, usize)>,
    },
    /// A chain of comparisons, each with a jump waiting to be told where its
    /// arm is, and one more for the default at the end.
    Chain {
        waiting: Vec<(Pending, usize)>,
        fallthrough: Pending,
    },
}

impl Dispatch {
    fn settle(self, emitter: &mut Emitter<'_>, targets: &[usize], default_at: usize) {
        match self {
            Dispatch::Instruction {
                opcode_at,
                default_slot,
                slots,
            } => {
                let offset = |to: usize| (to as i64 - opcode_at as i64) as i32;
                emitter.code[default_slot..default_slot + 4]
                    .copy_from_slice(&offset(default_at).to_be_bytes());
                for (slot, arm) in slots {
                    let to = match targets.get(arm) {
                        Some(found) => *found,
                        None => default_at,
                    };
                    emitter.code[slot..slot + 4].copy_from_slice(&offset(to).to_be_bytes());
                }
            }
            Dispatch::Chain {
                waiting,
                fallthrough,
            } => {
                for (pending, arm) in waiting {
                    let to = targets.get(arm).copied().unwrap_or(default_at);
                    emitter.land_at(pending, to);
                }
                emitter.land_at(fallthrough, default_at);
            }
        }
    }
}

/// A `case` label that has to be a constant integer, read as one.
fn constant_int(expression: &Expression) -> Option<i32> {
    match expression {
        Expression::Int(value) => i32::try_from(*value).ok(),
        Expression::Char(value) => Some(i32::from(*value)),
        Expression::Boolean(value) => Some(i32::from(*value)),
        Expression::Unary {
            operator: Unary::Negate,
            of,
        } => constant_int(of)?.checked_neg(),
        _ => None,
    }
}

/// The type a chain of names in front of `.class` or `.this` was spelling.
///
/// `java.util.List.class` parses as a field read of a field read of a name,
/// because nothing tells the parser it is a type until the `class` arrives.
fn as_a_written_type(expression: &Expression) -> Option<Written> {
    fn dotted(expression: &Expression) -> Option<String> {
        Some(match expression {
            Expression::Name(name) => name.clone(),
            Expression::Field { of, name } => format!("{}.{name}", dotted(of)?),
            _ => return None,
        })
    }
    Some(Written::Named(dotted(expression)?))
}

/// The box a primitive is kept in, which is where its class lives.
fn boxed_name(what: &Type) -> Option<&'static str> {
    Some(match what {
        Type::Boolean => "java/lang/Boolean",
        Type::Byte => "java/lang/Byte",
        Type::Short => "java/lang/Short",
        Type::Char => "java/lang/Character",
        Type::Int => "java/lang/Integer",
        Type::Long => "java/lang/Long",
        Type::Float => "java/lang/Float",
        Type::Double => "java/lang/Double",
        Type::Void => "java/lang/Void",
        _ => return None,
    })
}

/// The instruction that writes one element into an array of this type.
fn array_store(element: &Type) -> u8 {
    match element {
        Type::Long => 0x50,
        Type::Float => 0x51,
        Type::Double => 0x52,
        Type::Byte | Type::Boolean => 0x54,
        Type::Char => 0x55,
        Type::Short => 0x56,
        other if other.is_reference() => 0x53,
        _ => 0x4f,
    }
}

/// The instruction that reads one element out of an array of this type.
fn array_load(element: &Type) -> u8 {
    match element {
        Type::Long => 0x2f,
        Type::Float => 0x30,
        Type::Double => 0x31,
        Type::Byte | Type::Boolean => 0x33,
        Type::Char => 0x34,
        Type::Short => 0x35,
        other if other.is_reference() => 0x32,
        _ => 0x2e,
    }
}

fn write_attribute(out: &mut Vec<u8>, name: u16, body: &[u8]) {
    out.extend_from_slice(&name.to_be_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

/// Compiles one unit into the bytes of a class file.
/// The access flags a method carries beyond its written modifiers.
fn method_shape(method: &Method) -> u16 {
    // ACC_VARARGS. It changes nothing the JVM does; it is how reflection, and
    // anything reading the class file, knows the last parameter was written
    // with `...` rather than as an array.
    if method.variadic {
        return 0x0080;
    }
    0
}

/// One unit, and whatever classes its bodies turned out to need.
pub fn compile_unit(
    unit: &Unit,
    classpath: &Classpath,
) -> Result<(Vec<u8>, Vec<Unit>), Diagnostic> {
    // An `assert` is guarded by a flag the class initialiser works out once,
    // which is what makes assertions cost nothing where nobody asked for them
    // -- and on Android nobody does. It is an ordinary member of the class, so
    // it is added to the class rather than carried alongside it.
    let held;
    let unit = if holds_an_assert(unit) {
        held = with_the_assertion_flag(unit);
        &held
    } else {
        unit
    };

    let this_class = unit.internal_name();
    let mut pool = Pool::new();
    let mut made: Vec<Unit> = Vec::new();

    let superclass = match &unit.extends {
        Some(name) => {
            let probe = Emitter::new(
                &mut pool,
                classpath,
                unit,
                this_class.clone(),
                true,
                &mut made,
            );
            probe.resolve_class(name, 1)?
        }
        None => "java/lang/Object".to_string(),
    };

    let mut interfaces = Vec::new();
    for name in &unit.implements {
        let probe = Emitter::new(
            &mut pool,
            classpath,
            unit,
            this_class.clone(),
            true,
            &mut made,
        );
        interfaces.push(probe.resolve_class(name, 1)?);
    }

    // Fields.
    let mut field_bytes = Vec::new();
    for field in &unit.fields {
        let probe = Emitter::new(
            &mut pool,
            classpath,
            unit,
            this_class.clone(),
            true,
            &mut made,
        );
        let what = probe.resolve(&field.what, field.line)?;
        let name = pool.utf8(&field.name);
        let descriptor = pool.utf8(&what.descriptor());
        field_bytes.extend_from_slice(&field.modifiers.access_flags(0).to_be_bytes());
        field_bytes.extend_from_slice(&name.to_be_bytes());
        field_bytes.extend_from_slice(&descriptor.to_be_bytes());
        field_bytes.extend_from_slice(&0u16.to_be_bytes());
    }

    // What a field was given, as a statement that gives it.
    //
    // Java does not run these where they are written. An instance field's
    // value runs at the top of every constructor, after the call up into the
    // superclass, so that a constructor which reads the field sees it set; a
    // static field's value runs once, in the class initialiser, in the order
    // the fields were written.
    let assignment = |field: &Field| -> Option<Positioned<Statement>> {
        let value = field.value.clone()?;
        let target = if field.modifiers.static_ {
            Expression::Name(field.name.clone())
        } else {
            Expression::Field {
                of: Box::new(Expression::This),
                name: field.name.clone(),
            }
        };
        Some(Positioned {
            node: Statement::Express(Expression::Assign {
                target: Box::new(target),
                operator: None,
                value: Box::new(value),
            }),
            line: field.line,
            column: 1,
        })
    };

    let instance_setup: Vec<Positioned<Statement>> = unit
        .fields
        .iter()
        .filter(|field| !field.modifiers.static_)
        .filter_map(assignment)
        .chain(unit.instance_setup.iter().cloned())
        .collect();

    let static_setup: Vec<Positioned<Statement>> = unit
        .fields
        .iter()
        .filter(|field| field.modifiers.static_)
        .filter_map(assignment)
        .chain(unit.static_setup.iter().cloned())
        .collect();

    // Methods. A class with no constructor written gets the one Java would
    // have written for it, which calls up into the superclass and returns.
    let mut methods: Vec<Method> = unit.methods.clone();
    if unit.shape == Shape::Interface {
        // A method of an interface without a body is abstract, whether or not
        // anybody wrote it down.
        for method in &mut methods {
            if method.body.is_none() {
                method.modifiers.abstract_ = true;
            }
        }
    }
    if !static_setup.is_empty() {
        methods.push(Method {
            modifiers: Modifiers {
                static_: true,
                ..Modifiers::default()
            },
            returns: Written::Void,
            name: "<clinit>".to_string(),
            parameters: Vec::new(),
            body: Some(static_setup),
            constructor: false,
            variadic: false,
            line: 1,
        });
    }
    if unit.shape == Shape::Class && !methods.iter().any(|held| held.constructor) {
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
                variadic: false,
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
            let probe = Emitter::new(
                &mut pool,
                classpath,
                unit,
                this_class.clone(),
                true,
                &mut made,
            );
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
                variadic: false,
            }
            .descriptor();
            let descriptor = pool.utf8(&descriptor);
            method_bytes.extend_from_slice(
                &method
                    .modifiers
                    .access_flags(method_shape(method))
                    .to_be_bytes(),
            );
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
            &mut made,
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

        let mut body: &[Positioned<Statement>] = body;
        if method.constructor {
            // Every constructor begins by calling one above it, or the class
            // will not load. Written out, that is either `super(...)` or
            // `this(...)`; written by nobody, it is the superclass's own
            // no-argument constructor.
            let chained = match body.first().map(|one| &one.node) {
                Some(Statement::Chain {
                    to_super,
                    arguments,
                }) => {
                    let line = body[0].line;
                    let owner = if *to_super {
                        superclass.clone()
                    } else {
                        this_class.clone()
                    };
                    emitter.chain_to(&owner, arguments, line)?;
                    body = &body[1..];
                    Some(*to_super)
                }
                _ => {
                    emitter.load(0, &Type::Object(this_class.clone()));
                    let up = emitter.pool.method(&superclass, "<init>", "()V", false);
                    emitter.op2(0xb7, up);
                    emitter.pops(1);
                    Some(true)
                }
            };

            // A constructor that hands off to another one in the same class
            // must not run the field values again: the one it handed off to
            // has already run them.
            if chained == Some(true) {
                for statement in &instance_setup {
                    emitter.statement(statement)?;
                }
            }
        }

        for statement in body {
            emitter.statement(statement)?;
        }

        // A method that can fall off its end needs a return there. A `void`
        // one gets it; anything else that reaches the end without returning is
        // a mistake in the source and is said so.
        let ends_returned = body.iter().any(|one| never_completes(&one.node));
        let _ = &instance_setup;
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
        let frames = emitter.frames;
        let handlers = emitter.handlers;

        // A frame at the very start says nothing: the verifier already knows
        // what a method is entered with. One past the end is not a place
        // anything can land.
        let frames: Vec<Frame> = frames
            .into_iter()
            .filter(|frame| frame.at > 0 && frame.at < code.len())
            .collect();
        let table = stack_map_table(&frames, &mut pool);
        let table_name = table.as_ref().map(|_| pool.utf8("StackMapTable"));

        let mut attribute = Vec::new();
        attribute.extend_from_slice(&max_stack.to_be_bytes());
        attribute.extend_from_slice(&max_locals.to_be_bytes());
        attribute.extend_from_slice(&(code.len() as u32).to_be_bytes());
        attribute.extend_from_slice(&code);
        attribute.extend_from_slice(&(handlers.len() as u16).to_be_bytes());
        for handler in &handlers {
            attribute.extend_from_slice(&(handler.start as u16).to_be_bytes());
            attribute.extend_from_slice(&(handler.end as u16).to_be_bytes());
            attribute.extend_from_slice(&(handler.target as u16).to_be_bytes());
            let class = match &handler.class {
                Some(name) => pool.class(name),
                // Zero is the row that catches everything, which is how a
                // `finally` gets to run on the way out.
                None => 0,
            };
            attribute.extend_from_slice(&class.to_be_bytes());
        }
        attribute.extend_from_slice(&u16::from(table.is_some()).to_be_bytes());
        if let (Some(body), Some(name)) = (&table, table_name) {
            write_attribute(&mut attribute, name, body);
        }

        let name = pool.utf8(&method.name);
        let descriptor = Signature {
            owner: this_class.clone(),
            name: method.name.clone(),
            parameters,
            returns,
            static_: method.modifiers.static_,
            interface: false,
            variadic: false,
        }
        .descriptor();
        let descriptor = pool.utf8(&descriptor);

        method_bytes.extend_from_slice(
            &method
                .modifiers
                .access_flags(method_shape(method))
                .to_be_bytes(),
        );
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
    // ACC_SUPER, which every class written since Java 1.1 sets. An interface
    // sets ACC_INTERFACE and ACC_ABSTRACT instead, and must not set ACC_SUPER.
    let shape_flags = match unit.shape {
        Shape::Class => 0x0020u16,
        Shape::Interface => 0x0200 | 0x0400,
        // ACC_ENUM as well, which is how anything reading the file knows the
        // constants are constants.
        Shape::Enum => 0x0020 | 0x4000,
        // ACC_RECORD, and final, because a record cannot be extended.
        Shape::Record => 0x0020 | 0x0010,
    };
    out.extend_from_slice(&unit.modifiers.access_flags(shape_flags).to_be_bytes());
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
    Ok((out, made))
}

/// Reads Java and writes a class file.
/// Every class file one source file comes to.
///
/// A file declares one type most of the time and more than one sometimes, and
/// each of them is a class file of its own -- which is what the JVM has always
/// required and what `javac` has always done.
pub fn compile(source: &str, classpath: &Classpath) -> Result<Vec<(String, Vec<u8>)>, Diagnostic> {
    compile_together(&[("".to_string(), source.to_string())], classpath)
}

/// Every class a whole project's Java comes to.
///
/// The files are compiled as one compilation rather than one after another,
/// because that is what Java is: a class in one file names a class in another
/// without either of them saying where it lives, and neither exists as a class
/// file yet. So every file is parsed first, every type it declares is put on
/// one shared classpath, and only then is any body written. Compiling them in
/// turn would mean the first file could not see the second, which is a project
/// of more than one file refused for no reason a person would accept.
///
/// Each source is handed over with a label -- the path it was read from -- so
/// that a refusal says which file it is about.
pub fn compile_together(
    sources: &[(String, String)],
    classpath: &Classpath,
) -> Result<Vec<(String, Vec<u8>)>, Diagnostic> {
    let mut declared = Vec::new();
    for (label, text) in sources {
        let units = parse(text).map_err(|error| named_file(error, label))?;
        for unit in units {
            declared.push((label.clone(), unit));
        }
    }

    // A type declared beside another can be named by it, so each one is on the
    // classpath the others are compiled against.
    let mut together = classpath.clone();
    for (_, unit) in &declared {
        together.shell(unit);
    }
    for (_, unit) in &declared {
        together.declare(unit);
    }

    // Two files declaring the same type would each write over the other's
    // class, and which one the device ran would come down to the order the
    // filesystem handed them back.
    for (index, (label, unit)) in declared.iter().enumerate() {
        let name = unit.internal_name();
        if let Some((first, _)) = declared
            .iter()
            .take(index)
            .find(|(_, held)| held.internal_name() == name)
        {
            return Err(named_file(
                at(
                    "EJ253",
                    1,
                    1,
                    format!("`{}` is declared twice.", name.replace('/', ".")),
                )
                .with_context(format!("Also in: {first}"))
                .with_suggestion("One type, one place. Rename one of them."),
                label,
            ));
        }
    }

    // A body can turn out to need a class of its own -- one written where it
    // is used has no name until it is compiled. Those go on the end and are
    // compiled in their turn, and may need classes themselves.
    let mut out = Vec::new();
    let mut waiting = declared;
    let mut rounds = 0usize;
    while !waiting.is_empty() {
        rounds += 1;
        if rounds > 64 {
            return Err(at(
                "EJ119",
                1,
                1,
                "This keeps producing classes that produce more classes.",
            ));
        }
        let mut next = Vec::new();
        for (label, unit) in &waiting {
            let name = format!("{}.class", unit.internal_name());
            let (bytes, made) =
                compile_unit(unit, &together).map_err(|error| named_file(error, label))?;
            out.push((name, bytes));
            next.extend(made.into_iter().map(|one| (label.clone(), one)));
        }
        for (_, unit) in &next {
            together.shell(unit);
        }
        for (_, unit) in &next {
            together.declare(unit);
        }
        waiting = next;
    }
    Ok(out)
}

/// Says which file a refusal is about, where there is more than one.
fn named_file(error: Diagnostic, label: &str) -> Diagnostic {
    if label.is_empty() {
        return error;
    }
    error.with_context(format!("File: {label}"))
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
            // One file can declare more than one type, and each of them is a
            // class file of its own. The name it is offered under is the
            // class's, not the file's, because two types in one file would
            // otherwise be offered under the same name.
            let produced = compile(&text, &classpath)
                .map_err(|error| error.with_context(format!("File: {}", source.path)))?;
            for (name, class) in produced {
                session.offer(name, Kind::JvmClass, class)?;
            }
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

    /// The one class a source declaring one type comes to.
    fn compile_one(source: &str, classpath: &Classpath) -> Result<(String, Vec<u8>), Diagnostic> {
        let mut produced = compile(source, classpath)?;
        assert_eq!(
            produced.len(),
            1,
            "this source declares one type: {:?}",
            produced.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
        Ok(produced.remove(0))
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

        let (name, bytes) = compile_one(source, &empty()).expect("this must compile");
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
        let (_, again) = compile_one(source, &empty()).unwrap();
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
            // A lambda with nothing saying what it stands for.
            (
                "public class A { Object f() { return () -> {}; } }",
                "EJ250",
            ),
            // And one handed to an interface with more than one method, which
            // cannot say which of them it is.
            (
                "public class A { void f() { CharSequence c = () -> {}; } }",
                "EJ250",
            ),
            // A class that belongs to an instance, made where there is none.
            (
                "public class A { class B { } static B f() { return new B(); } }",
                "EJ252",
            ),
            // A `catch` of a class nobody handed over is a handler for
            // something that might not exist, and the class file would name
            // it either way.
            (
                "public class A { void f() { try { } catch (E e) { } } }",
                "EJ200",
            ),
        ] {
            let refused = compile(source, &empty())
                .err()
                .unwrap_or_else(|| panic!("this must be refused: {source}"));
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
            let refused = compile_one(source, &empty()).expect_err("this must be refused");
            assert_eq!(refused.code, expected, "{source}: {}", refused.message);
        }

        eprintln!("java: fifteen things refused, each by its own code");
    }

    #[test]
    fn the_code_this_writes_comes_back_out_of_the_class_file() {
        // A class file is only worth writing if what is in it can be got at
        // again. Until the reader kept the Code attribute, a method could be
        // named and described and its body was gone -- which is enough to say
        // what a class is and not enough to turn it into anything else.
        let source = r#"
            public class Small {
                public static int twice(int value) {
                    return value * 2;
                }
                public void nothing() {
                }
            }
        "#;
        let (_, bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&bytes).expect("what was written must read");

        let twice = class.methods.iter().find(|m| m.name == "twice").unwrap();
        let code = twice.code.as_ref().expect("a method with a body has code");
        assert!(code.max_stack >= 2, "{}", code.max_stack);
        assert_eq!(code.max_locals, 1, "one parameter, no `this`");
        assert!(code.handlers.is_empty());

        // iload_0, iconst_2, imul, ireturn. Read as the bytes they are, since
        // that is what a translator will be handed.
        assert_eq!(
            code.bytes,
            vec![0x1a, 0x05, 0x68, 0xac],
            "{:02x?}",
            code.bytes
        );

        let nothing = class.methods.iter().find(|m| m.name == "nothing").unwrap();
        let code = nothing.code.as_ref().expect("even an empty body has code");
        assert_eq!(code.bytes, vec![0xb1], "a bare return-void");
        assert_eq!(code.max_locals, 1, "`this` alone");

        eprintln!("java: the bytes of a method body survive being written and read back");
    }

    #[test]
    fn java_source_becomes_a_dex_that_dexdump_reads() {
        // The whole way through, in one test: text, to a class file, to the
        // bytecode Android runs, read back by the Android tool. Every step here
        // is this project's own -- there is no javac and no d8 in it.
        let source = r#"
            package com.my.app;

            public class Counter {
                private int count;
                private String label;

                public static int twice(int value) {
                    return value * 2;
                }

                public int add(int by) {
                    count = count + by;
                    return count;
                }

                public int sumTo(int limit) {
                    int total = 0;
                    int i = 0;
                    while (i < limit) {
                        total = total + i;
                        i = i + 1;
                    }
                    return total;
                }

                public void reset() {
                    count = 0;
                }
            }
        "#;

        let (name, class_bytes) = compile_one(source, &empty()).expect("this must compile");
        assert_eq!(name, "com/my/app/Counter.class");

        let class = crate::jvm::read(&class_bytes).expect("what was written must read");
        let translated =
            crate::dalvik::translate_class(&class).expect("and must translate to Dalvik");

        assert_eq!(translated.descriptor, "Lcom/my/app/Counter;");
        assert_eq!(translated.superclass, "Ljava/lang/Object;");
        assert_eq!(translated.instance_fields.len(), 2);
        assert!(translated.static_fields.is_empty());

        // `twice` is static and the constructor is direct; the rest are
        // dispatched on the object.
        let direct: Vec<&str> = translated
            .direct_methods
            .iter()
            .map(|one| one.reference.name.as_str())
            .collect();
        let virtual_: Vec<&str> = translated
            .virtual_methods
            .iter()
            .map(|one| one.reference.name.as_str())
            .collect();
        assert!(direct.contains(&"<init>"), "{direct:?}");
        assert!(direct.contains(&"twice"), "{direct:?}");
        for wanted in ["add", "sumTo", "reset"] {
            assert!(virtual_.contains(&wanted), "{virtual_:?}");
        }

        // Every method that has a body has instructions, and every one of them
        // declares enough registers to hold what it uses.
        for method in translated
            .direct_methods
            .iter()
            .chain(translated.virtual_methods.iter())
        {
            assert!(
                !method.instructions.is_empty(),
                "{} came out with no code",
                method.reference.name
            );
            assert!(
                method.registers >= method.inputs,
                "{} declares {} registers and takes {} in",
                method.reference.name,
                method.registers,
                method.inputs
            );
        }

        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");

        let mut sink = crate::diag::Sink::new();
        let read = crate::dex::read(&dex, &mut sink).expect("our own reader must read it");
        assert_eq!(sink.entries().len(), 0, "{:?}", sink.entries());
        assert_eq!(read.class_names(), vec!["com.my.app.Counter"]);
        assert!(crate::dex::integrity(&dex).unwrap().self_consistent());

        // Asking the Android tool to disassemble makes it verify first, which
        // is the difference between "the bytes are shaped like a dex" and "a
        // device would load this".
        if let Some(text) = dexdump_disassembly(&dex) {
            for wanted in ["Lcom/my/app/Counter;", "sumTo", "iget", "return"] {
                assert!(text.contains(wanted), "dexdump printed no {wanted:?}");
            }
        }

        // The same source is the same dex, which is what the compiler contract
        // promises when it says its output is reproducible.
        let (_, again) = compile_one(source, &empty()).unwrap();
        let again = crate::dalvik::translate_class(&crate::jvm::read(&again).unwrap()).unwrap();
        assert_eq!(
            dex,
            crate::dexwrite::write(&[again], &[]).unwrap(),
            "the same source must come to the same dex"
        );

        eprintln!(
            "dalvik: java source to a {} byte dex, five methods and two fields, no javac and no d8",
            dex.len()
        );
    }

    /// Runs `dexdump` over a dex, with the disassembly.
    ///
    /// The tool is part of the Android build-tools, which are not always here,
    /// and a missing tool is a fact about the machine rather than a failure.
    fn dexdump_disassembly(bytes: &[u8]) -> Option<String> {
        let tool = crate::tests::find_apksigner().and_then(|path| {
            let found = path.parent()?.join("dexdump");
            found.is_file().then_some(found)
        })?;

        // Tests run at the same time, so a directory named after the process
        // alone is a directory two of them write into.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let mine = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("omni-dexdump-{}-{mine}", std::process::id()));
        std::fs::create_dir_all(&directory).ok()?;
        let path = directory.join("classes.dex");
        std::fs::write(&path, bytes).ok()?;
        let output = std::process::Command::new(&tool)
            .args(["-d", path.to_str()?])
            .output()
            .ok()?;
        std::fs::remove_dir_all(&directory).ok();
        // A tool that is not here is a fact about the machine. A tool that is
        // here, ran, and refused is a fact about the dex, and it must not be
        // reported as the first one.
        assert!(
            output.status.success(),
            "dexdump refused a dex this wrote:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[test]
    fn switch_and_try_catch_reach_dalvik_and_dexdump_reads_them_back() {
        // A `switch` becomes an instruction plus a table written somewhere
        // else in the method, and a `try` becomes rows in a table that is not
        // in the instruction stream at all. Both are places where an offset
        // that is one out produces a dex our own reader still reads. So the
        // Android tool is asked what it sees.
        let source = r#"
            package com.my.app;

            public class Branching {
                private int seen;

                public int packed(int value) {
                    switch (value) {
                        case 0: return 10;
                        case 1: return 11;
                        case 2: return 12;
                        case 3: return 13;
                        default: return -1;
                    }
                }

                public int scattered(int value) {
                    switch (value) {
                        case 1: return 1;
                        case 500: return 2;
                        case 90000: return 3;
                        default: return 0;
                    }
                }

                public int named(String word) {
                    switch (word) {
                        case "yes": return 1;
                        case "no": return 0;
                        default: return -1;
                    }
                }

                public int guarded(int by) {
                    try {
                        return 100 / by;
                    } catch (ArithmeticException e) {
                        return -1;
                    }
                }

                public int cleanedUp(int by) {
                    try {
                        return 100 / by;
                    } finally {
                        seen = seen + 1;
                    }
                }

                public void refuse(int value) {
                    if (value < 0) {
                        throw new IllegalArgumentException("below zero");
                    }
                }

                public int stepping(int from) {
                    int steps = 0;
                    do {
                        from = from - 1;
                        steps++;
                    } while (from > 0);
                    return steps;
                }

                public int over(int[] values) {
                    int out = 0;
                    for (int one : values) {
                        out = out + one;
                    }
                    return out;
                }
            }
        "#;

        let (_, class_bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&class_bytes).expect("what was written must read");
        let translated =
            crate::dalvik::translate_class(&class).expect("and must translate to Dalvik");

        let with_tries: Vec<&str> = translated
            .virtual_methods
            .iter()
            .filter(|one| !one.tries.is_empty())
            .map(|one| one.reference.name.as_str())
            .collect();
        assert_eq!(
            with_tries,
            vec!["guarded", "cleanedUp"],
            "the two methods with a `try` are the two with a try table"
        );

        // Every protected range has to be inside the method it belongs to, and
        // every handler has to point at an instruction that exists.
        for method in &translated.virtual_methods {
            let total: u32 = method
                .instructions
                .iter()
                .map(|one| one.width() as u32)
                .sum();
            for one in &method.tries {
                assert!(
                    one.start + u32::from(one.units) <= total,
                    "{}: a protected range runs past the end of the method",
                    method.reference.name
                );
                for (_, handler) in &one.catches {
                    assert!(
                        *handler < total,
                        "{}: a handler points past the end of the method",
                        method.reference.name
                    );
                }
            }
        }

        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");
        let mut sink = crate::diag::Sink::new();
        crate::dex::read(&dex, &mut sink).expect("our own reader must read it");
        assert_eq!(sink.entries().len(), 0, "{:?}", sink.entries());
        assert!(crate::dex::integrity(&dex).unwrap().self_consistent());

        let Some(text) = dexdump_disassembly(&dex) else {
            eprintln!("dalvik: no dexdump here, so the tables were not put to the Android tool");
            return;
        };

        // The instructions that only exist because of this change.
        for wanted in [
            "packed-switch",
            "sparse-switch",
            "move-exception",
            "throw",
            "catches",
        ] {
            assert!(
                text.contains(wanted),
                "dexdump printed no {wanted:?} in\n{text}"
            );
        }
        // A payload dexdump could not follow prints as an unknown opcode or a
        // bad offset, and it says so rather than staying quiet.
        for wrong in ["<unknown", "bad offset", "unknown opcode", "???"] {
            assert!(!text.contains(wrong), "dexdump found {wrong:?} in\n{text}");
        }
        assert!(
            text.contains("Lcom/my/app/Branching;"),
            "dexdump did not find the class"
        );

        eprintln!(
            "dalvik: dexdump read back a {} byte dex holding two switch payloads, \
             a string switch, two try tables and a throw",
            dex.len()
        );
    }

    /// What a real JVM said when it was handed a class file.
    enum Verdict {
        /// It loaded and verified. The frames are right.
        Verified,
        /// It refused, and this is what it said.
        Refused(String),
        /// It is older than the class files this writes, so it never got as
        /// far as the verifier. That is a fact about the machine the tests are
        /// running on, not about the class file, and it must not be reported
        /// as either a pass or a failure.
        TooOld(String),
    }

    /// Hands a class file to a real JVM and asks it to verify it.
    ///
    /// `java -Xverify:all -cp <dir> <class>` loads and verifies before it looks
    /// for a `main`, so "no main method" means the verifier was satisfied and
    /// anything else means it was not. This is the only check that actually
    /// exercises the frames: our own reader will read a class file whose
    /// StackMapTable is nonsense, and so will `javap`.
    fn jvm_verifies(name: &str, bytes: &[u8]) -> Option<Verdict> {
        let mut verdict = None;
        for java in every_jvm_here() {
            let found = one_jvm_verifies(&java, name, bytes)?;
            // A machine can have several JVMs, and the default is often not
            // the newest. One that is too old has not disagreed with the
            // others -- it has not looked -- so keep asking.
            if !matches!(found, Verdict::TooOld(_)) {
                return Some(found);
            }
            verdict = Some(found);
        }
        verdict
    }

    /// Every `java` this machine has, the likeliest first.
    fn every_jvm_here() -> Vec<String> {
        let mut found = Vec::new();
        if let Ok(home) = std::env::var("JAVA_HOME") {
            found.push(format!("{home}/bin/java"));
        }
        if let Some(which) = std::process::Command::new("which")
            .arg("java")
            .output()
            .ok()
            .filter(|found| found.status.success())
        {
            found.push(String::from_utf8_lossy(&which.stdout).trim().to_string());
        }
        // Newest last in name order is newest last in version order for the
        // way distributions name these, so the list is walked backwards.
        if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
            let mut installed: Vec<String> = entries
                .flatten()
                .map(|entry| entry.path().join("bin/java"))
                .filter(|path| path.is_file())
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            installed.sort();
            found.extend(installed.into_iter().rev());
        }
        found.retain(|path| std::path::Path::new(path).is_file());
        found.dedup();
        found
    }

    fn one_jvm_verifies(java: &str, name: &str, bytes: &[u8]) -> Option<Verdict> {
        let directory =
            std::env::temp_dir().join(format!("omni-verify-{}-{name}", std::process::id()));
        let path = directory.join(format!("{name}.class"));
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, bytes).ok()?;

        let outcome = std::process::Command::new(java)
            .args([
                "-Xverify:all",
                "-cp",
                directory.to_str()?,
                &name.replace('/', "."),
            ])
            .output()
            .ok()?;
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&outcome.stdout),
            String::from_utf8_lossy(&outcome.stderr)
        );
        std::fs::remove_dir_all(&directory).ok();

        // A JVM older than the class files this writes stops at the version
        // number and never reaches the verifier. Counting that as a refusal
        // would turn "your JDK is old" into "the compiler is broken".
        if said.contains("UnsupportedClassVersionError") {
            return Some(Verdict::TooOld(said));
        }
        // The verifier speaks up by name when it is unhappy.
        if said.contains("VerifyError") || said.contains("ClassFormatError") {
            return Some(Verdict::Refused(said));
        }
        Some(Verdict::Verified)
    }

    #[test]
    fn a_real_jvm_verifies_what_this_writes() {
        // Branches in every shape this compiler can produce: an if with an
        // else, a while, a for, a conditional expression, both short-circuit
        // operators and a comparison. Each one is a place the verifier is told
        // what is true, and each one is a chance to have told it wrong.
        let source = r#"
            public class Branchy {
                private int count;

                public int classify(int value) {
                    if (value < 0) {
                        return -1;
                    } else {
                        return 1;
                    }
                }

                public int sumTo(int limit) {
                    int total = 0;
                    for (int i = 0; i < limit; i++) {
                        total = total + i;
                    }
                    return total;
                }

                public int countDown(int from) {
                    int steps = 0;
                    while (from > 0) {
                        from = from - 1;
                        steps++;
                    }
                    return steps;
                }

                public int pick(boolean which, int left, int right) {
                    return which ? left : right;
                }

                public boolean between(int value, int low, int high) {
                    return value >= low && value <= high;
                }

                public boolean outside(int value, int low, int high) {
                    return value < low || value > high;
                }

                public boolean isNot(boolean value) {
                    return !value;
                }

                public long widened(int value) {
                    long total = value;
                    if (total > 100) {
                        total = total * 2;
                    }
                    return total;
                }

                public String describe() {
                    return "count is " + count;
                }
            }
        "#;

        let (_, bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&bytes).expect("and read back");
        assert_eq!(class.major_version, CLASS_MAJOR, "Java 25 class files");

        // Every method that branches has to carry a table, or a version 69
        // class file is refused before it is even run.
        let with_frames = class
            .methods
            .iter()
            .filter(|method| {
                method
                    .code
                    .as_ref()
                    .is_some_and(|code| code.bytes.iter().any(|byte| (0x99..=0xa7).contains(byte)))
            })
            .count();
        assert!(with_frames >= 7, "only {with_frames} methods branch");

        match jvm_verifies("Branchy", &bytes) {
            None => {
                eprintln!("java: no JVM here, so the frames were not put to a verifier");
                return;
            }
            Some(Verdict::TooOld(said)) => {
                eprintln!(
                    "java: the JVM here is older than Java {LANGUAGE_RELEASE}, so the frames \
                     were not put to a verifier -- it said {:?}",
                    said.lines().last().unwrap_or_default()
                );
                return;
            }
            Some(Verdict::Refused(said)) => {
                panic!("a real JVM refused a class file this wrote:\n{said}")
            }
            Some(Verdict::Verified) => {}
        }

        eprintln!(
            "java: a real JVM verified a version {CLASS_MAJOR} class file with {with_frames} branching methods"
        );
    }

    #[test]
    fn a_real_jvm_verifies_the_whole_of_what_java_25_adds() {
        // Every construct this compiler learned, in one class, because the
        // verifier is the only thing that will say whether the frames and the
        // exception table are right. Our own reader will read nonsense; so
        // will `javap`.
        let source = "\
public class Wide {
    private int total;

    public String grade(int score) {
        switch (score) {
            case 0:
            case 1:
                return \"low\";
            case 2:
                return \"middle\";
            default:
                return \"high\";
        }
    }

    public int weigh(int kind) {
        int out = 0;
        switch (kind) {
            case 1 -> out = 10;
            case 2, 3 -> out = 20;
            default -> out = 30;
        }
        return out;
    }

    public int scattered(int key) {
        switch (key) {
            case 1: return 1;
            case 1000: return 2;
            case 1000000: return 3;
            default: return 0;
        }
    }

    public int named(String word) {
        switch (word) {
            case \"one\": return 1;
            case \"two\": return 2;
            default: return 0;
        }
    }

    public int guarded(int by) {
        int seen = 0;
        try {
            seen = 100 / by;
        } catch (ArithmeticException e) {
            seen = -1;
        } finally {
            total = total + 1;
        }
        return seen;
    }

    public String caught(String text) {
        try {
            return text.substring(0, 2);
        } catch (IndexOutOfBoundsException | NullPointerException e) {
            return e.getMessage();
        }
    }

    public int leavingThroughFinally(int value) {
        try {
            if (value > 0) {
                return value;
            }
            return 0;
        } finally {
            total = total + 1;
        }
    }

    public void refuse(int value) {
        if (value < 0) {
            throw new IllegalArgumentException(\"below zero\");
        }
    }

    public int stepDown(int from) {
        int steps = 0;
        do {
            from = from - 1;
            steps++;
        } while (from > 0);
        return steps;
    }

    public int sum(int[] values) {
        int out = 0;
        for (int one : values) {
            out = out + one;
        }
        return out;
    }

    public int firstNegative(int[][] rows) {
        outer:
        for (int[] row : rows) {
            for (int one : row) {
                if (one < 0) {
                    break outer;
                }
                if (one == 0) {
                    continue outer;
                }
                total = total + one;
            }
        }
        return total;
    }

    public String inferred(String text) {
        var held = text;
        var count = held.length();
        var doubled = count * 2;
        return held + doubled;
    }

    public String block() {
        return \"\"\"
            first
              second
            \"\"\";
    }

    public int nestedGuard(int value) {
        int out = 0;
        try {
            try {
                out = 10 / value;
            } finally {
                out = out + 1;
            }
        } catch (ArithmeticException e) {
            out = -1;
        }
        return out;
    }

    public int breakingOutOfASwitchInALoop(int[] values) {
        int out = 0;
        for (int one : values) {
            switch (one) {
                case 0:
                    break;
                case 1:
                    out = out + 1;
                    break;
                default:
                    out = out + 2;
            }
        }
        return out;
    }
}
";

        let (name, bytes) = compile_one(source, &empty()).expect("this must compile");
        assert_eq!(name, "Wide.class");
        let class = crate::jvm::read(&bytes).expect("and read back");
        assert_eq!(class.major_version, CLASS_MAJOR);

        let with_handlers = class
            .methods
            .iter()
            .filter(|method| {
                method
                    .code
                    .as_ref()
                    .is_some_and(|code| !code.handlers.is_empty())
            })
            .count();
        assert_eq!(
            with_handlers, 4,
            "the four methods with a `try` are the four with an exception table"
        );

        match jvm_verifies("Wide", &bytes) {
            None => {
                eprintln!("java: no JVM here, so the frames were not put to a verifier");
                return;
            }
            Some(Verdict::TooOld(_)) => {
                eprintln!(
                    "java: the JVM here is older than Java {LANGUAGE_RELEASE}, so the frames \
                     were not put to a verifier"
                );
                return;
            }
            Some(Verdict::Refused(said)) => {
                panic!("a real JVM refused a class file this wrote:\n{said}")
            }
            Some(Verdict::Verified) => {}
        }

        eprintln!(
            "java: a real JVM verified switch, try/catch/finally, throw, do/while, \
             enhanced for, labelled break, var and text blocks -- {} methods, {} with \
             exception tables",
            class.methods.len(),
            with_handlers
        );
    }

    #[test]
    fn a_local_past_the_fourth_slot_is_stored_with_the_right_instruction() {
        // The compact store instructions live four apart and the plain ones
        // one apart, so a table that is off by one is right for the first four
        // slots of every type and wrong for the fifth. Nothing but a verifier
        // notices: the bytes still read, `javap` still prints them, and the
        // method still looks like a method. So every type gets more than four
        // locals here, and a real JVM is asked.
        let source = r#"
            public class Slots {
                public double crowded(int a, long b, float c, double d, String e) {
                    int i1 = a; int i2 = a; int i3 = a; int i4 = a; int i5 = a; int i6 = a;
                    long l1 = b; long l2 = b; long l3 = b; long l4 = b; long l5 = b;
                    float f1 = c; float f2 = c; float f3 = c; float f4 = c; float f5 = c;
                    double d1 = d; double d2 = d; double d3 = d; double d4 = d; double d5 = d;
                    String s1 = e; String s2 = e; String s3 = e; String s4 = e; String s5 = e;
                    return i6 + l5 + f5 + d5 + s5.length();
                }
            }
        "#;
        let (_, bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&bytes).expect("and read back");
        let code = class
            .methods
            .iter()
            .find(|method| method.name == "crowded")
            .and_then(|method| method.code.as_ref())
            .expect("the method is there with its code");
        assert!(
            code.max_locals > 40,
            "only {} slots, which is not enough to reach past the compact forms",
            code.max_locals
        );

        match jvm_verifies("Slots", &bytes) {
            None | Some(Verdict::TooOld(_)) => {
                eprintln!("java: no JVM new enough here to check the slot instructions");
            }
            Some(Verdict::Refused(said)) => {
                panic!("a real JVM refused a class file this wrote:\n{said}")
            }
            Some(Verdict::Verified) => eprintln!(
                "java: {} local slots of five types, every load and store verified",
                code.max_locals
            ),
        }
    }

    #[test]
    fn the_shapes_ordinary_java_is_written_in_all_compile() {
        // A list rather than one large class, so that a refusal names the one
        // thing that broke instead of the file it was in.
        let cases: &[(&str, &str)] = &[
            ("a field with a value", "public class A { private int n = 5; int f() { return n; } }"),
            ("two names in one declaration", "public class A { int f() { int a = 1, b = 2; return a + b; } }"),
            ("a static initialiser", "public class A { static int n; static { n = 1; } }"),
            ("an instance initialiser", "public class A { int n; { n = 1; } }"),
            ("a class with a type parameter", "public class A<T> { }"),
            ("a varargs method", "public class A { int f(int... v) { return v.length; } }"),
            ("a static call", "public class A { int f() { return Math.max(1, 2); } }"),
            ("printing", "public class A { void f() { System.out.println(\"hi\"); } }"),
            ("printing a number, which is a different method", "public class A { void f(int n) { System.out.println(n); } }"),
            ("joining strings", "public class A { String f(int n) { return \"n=\" + n; } }"),
            ("throwing with a message", "public class A { void f() { throw new IllegalStateException(\"no\"); } }"),
            ("an array of objects", "public class A { String[] f() { return new String[3]; } }"),
            ("reading a number out of text", "public class A { int f() { return Integer.parseInt(\"1\"); } }"),
            ("a switch over characters", "public class A { int f(char c) { switch (c) { case 'a': return 1; default: return 0; } } }"),
            ("a labelled loop", "public class A { void f() { a: while (true) { break a; } } }"),
            ("compound assignment", "public class A { int f(int n) { n += 2; n *= 3; return n; } }"),
            ("the bitwise operators", "public class A { int f(int n) { return (n << 2) | (n >>> 1) & ~n ^ 3; } }"),
            ("long arithmetic", "public class A { long f(long n) { return n * 2L + 1L; } }"),
            ("a conditional inside a conditional", "public class A { int f(int n) { return n > 0 ? 1 : n < 0 ? -1 : 0; } }"),
            ("a final parameter", "public class A { int f(final int n) { return n; } }"),
            ("a call on this", "public class A { int a() { return 1; } int b() { return this.a(); } }"),
            ("a constructor calling up", "public class A { A() { super(); } }"),
            ("a constructor handing off", "public class A { A() { this(1); } A(int n) { } }"),
            ("underscores in a number", "public class A { int f() { return 1_000_000; } }"),
            ("hexadecimal and binary", "public class A { int f() { return 0xFF + 0b1010; } }"),
            ("a switch used for its value", "public class A { int f(int v) { return switch (v) { case 1 -> 2; default -> 3; }; } }"),
            ("a pattern in an instanceof", "public class A { int f(Object o) { if (o instanceof String s) { return s.length(); } return 0; } }"),
            ("a text block", "public class A { String f() { return \"\"\"\n    hello\n    \"\"\"; } }"),
            ("var", "public class A { int f() { var n = 1; return n; } }"),
            ("do while", "public class A { int f(int n) { do { n--; } while (n > 0); return n; } }"),
            ("try with a finally", "public class A { int f(int n) { try { return 1 / n; } finally { n = 0; } } }"),
            ("catching two things at once", "public class A { int f(String s) { try { return s.length(); } catch (NullPointerException | ClassCastException e) { return -1; } } }"),
            ("an enhanced for over an array", "public class A { int f(int[] v) { int t = 0; for (int one : v) { t += one; } return t; } }"),
        ];
        for (what, source) in cases {
            if let Err(refused) = compile_one(source, &empty()) {
                panic!("{what} must compile: {} {}", refused.code, refused.message);
            }
        }
        eprintln!(
            "java: {} shapes ordinary Java is written in, all compiled",
            cases.len()
        );
    }

    #[test]
    fn a_real_jvm_verifies_the_shape_a_class_is_actually_written_in() {
        // Field values, initialiser blocks, one constructor handing off to
        // another, `switch` used for its value, patterns, varargs, and
        // `System.out` -- the things a class has that a method body does not.
        let source = "\
public class Shaped {
    private static final int LIMIT = 10;
    private static int made;
    static {
        made = 0;
    }

    private int count = 1;
    private String label = \"none\";
    private final int[] slots = new int[LIMIT];
    private int a = 1, b = 2, c = a + b;

    {
        made = made + 1;
    }

    public Shaped() {
        this(\"unnamed\");
    }

    public Shaped(String named) {
        super();
        this.label = named;
    }

    public int total() {
        return count + a + b + c;
    }

    public String name() {
        return label;
    }

    public int sum(int... values) {
        int out = 0;
        for (int one : values) {
            out = out + one;
        }
        return out;
    }

    public int describe(int kind) {
        return switch (kind) {
            case 0 -> 100;
            case 1, 2 -> 200;
            default -> {
                int worked = kind * 3;
                yield worked;
            }
        };
    }

    public String named(String key) {
        return switch (key) {
            case \"one\": yield \"first\";
            case \"two\": yield \"second\";
            default: yield \"other\";
        };
    }

    public int measure(Object thing) {
        if (thing instanceof String text) {
            return text.length();
        }
        return -1;
    }

    public void say() {
        System.out.println(\"count is \" + count);
        System.out.println(Math.max(count, LIMIT));
    }

    public int guarded(String text) {
        try {
            return Integer.parseInt(text);
        } catch (NumberFormatException e) {
            return -1;
        } finally {
            made = made + 1;
        }
    }
}
";

        let (name, bytes) = compile_one(source, &empty()).expect("this must compile");
        assert_eq!(name, "Shaped.class");
        let class = crate::jvm::read(&bytes).expect("and read back");

        // The class initialiser exists because static fields were given values.
        assert!(
            class.methods.iter().any(|one| one.name == "<clinit>"),
            "a class with static values needs a class initialiser"
        );
        assert_eq!(
            class
                .methods
                .iter()
                .filter(|one| one.name == "<init>")
                .count(),
            2,
            "both constructors are there"
        );

        match jvm_verifies("Shaped", &bytes) {
            None | Some(Verdict::TooOld(_)) => {
                eprintln!("java: no JVM new enough here to verify the shape");
                return;
            }
            Some(Verdict::Refused(said)) => {
                panic!("a real JVM refused a class file this wrote:\n{said}")
            }
            Some(Verdict::Verified) => {}
        }

        // And it has to reach a device, not just a JVM.
        let translated =
            crate::dalvik::translate_class(&class).expect("and must translate to Dalvik");
        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            for wanted in ["LShaped;", "<clinit>", "sget", "sput", "check-cast"] {
                assert!(text.contains(wanted), "dexdump printed no {wanted:?}");
            }
        }

        eprintln!(
            "java: field values, initialiser blocks, chained constructors, switch \
             expressions, patterns, varargs and System.out -- {} methods verified",
            class.methods.len()
        );
    }

    #[test]
    fn an_interface_and_the_class_that_implements_it_compile_together() {
        // Two types in one file, naming each other, and neither of them a
        // class file yet when the other is compiled.
        let source = r#"
            package com.my.app;

            interface Shape {
                int SIDES = 0;

                double area();

                default String describe() {
                    return "a shape";
                }

                static Shape none() {
                    return null;
                }
            }

            public class Square implements Shape {
                private final double side;

                public Square(double side) {
                    this.side = side;
                }

                public double area() {
                    return side * side;
                }

                public double twice() {
                    return area() * 2;
                }

                public String tell(Shape other) {
                    return other.describe();
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("both must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec!["com/my/app/Shape.class", "com/my/app/Square.class"]
        );

        let shape = crate::jvm::read(&produced[0].1).expect("the interface must read back");
        assert!(
            shape.access_flags & 0x0200 != 0,
            "an interface has to say it is one"
        );
        assert!(shape.access_flags & 0x0400 != 0, "and that it is abstract");
        assert!(
            shape.methods.iter().any(|one| one.name == "<clinit>"),
            "a field of an interface is a constant, and constants are set once"
        );
        assert!(
            !shape.methods.iter().any(|one| one.name == "<init>"),
            "an interface has no constructor"
        );

        let square = crate::jvm::read(&produced[1].1).expect("the class must read back");
        assert_eq!(square.interfaces, vec!["com.my.app.Shape"]);

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class").rsplit('/').next().unwrap();
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        // And both of them reach a device.
        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        let dex = crate::dexwrite::write(&classes, &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/Shape;"));
            assert!(text.contains("Lcom/my/app/Square;"));
            assert!(
                text.contains("invoke-interface"),
                "a call on an interface is an interface call"
            );
        }

        eprintln!("java: an interface and the class implementing it, from one file, both verified");
    }

    #[test]
    fn an_enum_becomes_the_class_it_stands_for() {
        let source = r#"
            package com.my.app;

            public enum Planet {
                MERCURY(3.30),
                VENUS(4.87),
                EARTH(5.97);

                private final double mass;

                Planet(double mass) {
                    this.mass = mass;
                }

                public double mass() {
                    return mass;
                }

                public boolean heavierThan(Planet other) {
                    return mass > other.mass();
                }
            }
        "#;

        let (name, bytes) = compile_one(source, &empty()).expect("this must compile");
        assert_eq!(name, "com/my/app/Planet.class");
        let class = crate::jvm::read(&bytes).expect("and read back");
        assert_eq!(class.superclass.as_deref(), Some("java.lang.Enum"));
        assert!(class.access_flags & 0x4000 != 0, "an enum says it is one");
        assert!(class.access_flags & 0x0010 != 0, "and that it is final");

        let fields: Vec<&str> = class.fields.iter().map(|one| one.name.as_str()).collect();
        for wanted in ["MERCURY", "VENUS", "EARTH", "$VALUES", "mass"] {
            assert!(fields.contains(&wanted), "{fields:?}");
        }
        let methods: Vec<&str> = class.methods.iter().map(|one| one.name.as_str()).collect();
        for wanted in [
            "<init>",
            "<clinit>",
            "values",
            "valueOf",
            "mass",
            "heavierThan",
        ] {
            assert!(methods.contains(&wanted), "{methods:?}");
        }

        match jvm_verifies("com/my/app/Planet", &bytes) {
            None | Some(Verdict::TooOld(_)) => {
                eprintln!("java: no JVM new enough here to verify an enum");
                return;
            }
            Some(Verdict::Refused(said)) => panic!("a real JVM refused an enum:\n{said}"),
            Some(Verdict::Verified) => {}
        }

        let translated = crate::dalvik::translate_class(&class).expect("must translate");
        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/Planet;"));
            assert!(text.contains("<clinit>"));
        }

        // And a switch over it, in a class that is handed the enum's own
        // class file -- which is how a device would see it.
        let mut classpath = empty();
        classpath.learn(&class).expect("the enum must be learnable");
        let over = r#"
            package com.my.app;

            public class Weigh {
                public int rank(Planet which) {
                    switch (which) {
                        case MERCURY: return 1;
                        case VENUS: return 2;
                        default: return 3;
                    }
                }

                public String tell(Planet which) {
                    return switch (which) {
                        case EARTH -> "home";
                        default -> which.name();
                    };
                }
            }
        "#;
        let (_, weighed) = compile_one(over, &classpath).expect("a switch over an enum compiles");
        match jvm_verifies("com/my/app/Weigh", &weighed) {
            None | Some(Verdict::TooOld(_)) => {}
            Some(Verdict::Refused(said)) => {
                panic!("a real JVM refused a switch over an enum:\n{said}")
            }
            Some(Verdict::Verified) => {}
        }

        eprintln!(
            "java: an enum with three constants, a field and a constructor, and a \
             switch over it, verified"
        );
    }

    #[test]
    fn a_record_becomes_the_class_it_stands_for() {
        let source = r#"
            package com.my.app;

            public record Point(int x, int y) {
                public int sum() {
                    return x + y;
                }

                public static Point origin() {
                    return new Point(0, 0);
                }
            }
        "#;

        let (name, bytes) = compile_one(source, &empty()).expect("this must compile");
        assert_eq!(name, "com/my/app/Point.class");
        let class = crate::jvm::read(&bytes).expect("and read back");
        assert!(class.access_flags & 0x0010 != 0, "a record is final");
        assert!(class.interfaces.iter().any(|one| one == "java.lang.Record"));

        let fields: Vec<&str> = class.fields.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(fields, vec!["x", "y"]);
        let methods: Vec<&str> = class.methods.iter().map(|one| one.name.as_str()).collect();
        for wanted in ["<init>", "x", "y", "toString", "sum", "origin"] {
            assert!(methods.contains(&wanted), "{methods:?}");
        }

        match jvm_verifies("com/my/app/Point", &bytes) {
            None | Some(Verdict::TooOld(_)) => {
                eprintln!("java: no JVM new enough here to verify a record");
                return;
            }
            Some(Verdict::Refused(said)) => panic!("a real JVM refused a record:\n{said}"),
            Some(Verdict::Verified) => {}
        }

        let translated = crate::dalvik::translate_class(&class).expect("must translate");
        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/Point;"));
        }

        eprintln!("java: a record with two components, an accessor each and a toString, verified");
    }

    #[test]
    fn code_that_holds_things_and_closes_them_compiles() {
        let source = r#"
            package com.my.app;

            import java.util.ArrayList;
            import java.util.List;
            import java.util.Map;
            import java.util.HashMap;

            public class Holding {
                public int count(List names) {
                    int total = 0;
                    for (Object one : names) {
                        total = total + 1;
                    }
                    return total;
                }

                public List made() {
                    List out = new ArrayList();
                    out.add("first");
                    out.add("second");
                    return out;
                }

                public String look(Map by, String key) {
                    Object found = by.get(key);
                    if (found instanceof String text) {
                        return text;
                    }
                    return "";
                }

                public Map counted() {
                    Map out = new HashMap();
                    out.put("one", Integer.valueOf(1));
                    return out;
                }

                public int walk(Iterable things) {
                    int seen = 0;
                    for (Object one : things) {
                        seen++;
                    }
                    return seen;
                }

                public void closing(AutoCloseable thing) throws Exception {
                    try (AutoCloseable held = thing) {
                        System.out.println("using it");
                    }
                }
            }
        "#;

        let (_, bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&bytes).expect("and read back");

        match jvm_verifies("com/my/app/Holding", &bytes) {
            None | Some(Verdict::TooOld(_)) => {
                eprintln!("java: no JVM new enough here to verify collections code");
                return;
            }
            Some(Verdict::Refused(said)) => panic!("a real JVM refused it:\n{said}"),
            Some(Verdict::Verified) => {}
        }

        let translated = crate::dalvik::translate_class(&class).expect("must translate");
        let dex = crate::dexwrite::write(&[translated], &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(
                text.contains("invoke-interface"),
                "a call on a collection is an interface call"
            );
        }

        eprintln!("java: lists, maps, iterators and a try that closes what it opened, verified");
    }

    #[test]
    fn a_class_written_where_it_is_used_becomes_a_class_of_its_own() {
        let source = r#"
            package com.my.app;

            import android.app.Activity;
            import android.os.Bundle;
            import android.view.View;
            import android.widget.Button;

            public final class MainActivity extends Activity {
                private int taps;

                @Override
                protected void onCreate(Bundle state) {
                    super.onCreate(state);
                    final Button button = new Button(this);
                    final String said = "tapped";
                    button.setOnClickListener(new View.OnClickListener() {
                        @Override
                        public void onClick(View which) {
                            count();
                            button.setText(said + taps);
                        }
                    });
                    setContentView(button);
                }

                private void count() {
                    taps = taps + 1;
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("this must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "com/my/app/MainActivity.class",
                "com/my/app/MainActivity$1.class"
            ]
        );

        let listener = crate::jvm::read(&produced[1].1).expect("the listener must read back");
        assert_eq!(
            listener.interfaces,
            vec!["android.view.View$OnClickListener"]
        );
        let fields: Vec<&str> = listener
            .fields
            .iter()
            .map(|one| one.name.as_str())
            .collect();
        // The enclosing instance, and every local the body reads.
        for wanted in ["$outer", "button", "said"] {
            assert!(fields.contains(&wanted), "{fields:?}");
        }

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        let dex = crate::dexwrite::write(&classes, &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/MainActivity$1;"));
            assert!(text.contains("$outer"));
        }

        eprintln!(
            "java: a listener written where it is used, holding its enclosing instance \
             and two locals, verified"
        );
    }

    #[test]
    fn a_lambda_becomes_the_interface_it_is_handed_to() {
        let source = r#"
            package com.my.app;

            import android.app.Activity;
            import android.os.Bundle;
            import android.view.View;
            import android.widget.Button;

            public final class MainActivity extends Activity {
                private int taps;

                @Override
                protected void onCreate(Bundle state) {
                    super.onCreate(state);
                    final Button button = new Button(this);
                    final String said = "tapped ";
                    button.setOnClickListener(which -> {
                        taps = taps + 1;
                        button.setText(said + taps);
                    });
                    Runnable later = () -> button.setText("done");
                    later.run();
                    setContentView(button);
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("this must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "com/my/app/MainActivity.class",
                "com/my/app/MainActivity$1.class",
                "com/my/app/MainActivity$2.class"
            ],
            "one class per lambda, numbered in the order they were written"
        );

        let listener = crate::jvm::read(&produced[1].1).unwrap();
        assert_eq!(
            listener.interfaces,
            vec!["android.view.View$OnClickListener"]
        );
        assert!(listener.methods.iter().any(|one| one.name == "onClick"));

        let later = crate::jvm::read(&produced[2].1).unwrap();
        assert_eq!(later.interfaces, vec!["java.lang.Runnable"]);
        assert!(later.methods.iter().any(|one| one.name == "run"));

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        let dex = crate::dexwrite::write(&classes, &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/MainActivity$2;"));
            // No invokedynamic: the class is written out, which is what
            // Android's own tooling does to a lambda anyway.
            assert!(!text.contains("invoke-polymorphic"));
        }

        eprintln!("java: two lambdas, two classes, no invokedynamic, verified");
    }

    #[test]
    fn a_class_written_inside_a_class_is_a_class_of_its_own() {
        let source = r#"
            package com.my.app;

            public final class Store {
                private final Entry first;

                public Store() {
                    first = new Entry("one", 1);
                }

                public String name() {
                    return first.name();
                }

                public Kind kindOf() {
                    return Kind.SMALL;
                }

                public static final class Entry {
                    private final String name;
                    private final int weight;

                    public Entry(String name, int weight) {
                        this.name = name;
                        this.weight = weight;
                    }

                    public String name() {
                        return name;
                    }

                    public int weight() {
                        return weight;
                    }
                }

                public enum Kind {
                    SMALL, LARGE
                }

                public record Pair(int left, int right) { }

                public interface Watcher {
                    void changed(Entry which);
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("all of it must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        for wanted in [
            "com/my/app/Store.class",
            "com/my/app/Store$Entry.class",
            "com/my/app/Store$Kind.class",
            "com/my/app/Store$Pair.class",
            "com/my/app/Store$Watcher.class",
        ] {
            assert!(names.contains(&wanted), "{names:?}");
        }

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        let dex = crate::dexwrite::write(&classes, &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            assert!(text.contains("Lcom/my/app/Store$Entry;"));
            assert!(text.contains("Lcom/my/app/Store$Kind;"));
        }

        eprintln!(
            "java: a class, a nested class, a nested enum, a nested record and a nested \
             interface -- {} classes from one file, all verified",
            produced.len()
        );
    }

    #[test]
    fn a_class_that_belongs_to_an_instance_is_made_from_one() {
        let source = r#"
            package com.my.app;

            public final class Counter {
                private int held;

                public Step step() {
                    return new Step(2);
                }

                public int total() {
                    Step one = new Step(3);
                    one.take();
                    return held;
                }

                public final class Step {
                    private final int by;

                    public Step(int by) {
                        this.by = by;
                    }

                    public void take() {
                        held = held + by;
                    }

                    public int seen() {
                        return held;
                    }
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("both must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec!["com/my/app/Counter.class", "com/my/app/Counter$Step.class"]
        );

        let step = crate::jvm::read(&produced[1].1).unwrap();
        let fields: Vec<&str> = step.fields.iter().map(|one| one.name.as_str()).collect();
        assert!(fields.contains(&"$outer"), "{fields:?}");

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        crate::dexwrite::write(&classes, &[]).expect("the dex must be written");

        eprintln!("java: a class belonging to an instance, reading and writing what it holds");
    }

    #[test]
    fn a_method_reference_is_a_lambda_written_shorter() {
        let source = r#"
            package com.my.app;

            public final class Work {
                private String held = "";

                public void run(Runnable which) {
                    which.run();
                }

                public void tidy() {
                    held = "";
                }

                public void go() {
                    run(this::tidy);
                    run(Work::shared);
                }

                public static void shared() {
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("this must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "com/my/app/Work.class",
                "com/my/app/Work$1.class",
                "com/my/app/Work$2.class"
            ]
        );
        for held in &produced[1..] {
            let class = crate::jvm::read(&held.1).unwrap();
            assert_eq!(class.interfaces, vec!["java.lang.Runnable"]);
        }

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        eprintln!("java: a method reference on an instance and one on a class, both verified");
    }

    #[test]
    fn the_mechanics_ordinary_java_leans_on_all_verify() {
        // Arrays written out, more than one dimension, boxing, locks,
        // assertions, class literals, varargs and stepping an array element:
        // the things a person writes without thinking about them, which is
        // exactly why every one of them has to be right.
        let source = r#"
            package com.my.app;

            public final class Mechanics {
                private int[] held = {1, 2, 3};
                private final int[][] grid = new int[3][4];
                private static final String TAG = "Mechanics";

                public int[] made() {
                    return new int[]{4, 5, 6};
                }

                public int[][] table() {
                    return new int[][]{{1, 2}, {3, 4}};
                }

                public int corner(int[][] of) {
                    return of[0][1];
                }

                public Object boxed(int value) {
                    return value;
                }

                public int unboxed(Integer value) {
                    return value;
                }

                public void collected(java.util.List into) {
                    into.add(1);
                    into.add("two");
                }

                public synchronized int guardedByTheMethod() {
                    return held.length;
                }

                public int guardedByABlock() {
                    synchronized (this) {
                        held[0] = held[0] + 1;
                        return held[0];
                    }
                }

                public void checked(int value) {
                    assert value > 0;
                    assert value < 100 : "too big";
                }

                public String named() {
                    return Mechanics.class.getName() + int.class.getName();
                }

                public String said(int count) {
                    return String.format("%s has %d", TAG, Integer.valueOf(count));
                }

                public void stepped() {
                    held[0]++;
                    held[1] += 5;
                    held[2] -= 2;
                    grid[0][0]++;
                }

                public int total() {
                    int sum = 0;
                    for (int[] row : grid) {
                        for (int one : row) {
                            sum += one;
                        }
                    }
                    return sum;
                }

                public final class Inside {
                    public int outerFirst() {
                        return Mechanics.this.held[0];
                    }
                }
            }
        "#;

        let produced = compile(source, &empty()).expect("all of this must compile");
        assert_eq!(produced.len(), 2, "the class and the one inside it");

        let main = crate::jvm::read(&produced[0].1).unwrap();
        assert!(
            main.fields
                .iter()
                .any(|one| one.name == "$assertionsDisabled"),
            "an `assert` is guarded by a flag the class works out once"
        );
        assert!(
            main.methods
                .iter()
                .any(|one| one.name == "guardedByTheMethod" && one.access_flags & 0x0020 != 0),
            "a synchronized method says so in its flags"
        );

        for (name, bytes) in &produced {
            let simple = name.trim_end_matches(".class");
            match jvm_verifies(simple, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify {name}");
                    return;
                }
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {name}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        let mut classes = Vec::new();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).unwrap();
            classes.push(crate::dalvik::translate_class(&class).expect("must translate"));
        }
        let dex = crate::dexwrite::write(&classes, &[]).expect("the dex must be written");
        if let Some(text) = dexdump_disassembly(&dex) {
            for wanted in [
                "Lcom/my/app/Mechanics;",
                "monitor-enter",
                "monitor-exit",
                "const-class",
                "filled-new-array|new-array",
            ] {
                let any = wanted.split('|').any(|one| text.contains(one));
                assert!(any, "dexdump printed no {wanted:?}");
            }
        }

        eprintln!(
            "java: arrays written out, two dimensions, boxing, locks, assertions, class \
             literals, varargs and stepping an element -- all verified"
        );
    }

    #[test]
    fn a_class_written_inside_a_class_is_reached_through_its_holder() {
        // `R.string.app_name` is how every Android application reaches a
        // resource, and it is a static field of a class written inside a
        // class. Written down it looks exactly like a field of a field, which
        // is why it has to be tried as a class first.
        let held = r#"
            package com.my.app;

            public final class R {
                public static final class string {
                    public static final int app_name = 2130903040;
                }
                public static final class drawable {
                    public static final int icon = 2130837504;
                }
            }
        "#;
        let produced = compile(held, &empty()).expect("the holder must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"com/my/app/R$string.class"), "{names:?}");

        let mut classpath = empty();
        for (_, bytes) in &produced {
            let class = crate::jvm::read(bytes).expect("read back");
            classpath.learn(&class).expect("and learn");
        }

        let user = r#"
            package com.my.app;

            public class Screen {
                public int title() {
                    return R.string.app_name;
                }
                public int icon() {
                    return R.drawable.icon;
                }
            }
        "#;
        let (_, bytes) = compile_one(user, &classpath).expect("and reaching it must compile");
        match jvm_verifies("com/my/app/Screen", &bytes) {
            None | Some(Verdict::TooOld(_)) => {}
            Some(Verdict::Refused(said)) => panic!("a real JVM refused it:\n{said}"),
            Some(Verdict::Verified) => {}
        }

        // A name that is not there is still refused, rather than read as
        // something else and quietly emitted.
        let missing = r#"
            package com.my.app;
            public class Screen { public int f() { return R.string.nothing; } }
        "#;
        let refused = compile_one(missing, &classpath).expect_err("must be refused");
        assert_eq!(refused.code, "EJ213");

        eprintln!("java: a class inside a class, reached the way Android reaches R");
    }

    #[test]
    fn files_compiled_together_can_name_each_other() {
        // A project is more than one file, and a class in one names a class in
        // another without either saying where it lives. Compiling them one
        // after the other would mean the first could not see the second.
        let sources = vec![
            (
                "Greeter.java".to_string(),
                r#"
                    package com.my.app;

                    public class Greeter {
                        private final Name held;

                        public Greeter(Name held) {
                            this.held = held;
                        }

                        public String greeting() {
                            return "hello " + held.text();
                        }
                    }
                "#
                .to_string(),
            ),
            (
                "Name.java".to_string(),
                r#"
                    package com.my.app;

                    public class Name {
                        private final String text;

                        public Name(String text) {
                            this.text = text;
                        }

                        public String text() {
                            return text;
                        }

                        public static Greeter of(String text) {
                            return new Greeter(new Name(text));
                        }
                    }
                "#
                .to_string(),
            ),
        ];
        let produced = compile_together(&sources, &empty()).expect("both must compile");
        let names: Vec<&str> = produced.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"com/my/app/Greeter.class"), "{names:?}");
        assert!(names.contains(&"com/my/app/Name.class"), "{names:?}");

        for (file, bytes) in &produced {
            let named = file.trim_end_matches(".class");
            match jvm_verifies(named, bytes) {
                None | Some(Verdict::TooOld(_)) => return,
                Some(Verdict::Refused(said)) => panic!("a real JVM refused {file}:\n{said}"),
                Some(Verdict::Verified) => {}
            }
        }

        // Named the other way round, the answer is the same.
        let backwards = vec![sources[1].clone(), sources[0].clone()];
        let again = compile_together(&backwards, &empty()).expect("order must not matter");
        assert_eq!(again.len(), produced.len());

        // And one type in two files is refused rather than silently written
        // over.
        let twice = vec![sources[0].clone(), sources[0].clone()];
        let refused = compile_together(&twice, &empty()).expect_err("one type, one place");
        assert_eq!(refused.code, "EJ253");
        assert!(refused
            .context
            .iter()
            .any(|line| line.contains("Greeter.java")));

        eprintln!("java: files compiled together, whichever order they arrive in");
    }

    #[test]
    fn a_branch_inside_an_expression_leaves_the_stack_where_the_verifier_expects_it() {
        // A frame says what is on the stack where a branch lands. A branch
        // written in the middle of an expression lands somewhere that already
        // has the rest of the expression under it, and a frame that forgets
        // that is a claim the verifier throws the class out over. Every shape
        // that branches without being a statement is here.
        let source = r#"
            public class Branchy {
                public int pick(int a, int b) { return a + b; }

                public int inACall(boolean c) { return pick(7, c ? 1 : 2); }

                public String inAnArgument(boolean c, String s) {
                    return s.substring(c ? 0 : 1);
                }

                public boolean between(int x, int low, int high) {
                    return x >= low && x <= high;
                }

                public int inASwitch(int i) {
                    return pick(1, switch (i) { case 0 -> 10; default -> 20; });
                }

                public int inAnArray(boolean c) {
                    int[] made = { c ? 1 : 2, 3 };
                    return made[0];
                }

                public void checked(int value) {
                    assert value > 0;
                    assert value < 100 : "too big";
                }

                public int measured(Object o) {
                    if (o instanceof String s) {
                        return s.length();
                    }
                    return -1;
                }

                // A `new` whose argument branches: between the `new` and the
                // constructor the verifier holds an "uninitialized" type that
                // is nothing else, and a frame in between has to say so.
                public String made(boolean c) {
                    return new String(c ? "a" : "b");
                }

                // A branch with a long under it, which is two slots and a
                // `top` the frame has to name.
                public long wide(boolean c, long held) {
                    return held + (c ? 1 : 2);
                }

                public double alsoWide(boolean c, double held) {
                    return held * (c ? 1.5 : 2.5);
                }

                // A field read under a branch, and a branch inside an index.
                private int[] kept = { 1, 2, 3 };

                public int fromAField(boolean c) {
                    return kept[c ? 0 : 1];
                }

                public String joined(boolean c, String s) {
                    return "x" + (c ? s : "y") + 1;
                }

                public int nested(boolean a, boolean b) {
                    return pick(a ? 1 : 2, b ? 3 : 4);
                }

                public int howMany(boolean c) {
                    return (c ? kept : new int[0]).length;
                }

                public Object either(boolean c, Object o) {
                    return c ? o : this;
                }

                // A call that leaves nothing, with a branch after it: a frame
                // written there must not have the return of a void method on
                // it.
                public int nothingLeft(boolean c) {
                    pick(1, 2);
                    return c ? 1 : 2;
                }

                public int inALoop(boolean c) {
                    int sum = 0;
                    int i = 0;
                    while (i < (c ? 3 : 4)) {
                        sum += c ? i : -i;
                        i++;
                    }
                    do {
                        sum--;
                    } while (sum > 0);
                    return sum;
                }

                public void intoAnArray(boolean c, int[] into) {
                    into[c ? 0 : 1] = c ? 2 : 3;
                }

                public int caught(boolean c) {
                    try {
                        return pick(c ? 1 : 2, 3);
                    } catch (RuntimeException e) {
                        return c ? -1 : -2;
                    } finally {
                        pick(0, 0);
                    }
                }

                public int labelled(int[][] grid, boolean c) {
                    int found = -1;
                    outer:
                    for (int[] row : grid) {
                        for (int one : row) {
                            if (one == (c ? 1 : 2)) {
                                found = one;
                                break outer;
                            }
                        }
                    }
                    return found;
                }

                public String overText(String s, boolean c) {
                    switch (s) {
                        case "a":
                            return c ? "A" : "b";
                        default:
                            return "z";
                    }
                }

                public Runnable lambdaWithABranch(boolean c) {
                    return () -> pick(c ? 1 : 2, 3);
                }

                public long counted(boolean c) {
                    long total = 0L;
                    for (int i = 0; i < (c ? 2 : 3); i++) {
                        total += c ? 1L : 2L;
                    }
                    return total;
                }
            }
        "#;
        // The lambda becomes a class of its own, and it has to verify too.
        let produced = compile(source, &empty()).expect("must compile");
        for (file, bytes) in &produced {
            let named = file.trim_end_matches(".class");
            match jvm_verifies(named, bytes) {
                None | Some(Verdict::TooOld(_)) => {
                    eprintln!("java: no JVM new enough here to verify the branches");
                    return;
                }
                Some(Verdict::Refused(said)) => {
                    panic!("a real JVM refused {file}:\n{said}")
                }
                Some(Verdict::Verified) => {}
            }
        }

        // And the same classes go the rest of the way: Dalvik is where they
        // actually run, and a shape the JVM accepts is no use if the device
        // refuses it.
        let translated: Vec<_> = produced
            .iter()
            .map(|(file, bytes)| {
                let class = crate::jvm::read(bytes).unwrap_or_else(|_| panic!("read back {file}"));
                crate::dalvik::translate_class(&class)
                    .unwrap_or_else(|refused| panic!("{file} to Dalvik: {refused:?}"))
            })
            .collect();
        let dex = crate::dexwrite::write(&translated, &[]).expect("and reach a dex");
        let mut sink = crate::diag::Sink::new();
        crate::dex::read(&dex, &mut sink).expect("which our own reader reads");
        assert_eq!(sink.entries().len(), 0, "{:?}", sink.entries());
        if let Some(said) = dexdump_disassembly(&dex) {
            assert!(said.contains("Branchy"), "dexdump read it back");
        }

        eprintln!(
            "java: branches inside expressions verify, across {} classes, and reach a dex",
            produced.len()
        );
    }

    #[test]
    fn a_text_block_keeps_what_was_meant_and_drops_what_was_not() {
        // The indentation rule is the whole reason these exist, so it is the
        // thing worth pinning down.
        let cases: &[(&str, &str)] = &[
            ("\"\"\"\n    a\n    b\n    \"\"\"", "a\nb\n"),
            ("\"\"\"\n    a\n      b\n    \"\"\"", "a\n  b\n"),
            ("\"\"\"\n    a\n    b\"\"\"", "a\nb"),
            ("\"\"\"\n      a\n    b\n    \"\"\"", "  a\nb\n"),
            // Trailing whitespace is invisible and therefore cannot have been
            // meant, unless it was written as an escape.
            ("\"\"\"\n    a   \n    \"\"\"", "a\n"),
            ("\"\"\"\n    a\\s\\s\n    \"\"\"", "a  \n"),
            // A backslash at the end of a line joins it to the next.
            ("\"\"\"\n    a\\\n    b\n    \"\"\"", "ab\n"),
            (
                r#""""
    say \"hi\"
    """"#,
                "say \"hi\"\n",
            ),
        ];
        for (written, wanted) in cases {
            let tokens = Lexer::new(written).tokens().expect(written);
            let Token::Str(found) = &tokens[0].token else {
                panic!("{written} did not lex as a string: {:?}", tokens[0].token);
            };
            assert_eq!(found, wanted, "{written}");
        }
    }

    #[test]
    fn what_is_still_refused_is_refused_by_name() {
        let cases: &[(&str, &str)] = &[
            // A `try` with no way out and nothing to close.
            ("public class A { void f() { try { } } }", "EJ114"),
            // Something a `try` closes has to be given a value.
            (
                "public class A { void f() { try (AutoCloseable a) {} } }",
                "EJ118",
            ),
            // A `switch` over something that is neither an integer nor a
            // String has no instruction behind it.
            (
                "public class A { void f(double d) { switch (d) { default: } } }",
                "EJ238",
            ),
            // Two arms answering to one value is a question with two answers.
            (
                "public class A { void f(int i) { switch (i) { case 1: case 1: } } }",
                "EJ240",
            ),
            // The two forms mean different things about falling through, so
            // they are not mixed.
            (
                "public class A { void f(int i) { switch (i) { case 1 -> {} case 2: } } }",
                "EJ112",
            ),
            ("public class A { void f() { break; } }", "EJ231"),
            (
                "public class A { void f() { while (true) { continue nowhere; } } }",
                "EJ231",
            ),
            ("public class A { void f() { throw 1; } }", "EJ235"),
            ("public class A { void f() { var a; } }", "EJ234"),
            (
                "public class A { void f(int i) { for (int one : i) {} } }",
                "EJ236",
            ),
        ];
        for (source, code) in cases {
            let error = compile_one(source, &empty()).expect_err(source);
            assert_eq!(error.code, *code, "{source}");
        }
    }

    #[test]
    fn arrays_and_floating_point_reach_dalvik() {
        let source = r#"
            public class Numbers {
                public int first(int[] values) {
                    return values[0];
                }
                public void put(int[] values, int at, int value) {
                    values[at] = value;
                }
                public int[] make(int size) {
                    return new int[size];
                }
                public int howMany(int[] values) {
                    return values.length;
                }
                public double half(double value) {
                    return value / 2.0;
                }
                public float scale(float value, float by) {
                    return value * by;
                }
                public double widen(int value) {
                    return value;
                }
                public int narrow(double value) {
                    return (int) value;
                }
            }
        "#;
        let (_, bytes) = compile_one(source, &empty()).expect("this must compile");
        let class = crate::jvm::read(&bytes).expect("and read back");
        let translated = crate::dalvik::translate_class(&class)
            .expect("arrays and floating point must translate");

        let named: Vec<&str> = translated
            .direct_methods
            .iter()
            .chain(translated.virtual_methods.iter())
            .map(|one| one.reference.name.as_str())
            .collect();
        for wanted in [
            "first", "put", "make", "howMany", "half", "scale", "widen", "narrow",
        ] {
            assert!(
                named.contains(&wanted),
                "{wanted} is missing from {named:?}"
            );
        }

        let dex = crate::dexwrite::write(&[translated], &[]).expect("and reach a dex");
        let mut sink = crate::diag::Sink::new();
        crate::dex::read(&dex, &mut sink).expect("which our own reader reads");
        assert_eq!(sink.entries().len(), 0, "{:?}", sink.entries());

        eprintln!(
            "dalvik: arrays and floating point, eight methods, into a {} byte dex",
            dex.len()
        );
    }

    #[test]
    fn what_the_translator_cannot_turn_into_dalvik_it_names() {
        // Built by hand, because the Java compiler here no longer emits
        // anything the translator refuses -- which is what the last change was
        // for, and not a reason to stop checking that a refusal happens.
        let class = crate::jvm::Class {
            major_version: CLASS_MAJOR,
            minor_version: 0,
            constants: vec![crate::jvm::Constant::Unusable],
            access_flags: 0x0001,
            name: "Thrower".to_string(),
            superclass: Some("java.lang.Object".to_string()),
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![crate::jvm::Member {
                access_flags: 0x0001,
                name: "boom".to_string(),
                descriptor: "()V".to_string(),
                code: Some(crate::jvm::Code {
                    max_stack: 1,
                    max_locals: 1,
                    // aconst_null, jsr -- and a subroutine is not translated,
                    // because Dalvik has no return address to jump back to and
                    // the type-checking verifier refuses `jsr` anyway.
                    bytes: vec![0x01, 0xa8, 0x00, 0x03],
                    handlers: Vec::new(),
                }),
            }],
            attributes: Vec::new(),
            kotlin: None,
        };

        let refused = crate::dalvik::translate_class(&class)
            .expect_err("an instruction it does not know must be refused");
        assert_eq!(refused.code, "ED900");
        assert!(
            refused.message.contains("jsr"),
            "the refusal has to name it: {}",
            refused.message
        );
        assert!(
            refused.context.iter().any(|line| line.contains("Offset 1")),
            "and say where: {:?}",
            refused.context
        );
        assert!(refused.suggestion.is_some());

        eprintln!("dalvik: what it cannot translate, it names and locates");
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
        let (_, bytes) = compile_one(library, &empty()).unwrap();
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
        let (name, made) =
            compile_one(caller, &classpath).expect("a call it was handed must compile");
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
