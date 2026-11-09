use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;
use std::{fmt, io};

use chrono::{DateTime, Utc};
use inpt::{inpt, Inpt};

use crate::{Error, ResultExt as _, Source, TypedSection};

pub type Arena = typed_arena::Arena<String>;

#[derive(Clone)]
pub struct Config<'a> {
    pub arena: &'a Arena,
    pub prefix: Vec<Line<'a>>,
    pub sections: Vec<Section<'a>>,
    pub modified: Option<DateTime<Utc>>,
}

impl<'a> Config<'a> {
    pub fn new(arena: &'a Arena) -> Self {
        Config {
            arena,
            prefix: Vec::new(),
            sections: Vec::new(),
            modified: None,
        }
    }

    pub fn append<T: TypedSection<'a>>(
        &mut self,
        section: &T,
        name: Option<&str>,
    ) -> Result<(), Error> {
        let name = name.map(|n| self.arena.alloc(n.to_string()).as_str());
        let section = T::append(section, self.arena, name)?;
        self.sections.push(section);
        Ok(())
    }

    /// Panics past the end, like [`Vec::insert`].
    pub fn insert<T: TypedSection<'a>>(
        &mut self,
        index: usize,
        section: &T,
        name: Option<&str>,
    ) -> Result<(), Error> {
        let name = name.map(|n| self.arena.alloc(n.to_string()).as_str());
        let section = T::append(section, self.arena, name)?;
        self.sections.insert(index, section);
        Ok(())
    }

    pub fn each<T: TypedSection<'a>, E>(
        &self,
        mut each: impl FnMut(Option<&str>, T),
    ) -> Result<(), Error> {
        for s in &self.sections {
            if T::is_type(&s.ty()) {
                each(s.name().as_deref(), T::read(s)?);
            }
        }
        Ok(())
    }

    pub fn try_each<T: TypedSection<'a>, E: From<Error>>(
        &self,
        mut each: impl FnMut(Option<&str>, T) -> Result<(), E>,
    ) -> Result<(), E> {
        for s in &self.sections {
            if T::is_type(&s.ty()) {
                each(s.name().as_deref(), T::read(s)?)?;
            }
        }
        Ok(())
    }

    pub async fn parse(arena: &'a Arena, path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let mut modified = tokio::fs::metadata(path)
            .await
            .and_then(|m| m.modified())
            .map(DateTime::<Utc>::from)
            .ok();
        let text = match tokio::fs::read_to_string(path).await {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                modified = None;
                String::new()
            }
            Err(cause) => {
                return Err(Error::Io {
                    cause,
                    src: Source::Path(path.to_path_buf()),
                })
            }
        };
        let text = arena.alloc(text);
        let mut config = Self::parse_str(arena, text).with_path(path)?;
        config.modified = modified;
        Ok(config)
    }

    pub fn parse_io(arena: &'a Arena, mut reader: impl io::Read) -> Result<Self, Error> {
        let mut text = String::new();
        reader
            .read_to_string(&mut text)
            .map_err(|cause| Error::Io {
                cause,
                src: Source::Unknown,
            })?;
        let text = arena.alloc(text);
        Self::parse_str(arena, text)
    }

    pub fn parse_str(arena: &'a Arena, text: &'a str) -> Result<Self, Error> {
        // now break the file into sections
        let mut sections = vec![Section {
            arena,
            lines: Vec::new(),
        }];
        let mut lexer = Lexer::new(text, 0);
        while let Some(statement) = lexer.statement() {
            let line = statement?;
            if matches!(line, Line::Section { .. }) {
                let has_section = sections.len() > 1;
                let last_section = sections.last_mut().unwrap();
                let mut new_section = Section {
                    arena,
                    lines: Vec::new(),
                };
                while matches!(
                    last_section.lines.last(),
                    Some(Line::Comment { indent: false, .. })
                ) || (has_section && matches!(last_section.lines.last(), Some(Line::Empty)))
                {
                    new_section.lines.push(last_section.lines.pop().unwrap());
                }
                new_section.lines.reverse();
                sections.push(new_section);
            }
            sections.last_mut().unwrap().lines.push(line);
        }

        // separate out the prefix
        let prefix = if sections.is_empty() {
            Vec::new()
        } else {
            sections.remove(0).lines
        };
        for (n, prefix_line) in prefix.iter().enumerate() {
            if matches!(prefix_line, Line::Option { .. } | Line::List { .. }) {
                return Err(Error::ExpectedSection {
                    src: Source::UnknownLine(n),
                });
            }
        }

        Ok(Config {
            arena,
            prefix,
            sections,
            modified: None,
        })
    }

    pub async fn dump(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let mut w = super::LockedConfig::open(path.as_ref().to_path_buf()).await?;
        if let Some(expected) = self.modified {
            w.check_modified(expected).await?;
        }
        w.dump(self).await
    }

    pub fn dump_io(&self, mut writer: impl io::Write) -> Result<(), Error> {
        let mut empty = true;
        for line in &self.prefix {
            empty = false;
            write!(&mut writer, "{}", line).map_err(|cause| Error::Io {
                cause,
                src: Source::Unknown,
            })?;
        }
        for section in &self.sections {
            for line in &section.lines {
                if empty & matches!(line, Line::Empty) {
                    continue;
                }
                empty = false;
                write!(&mut writer, "{}", line).map_err(|cause| Error::Io {
                    cause,
                    src: Source::Unknown,
                })?;
            }
        }
        Ok(())
    }

    pub fn dump_str(&self) -> String {
        let mut empty = true;
        let mut string = String::new();
        use std::fmt::Write;
        for line in &self.prefix {
            empty = false;
            let _ = write!(&mut string, "{}", line);
        }
        for section in &self.sections {
            for line in &section.lines {
                if empty & matches!(line, Line::Empty) {
                    continue;
                }
                empty = false;
                let _ = write!(&mut string, "{}", line);
            }
        }
        string
    }
}

