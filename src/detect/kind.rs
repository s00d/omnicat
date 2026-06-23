use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlerKind {
    Markdown,
    Image,
    Data,
    Pdf,
    Archive,
    Code,
    Spreadsheet,
    Document,
    Presentation,
    LegacyOffice,
    Directory,
    Ebook,
    Media,
    Font,
    Database,
    Email,
    Notebook,
    Plist,
    Fallback,
}

impl HandlerKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Image => "image",
            Self::Data => "data",
            Self::Pdf => "pdf",
            Self::Archive => "archive",
            Self::Code => "code",
            Self::Spreadsheet => "spreadsheet",
            Self::Document => "document",
            Self::Presentation => "presentation",
            Self::LegacyOffice => "legacy_office",
            Self::Directory => "directory",
            Self::Ebook => "ebook",
            Self::Media => "media",
            Self::Font => "font",
            Self::Database => "database",
            Self::Email => "email",
            Self::Notebook => "notebook",
            Self::Plist => "plist",
            Self::Fallback => "fallback",
        }
    }

    pub fn renderer_name(self) -> &'static str {
        match self {
            Self::Markdown => "driver:markdown",
            Self::Image => "driver:image",
            Self::Data => "driver:data",
            Self::Pdf => "driver:pdf",
            Self::Archive => "driver:archive",
            Self::Code => "driver:code",
            Self::Spreadsheet => "driver:spreadsheet",
            Self::Document => "driver:document",
            Self::Presentation => "driver:presentation",
            Self::LegacyOffice => "driver:legacy_office",
            Self::Directory => "driver:directory",
            Self::Ebook => "driver:ebook",
            Self::Media => "driver:media",
            Self::Font => "driver:font",
            Self::Database => "driver:database",
            Self::Email => "driver:email",
            Self::Notebook => "driver:notebook",
            Self::Plist => "driver:plist",
            Self::Fallback => "driver:fallback",
        }
    }

    pub fn all() -> &'static [HandlerKind] {
        &[
            HandlerKind::Markdown,
            HandlerKind::Image,
            HandlerKind::Data,
            HandlerKind::Pdf,
            HandlerKind::Archive,
            HandlerKind::Code,
            HandlerKind::Spreadsheet,
            HandlerKind::Document,
            HandlerKind::Presentation,
            HandlerKind::LegacyOffice,
            HandlerKind::Directory,
            HandlerKind::Ebook,
            HandlerKind::Media,
            HandlerKind::Font,
            HandlerKind::Database,
            HandlerKind::Email,
            HandlerKind::Notebook,
            HandlerKind::Plist,
            HandlerKind::Fallback,
        ]
    }

    pub fn from_name(name: &str) -> Option<HandlerKind> {
        Self::all().iter().copied().find(|k| k.name() == name)
    }
}

impl fmt::Display for HandlerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
