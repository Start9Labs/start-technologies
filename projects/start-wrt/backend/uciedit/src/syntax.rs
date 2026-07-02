use crate::{Error, ResultExt as _, Source, TypedSection};
use chrono::{DateTime, Utc};
use inpt::split::{unescape, Quoted, SingleQuoted, Spaced};
use inpt::{inpt, inpt_step, Inpt, InptStep};
use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;
use std::{fmt, io};

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
        for (n, line) in text.lines().enumerate() {
            let line = Line::parse(line, n)?;
            if matches!(line, Line::Section { .. }) {
                // start a new section's line Vec
                // pop anything like a header comment from the end of the previous section
                let last_section = sections.last_mut().unwrap();
                let mut new_section = Section {
                    arena,
                    lines: Vec::new(),
                };
                while matches!(last_section.lines.last(), Some(Line::Comment { .. })) {
                    new_section.lines.push(last_section.lines.pop().unwrap());
                }
                new_section.lines.reverse(); // we pushed comment lines in reverse order
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

#[derive(Debug, Inpt, Clone, Copy)]
pub enum LineComment<'a> {
    #[inpt(regex = r"\s*")]
    None,
    #[inpt(regex = r"\s*#(.*)")]
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
    pub fn parse(line: &'a str, line_number: usize) -> Result<Self, Error> {
        #[derive(Inpt, Clone, Copy)]
        enum ConfigLine<'a> {
            Named(Token<'a>, Token<'a>, LineComment<'a>),
            Unnamed(Token<'a>, LineComment<'a>),
        }

        let rest = line.trim();
        if rest.is_empty() {
            return Ok(Line::Empty);
        }
        if let Some(rest) = rest.strip_prefix('#') {
            return Ok(Line::Comment {
                indent: line.starts_with(char::is_whitespace),
                text: rest,
            });
        }
        let InptStep {
            data: Ok(keyword),
            rest,
        } = inpt_step::<Token>(rest)
        else {
            unreachable!()
        };
        Ok(match &*keyword.as_str() {
            "config" => {
                match inpt(rest).map_err(|err| Error::BadSection {
                    src: Source::UnknownLine(line_number),
                    desc: err.to_string(),
                })? {
                    ConfigLine::Named(ty, name, comment) => Line::Section {
                        ty,
                        name: Some(name),
                        comment,
                    },
                    ConfigLine::Unnamed(ty, comment) => Line::Section {
                        ty,
                        name: None,
                        comment,
                    },
                }
            }
            "option" => {
                let (option, value, comment): (Token, Token, LineComment) =
                    inpt(rest).map_err(|err| Error::BadOption {
                        src: Source::UnknownLine(line_number),
                        desc: err.to_string(),
                    })?;
                Line::Option {
                    option,
                    value,
                    comment,
                }
            }
            "list" => {
                let (list, item, comment): (Token, Token, LineComment) =
                    inpt(rest).map_err(|err| Error::BadList {
                        src: Source::UnknownLine(line_number),
                        desc: err.to_string(),
                    })?;
                Line::List {
                    list,
                    item,
                    comment,
                }
            }
            kw => {
                return Err(Error::UnknownKeyword {
                    src: Source::UnknownLine(line_number),
                    found: kw.into(),
                })
            }
        })
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

#[derive(Inpt, Clone, Copy, Debug)]
pub enum Token<'a> {
    Q(Quoted<&'a str>),
    Sq(SingleQuoted<&'a str>),
    W(Spaced<&'a str>),
}

impl<'a> Token<'a> {
    pub fn unquoted_string(self) -> String {
        self.as_str().into_owned()
    }

    pub fn as_str(&self) -> Cow<'a, str> {
        match self {
            Token::Q(x) => unescape(x.inner),
            Token::Sq(x) => unescape(x.inner),
            Token::W(x) => Cow::Borrowed(x.inner),
        }
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
        if s.contains('\'') {
            // Value contains single quote — must use double-quoting with escapes
            let q = arena.alloc(format!("{:?}", s));
            Token::Q(Quoted {
                inner: &q[1..q.len() - 1],
            })
        } else {
            let s = arena.alloc(s);
            Token::Sq(SingleQuoted { inner: s })
        }
    }

    pub fn from_str(s: &'a str, arena: &'a Arena) -> Self {
        if s.contains(|c: char| !(c.is_alphanumeric() || ['_', '-', '.'].contains(&c))) {
            let q = arena.alloc(format!("{:?}", s));
            Token::Q(Quoted {
                inner: &q[1..q.len() - 1],
            })
        } else {
            Token::W(Spaced { inner: s })
        }
    }

    pub fn from_bool(s: bool) -> Self {
        match s {
            false => Token::Sq(SingleQuoted { inner: "0" }),
            true => Token::Sq(SingleQuoted { inner: "1" }),
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

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Q(quoted) => write!(f, "\"{}\"", quoted.inner),
            Token::Sq(single_quoted) => write!(f, "'{}'", single_quoted.inner),
            Token::W(word) => write!(f, "{}", word.inner),
        }
    }
}