#[derive(Clone)]
pub struct Section<'a> {
    pub arena: &'a Arena,
    pub lines: Vec<Line<'a>>,
}

impl<'a> Section<'a> {
    pub fn new(arena: &'a Arena, ty: &'a str, name: Option<&'a str>) -> Self {
        Self {
            lines: vec![
                Line::Empty,
                Line::Section {
                    ty: Token::from_str(ty, arena),
                    name: name.map(|name| Token::from_str(name, arena)),
                    comment: LineComment::None,
                },
            ],
            arena,
        }
    }

    pub fn ty(&self) -> Cow<'_, str> {
        for line in &self.lines {
            if let Line::Section { ty, .. } = line {
                return ty.as_str();
            }
        }
        panic!("section missing header")
    }

    pub fn name(&self) -> Option<Cow<'_, str>> {
        for line in &self.lines {
            if let Line::Section { name, .. } = line {
                return name.as_ref().map(Token::as_str);
            }
        }
        panic!("section missing header")
    }

    pub fn set_name(&mut self, name: &str) {
        let arena = self.arena;
        for line in &mut self.lines {
            if let Line::Section { name: header, .. } = line {
                *header = Some(Token::from_str(
                    arena.alloc(name.to_string()).as_str(),
                    arena,
                ));
                return;
            }
        }
        panic!("section missing header")
    }

    pub fn get_typed<T: TypedSection<'a>>(&self) -> Result<Option<T>, Error> {
        if T::is_type(&self.ty()) {
            Ok(Some(T::read(self)?))
        } else {
            Ok(None)
        }
    }

    pub fn get<T: TypedSection<'a>>(&self) -> Result<T, Error> {
        T::read(self)
    }

    pub fn set<T: TypedSection<'a>>(&mut self, section: &T) -> Result<(), Error> {
        T::write(section, self)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LineComment<'a> {
    None,
    Commented(&'a str),
}

impl fmt::Display for LineComment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LineComment::None => Ok(()),
            LineComment::Commented(text) => write!(f, " #{}", text),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Line<'a> {
    Empty,
    Comment {
        indent: bool,
        text: &'a str,
    },
    /// libuci reads past it within a single config file.
    Package {
        name: Token<'a>,
        comment: LineComment<'a>,
    },
    Section {
        ty: Token<'a>,
        name: Option<Token<'a>>,
        comment: LineComment<'a>,
    },
    Option {
        option: Token<'a>,
        value: Token<'a>,
        comment: LineComment<'a>,
    },
    List {
        list: Token<'a>,
        item: Token<'a>,
        comment: LineComment<'a>,
    },
    Skip,
}

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Line::Empty => writeln!(f),
            Line::Comment {
                indent: false,
                text,
            } => writeln!(f, "#{}", text),
            Line::Comment { indent: true, text } => writeln!(f, "\t#{}", text),
            Line::Package {
                name,
                comment: tail,
            } => writeln!(f, "package {}{}", name, tail),
            Line::Section {
                ty,
                name: None,
                comment: tail,
            } => writeln!(f, "config {}{}", ty, tail),
            Line::Section {
                ty,
                name: Some(name),
                comment: tail,
            } => writeln!(f, "config {} {}{}", ty, name, tail),
            Line::Option {
                option,
                value,
                comment: tail,
            } => writeln!(f, "\toption {} {}{}", option, value, tail),
            Line::List {
                list,
                item,
                comment: tail,
            } => writeln!(f, "\tlist {} {}{}", list, item, tail),
            Line::Skip => Ok(()),
        }
    }
}

impl<'a> Line<'a> {
    /// Parses the first statement of `line`; anything after it is ignored.
    pub fn parse(line: &'a str, line_number: usize) -> Result<Self, Error> {
        Lexer::new(line, line_number)
            .statement()
            .unwrap_or(Ok(Line::Empty))
    }

    pub fn is_in_section(&self) -> bool {
        matches!(
            self,
            Line::Comment { indent: true, .. } | Line::Option { .. } | Line::List { .. }
        )
    }

    pub fn option_from_display(
        option: &'a str,
        value: impl fmt::Display,
        arena: &'a Arena,
    ) -> Self {
        let option = Token::from_str(option, arena);
        Line::Option {
            option,
            value: Token::from_display(value, arena),
            comment: LineComment::None,
        }
    }

    pub fn list_from_display<'i>(
        list: &'a str,
        items: &'i [impl fmt::Display],
        arena: &'a Arena,
    ) -> impl Iterator<Item = Self> + 'i
    where
        'a: 'i,
    {
        let list = Token::from_str(list, arena);
        items.iter().map(move |item| Line::List {
            list,
            item: Token::from_display(item, arena),
            comment: LineComment::None,
        })
    }

    pub fn option_from_bool(option: &'a str, value: bool, arena: &'a Arena) -> Self {
        let option = Token::from_str(option, arena);
        Line::Option {
            option,
            value: Token::from_bool(value),
            comment: LineComment::None,
        }
    }

    pub fn list_from_bool<'i>(
        list: &'a str,
        items: &'i [bool],
        arena: &'a Arena,
    ) -> impl Iterator<Item = Self> + 'i
    where
        'a: 'i,
    {
        let list = Token::from_str(list, arena);
        items.iter().map(move |item| Line::List {
            list,
            item: Token::from_bool(*item),
            comment: LineComment::None,
        })
    }

    pub fn section_from(ty: &'a str, name: Option<&'a str>, arena: &'a Arena) -> Self {
        Line::Section {
            ty: Token::from_str(ty, arena),
            name: name.map(|n| Token::from_str(n, arena)),
            comment: LineComment::None,
        }
    }
}

/// Splits UCI text into statements the way libuci's `uci_parse_line` does.
struct Lexer<'a> {
    text: &'a str,
    pos: usize,
    line: usize,
    line_start: bool,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str, line: usize) -> Self {
        Lexer {
            text,
            pos: 0,
            line,
            line_start: true,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn skip_blanks(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.peek(), Some(b) if b != b'\n' && b.is_ascii_whitespace()) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn end_line(&mut self) {
        if self.peek() == Some(b'\n') {
            self.pos += 1;
            self.line += 1;
        }
        self.line_start = true;
    }

    /// Excludes the line break and trailing whitespace.
    fn rest_of_line(&mut self) -> &'a str {
        let text = self.text;
        let rest = &text[self.pos..];
        let end = rest.find('\n').unwrap_or(rest.len());
        self.pos += end;
        self.end_line();
        rest[..end].trim_end()
    }

    fn statement(&mut self) -> Option<Result<Line<'a>, Error>> {
        loop {
            if self.pos >= self.text.len() {
                return None;
            }
            let at_start = std::mem::replace(&mut self.line_start, false);
            let indent = self.skip_blanks() || !at_start;
            match self.peek() {
                None | Some(b'\n') => {
                    self.end_line();
                    if at_start {
                        return Some(Ok(Line::Empty));
                    }
                }
                Some(b';') => self.pos += 1,
                Some(b'#') => {
                    self.pos += 1;
                    let text = self.rest_of_line();
                    return Some(Ok(Line::Comment { indent, text }));
                }
                Some(_) => return Some(self.command()),
            }
        }
    }

    fn command(&mut self) -> Result<Line<'a>, Error> {
        let line = self.line;
        let start = self.pos;
        while matches!(self.peek(), Some(b) if b != b';' && !b.is_ascii_whitespace()) {
            self.pos += 1;
        }
        let src = || Source::UnknownLine(line);
        let text = self.text;
        match &text[start..self.pos] {
            "config" | "c" => {
                let bad = |desc: String| Error::BadSection { src: src(), desc };
                let ty = self.token().map_err(bad)?;
                let ty = ty.ok_or_else(|| bad("config without type".into()))?;
                let name = self.token().map_err(bad)?;
                let comment = self.end_statement().map_err(bad)?;
                Ok(Line::Section { ty, name, comment })
            }
            "option" | "o" => {
                let bad = |desc: String| Error::BadOption { src: src(), desc };
                let (option, value, comment) = self.option_args().map_err(bad)?;
                Ok(Line::Option {
                    option,
                    value,
                    comment,
                })
            }
            "list" | "l" => {
                let bad = |desc: String| Error::BadList { src: src(), desc };
                let (list, item, comment) = self.option_args().map_err(bad)?;
                Ok(Line::List {
                    list,
                    item,
                    comment,
                })
            }
            "package" | "p" => {
                let bad = |desc: String| Error::BadPackage { src: src(), desc };
                let name = self.token().map_err(bad)?;
                let name = name.ok_or_else(|| bad("package without name".into()))?;
                let comment = self.end_statement().map_err(bad)?;
                Ok(Line::Package { name, comment })
            }
            found => Err(Error::UnknownKeyword {
                src: src(),
                found: found.into(),
            }),
        }
    }

    /// A missing value is the empty string.
    fn option_args(&mut self) -> Result<(Token<'a>, Token<'a>, LineComment<'a>), String> {
        let name = self.token()?.ok_or("missing option name")?;
        let value = self.token()?.unwrap_or(Token { raw: "" });
        let comment = self.end_statement()?;
        Ok((name, value, comment))
    }

    fn end_statement(&mut self) -> Result<LineComment<'a>, String> {
        self.skip_blanks();
        match self.peek() {
            Some(b'#') => {
                self.pos += 1;
                Ok(LineComment::Commented(self.rest_of_line()))
            }
            Some(b';') => {
                self.pos += 1;
                Ok(LineComment::None)
            }
            None | Some(b'\n') => {
                self.end_line();
                Ok(LineComment::None)
            }
            Some(_) => Err("too many arguments".into()),
        }
    }

    /// One argument, as libuci's `parse_str` delimits it.
    fn token(&mut self) -> Result<Option<Token<'a>>, String> {
        self.skip_blanks();
        let start = self.pos;
        while let Some(b) = self.peek() {
            match b {
                b'\'' | b'"' => self.quoted(b)?,
                b'\\' => self.backslash(),
                b'#' | b';' => break,
                b if b.is_ascii_whitespace() => break,
                _ => self.pos += 1,
            }
        }
        let text = self.text;
        Ok((self.pos > start).then(|| Token {
            raw: &text[start..self.pos],
        }))
    }

    /// Backslash escapes apply inside double quotes only.
    fn quoted(&mut self, quote: u8) -> Result<(), String> {
        self.pos += 1;
        loop {
            match self.peek() {
                None => return Err(format!("unterminated {}", quote as char)),
                Some(b) if b == quote => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(b'\\') if quote == b'"' => self.backslash(),
                Some(b'\n') => {
                    self.pos += 1;
                    self.line += 1;
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn backslash(&mut self) {
        self.pos += 1;
        let rest = &self.text.as_bytes()[self.pos..];
        if rest.starts_with(b"\n") {
            self.pos += 1;
            self.line += 1;
        } else if rest.starts_with(b"\r\n") {
            self.pos += 2;
            self.line += 1;
        } else if !rest.is_empty() {
            self.pos += 1;
        }
    }
}

/// libuci's `parse_str` decoding: adjacent pieces join, single-quoted text is literal, and
/// elsewhere a backslash keeps the next character or joins the next line.
fn decode(raw: &str) -> Cow<'_, str> {
    if !raw.contains(['\'', '"', '\\']) {
        return Cow::Borrowed(raw);
    }
    if let Some(inner) = raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        if !inner.contains('\'') {
            return Cow::Borrowed(inner);
        }
    }
    fn escaped(chars: &mut std::str::Chars, out: &mut String) {
        if let Some(rest) = chars.as_str().strip_prefix("\r\n") {
            *chars = rest.chars();
            return;
        }
        match chars.next() {
            Some('\n') | None => {}
            Some(c) => out.push(c),
        }
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    out.push(c);
                }
            }
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => escaped(&mut chars, &mut out),
                        c => out.push(c),
                    }
                }
            }
            '\\' => escaped(&mut chars, &mut out),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// The quoting libuci writes: single quotes, with each `'` as `'\''`.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    out.push_str(&s.replace('\'', r"'\''"));
    out.push('\'');
    out
}

/// One UCI argument exactly as written, quotes and escapes included.
#[derive(Clone, Copy, Debug)]
pub struct Token<'a> {
    raw: &'a str,
}

impl<'a> Token<'a> {
    pub fn unquoted_string(self) -> String {
        self.as_str().into_owned()
    }

    pub fn as_str(&self) -> Cow<'a, str> {
        decode(self.raw)
    }

    pub fn as_arena_str(self, arena: &'a Arena) -> &'a str {
        match self.as_str() {
            Cow::Borrowed(x) => x,
            Cow::Owned(x) => arena.alloc(x),
        }
    }

    pub fn from_display(s: impl fmt::Display, arena: &'a Arena) -> Self {
        Self::from_string(s.to_string(), arena)
    }

    pub fn from_string(s: String, arena: &'a Arena) -> Self {
        Token {
            raw: arena.alloc(quote(&s)),
        }
    }

    /// A word of letters, digits, `_`, `-` or `.` stays unquoted.
    pub fn from_str(s: &'a str, arena: &'a Arena) -> Self {
        let word = !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'));
        if word {
            Token { raw: s }
        } else {
            Token {
                raw: arena.alloc(quote(s)),
            }
        }
    }

    pub fn from_bool(s: bool) -> Self {
        Token {
            raw: if s { "'1'" } else { "'0'" },
        }
    }

    pub fn parse_fromstr<T: FromStr>(self, line_number: usize) -> Result<T, Error>
    where
        T::Err: std::error::Error + Sync + Send + 'static,
    {
        match self.as_str().parse::<T>() {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::ValueMsg {
                src: Source::UnknownLine(line_number),
                desc: e.to_string(),
            }),
        }
    }

    pub fn parse_inpt<T: Inpt<'a>>(self, line_number: usize, arena: &'a Arena) -> Result<T, Error> {
        match inpt(self.as_arena_str(arena)) {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::ValueMsg {
                src: Source::UnknownLine(line_number),
                desc: e.to_string(),
            }),
        }
    }

    pub fn parse_bool(self, line_number: usize) -> Result<bool, Error> {
        match self.as_str().trim() {
            "0" | "no" | "off" | "false" | "disabled" => Ok(false),
            "1" | "yes" | "on" | "true" | "enabled" => Ok(true),
            other => Err(Error::ValueBoolean {
                src: Source::UnknownLine(line_number),
                found: other.to_string(),
            }),
        }
    }
}

impl PartialEq<str> for Token<'_> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

/// An escape at end of input would take the newline written after it.
fn dangling_escape(raw: &str) -> bool {
    let body = raw.strip_suffix('\r').unwrap_or(raw);
    body.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.raw.is_empty() {
            f.write_str("''")
        } else if dangling_escape(self.raw) {
            f.write_str(&quote(&decode(self.raw)))
        } else {
            f.write_str(self.raw)
        }
    }
}
