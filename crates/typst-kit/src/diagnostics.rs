//! Diagnostic pretty-printing.

#![cfg(feature = "emit-diagnostics")]

use std::collections::HashMap;
use std::io::{self, Write};
use std::ops::Range;

use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::Files;
use codespan_reporting::term;
use ecow::EcoString;
use serde::Serialize;
use termcolor::{Color, ColorSpec, WriteColor};
use typst_library::World;
use typst_library::diag::{FileError, Severity, SourceDiagnostic, Tracepoint};
use typst_syntax::{DiagSpan, DiagSpanKind, FileId, Lines, Source, Spanned};

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

pub use term::termcolor;

/// Extends the [`World`] for diagnostic printing.
pub trait DiagnosticWorld: World {
    /// Formats a file ID for user-facing display.
    ///
    /// In the CLI, this formats as a path relative to the working directory.
    fn name(&self, id: FileId) -> String;
}

/// Which format to use for diagnostics.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiagnosticFormat {
    /// Displays a richly formatted message showing the source code and context.
    #[default]
    Human,
    /// Displays a short single-line diagnostic.
    Short,
    /// Emits diagnostics as JSON on a single line.
    Json,
}

/// Emits diagnostic messages to a writable, colorized output.
pub fn emit<'a>(
    dest: &mut dyn WriteColor,
    world: &dyn DiagnosticWorld,
    diagnostics: impl IntoIterator<Item = &'a SourceDiagnostic>,
    format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let mut files = WorldFiles { world, sources: HashMap::new() };

    let mut config = term::Config { tab_width: 2, ..Default::default() };
    if format == DiagnosticFormat::Short {
        config.display_style = term::DisplayStyle::Short;
    }

    for diagnostic in diagnostics {
        let diag = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .filter(|s| s.span.is_detached())
                .map(|s| format!("hint: {}", s.v))
                .collect(),
        )
        .with_labels(
            diagnostic
                .span
                .id()
                .and_then(|id| {
                    let range = files.range(diagnostic.span)?;
                    Some(Label::primary(id, range))
                })
                .into_iter()
                .chain(diagnostic.hints.iter().filter_map(|hint| {
                    let id = hint.span.id()?;
                    let range = files.range(hint.span)?;
                    Some(Label::secondary(id, range).with_message(&hint.v))
                }))
                .collect(),
        );

        term::emit(dest, &config, &files, &diag)?;

        // Stacktrace-like helper diagnostics.
        if format == DiagnosticFormat::Human {
            let mut traced = false;
            for point in &diagnostic.trace {
                emit_trace(dest, &mut files, point)?;
                traced = true;
            }

            if traced {
                writeln!(dest)?;
            }
        }
    }

    Ok(())
}

/// Emits diagnostic messages as a JSON array to a writable stream.
///
/// Each call writes exactly one compact JSON array containing all diagnostics,
/// followed by a newline. This makes each emission self-contained and gives
/// consumers a stable line-based framing, e.g. in watch mode where each
/// recompilation emits one line. An empty emission is written as `[]`.
///
/// The JSON schema is experimental and may change in minor releases.
pub fn emit_json<'a>(
    dest: &mut dyn Write,
    world: &dyn DiagnosticWorld,
    diagnostics: impl IntoIterator<Item = &'a SourceDiagnostic>,
) -> io::Result<()> {
    let mut files = WorldFiles { world, sources: HashMap::new() };

    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| JsonDiagnostic {
            severity: match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            message: diagnostic.message.clone(),
            span: files.json_span(diagnostic.span),
            hints: diagnostic
                .hints
                .iter()
                .map(|hint| JsonHint {
                    message: hint.v.clone(),
                    span: files.json_span(hint.span),
                })
                .collect(),
            trace: diagnostic
                .trace
                .iter()
                .map(|point| {
                    let (kind, name) = match &point.v {
                        Tracepoint::Call(name) => ("call", name.clone()),
                        Tracepoint::Show(name) => ("show", Some(name.clone())),
                        Tracepoint::Import(name) => ("import", Some(name.clone())),
                        Tracepoint::Include(name) => ("include", Some(name.clone())),
                    };
                    JsonTrace { kind, name, span: files.json_span(point.span) }
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    serde_json::to_writer(&mut *dest, &diagnostics)?;
    dest.write_all(b"\n")?;
    // Flush eagerly so that watch-mode consumers see each emission even
    // though the process stays alive.
    dest.flush()?;
    Ok(())
}

/// Emits an application-level error (independent from a source file) in the
/// same JSON schema as [`emit_json`].
///
/// The error is emitted as a single-element array whose only diagnostic has a
/// detached span, matching the representation of source diagnostics without a
/// location.
pub fn emit_app_error_json(
    dest: &mut dyn Write,
    message: &str,
    hints: &[EcoString],
) -> io::Result<()> {
    let diagnostic = JsonDiagnostic {
        severity: "error",
        message: message.into(),
        span: None,
        hints: hints
            .iter()
            .map(|hint| JsonHint { message: hint.clone(), span: None })
            .collect(),
        trace: Vec::new(),
    };

    serde_json::to_writer(&mut *dest, &[diagnostic])?;
    dest.write_all(b"\n")?;
    dest.flush()?;
    Ok(())
}

/// A diagnostic in the JSON format.
#[derive(Debug, Serialize)]
struct JsonDiagnostic {
    severity: &'static str,
    message: EcoString,
    /// `None` when the diagnostic is not tied to any source location.
    span: Option<JsonSpan>,
    hints: Vec<JsonHint>,
    trace: Vec<JsonTrace>,
}

/// A hint attached to a diagnostic in the JSON format.
#[derive(Debug, Serialize)]
struct JsonHint {
    message: EcoString,
    /// `None` when the hint is not tied to any source location.
    span: Option<JsonSpan>,
}

/// A tracepoint in the JSON format.
#[derive(Debug, Serialize)]
struct JsonTrace {
    kind: &'static str,
    name: Option<EcoString>,
    span: Option<JsonSpan>,
}

/// A source location in the JSON format.
///
/// Byte offsets are zero-based and count UTF-8 bytes. Lines and columns are
/// one-based, matching the human output and rustc's JSON diagnostics, with
/// columns counting Unicode characters. If a file cannot be read, only the
/// fields that can be determined without its content are populated.
#[derive(Debug, Serialize)]
struct JsonSpan {
    file: Option<String>,
    start: Option<usize>,
    end: Option<usize>,
    line: Option<usize>,
    column: Option<usize>,
}

impl JsonSpan {
    /// A span whose file is known, but whose range cannot be determined.
    fn file_only(file: String) -> Self {
        Self {
            file: Some(file),
            start: None,
            end: None,
            line: None,
            column: None,
        }
    }
}

/// Emits a tracepoint.
fn emit_trace(
    dest: &mut dyn WriteColor,
    files: &mut WorldFiles,
    point: &Spanned<Tracepoint>,
) -> Result<(), codespan_reporting::files::Error> {
    let Some(id) = point.span.id() else { return Ok(()) };
    let Some(range) = files.range(point.span) else { return Ok(()) };
    let lines = files.lines(id)?;

    let name = files.name(id)?;
    let line_index = files.line_index(id, range.start)?;
    let line = files.line_number(id, line_index)?;
    let column = files.column_number(id, line_index, range.start)?;
    let text = &lines.text()[range];

    // Displays what kind of tracepoint we have and where.
    write!(dest, "  {} at ", point.v)?;
    dest.set_color(ColorSpec::new().set_underline(true))?;
    write!(dest, "{name}:{line}:{column}")?;
    dest.reset()?;
    writeln!(dest)?;

    // Displays the context in the source in a single line.
    let mut lines = text.lines();
    write!(dest, "    ")?;
    dest.set_color(ColorSpec::new().set_fg(Some(Color::Ansi256(248))))?;
    if let Some(first) = lines.next() {
        write!(dest, "{first}")?;
    }
    if let Some(last) = lines.next_back()
        && let Some(last_char) = last.chars().next_back()
        && !last_char.is_whitespace()
    {
        // If the traced source text is multi-line, try to display it
        // with inner ellipses followed by the last character.
        write!(dest, "…{last_char}")?;
    }
    dest.reset()?;
    writeln!(dest)?;

    Ok(())
}

/// Provides file contents and metadata to `codespan-reporting`.
struct WorldFiles<'a> {
    world: &'a dyn DiagnosticWorld,
    sources: HashMap<FileId, Source>,
}

impl WorldFiles<'_> {
    /// Determine the byte range of a span, also remembering the source file
    /// for future line / column lookups.
    fn range(&mut self, span: impl Into<DiagSpan>) -> Option<Range<usize>> {
        match span.into().get() {
            DiagSpanKind::Detached => None,
            DiagSpanKind::Number { id, num, sub_range } => {
                let source = self.world.source(id).ok()?;
                let range = source.range(num, sub_range);
                self.sources.entry(id).or_insert(source);
                range
            }
            DiagSpanKind::Range { id: _, range } => Some(range),
        }
    }

    /// Resolve a diagnostic span into a JSON-serializable location.
    ///
    /// Returns `None` for detached spans. The result is partial if the file
    /// cannot be read.
    fn json_span(&mut self, span: impl Into<DiagSpan>) -> Option<JsonSpan> {
        // Determine the file and byte range of the span.
        let (id, range) = match span.into().get() {
            DiagSpanKind::Detached => return None,
            DiagSpanKind::Number { id, num, sub_range } => {
                let Some(source) = self.world.source(id).ok() else {
                    return Some(JsonSpan::file_only(self.world.name(id)));
                };
                let Some(range) = source.range(num, sub_range) else {
                    return Some(JsonSpan::file_only(self.world.name(id)));
                };
                self.sources.entry(id).or_insert(source);
                (id, range)
            }
            DiagSpanKind::Range { id, range } => (id, range),
        };

        let file = self.world.name(id);
        let lines = self.lines(id).ok();
        let location = lines
            .as_ref()
            .and_then(|lines| lines.byte_to_line_column(range.start))
            // Lines and columns are one-based, as in the human output and in
            // rustc's JSON output.
            .map(|(line, column)| (line + 1, column + 1));

        Some(JsonSpan {
            file: Some(file),
            start: Some(range.start),
            end: Some(range.end),
            line: location.map(|(line, _)| line),
            column: location.map(|(_, column)| column),
        })
    }

    /// Lookup line metadata for a file by id. If a source file was remembered,
    /// it will be used. Otherwise, we load as a file as compute line metadata.
    fn lines(&self, id: FileId) -> CodespanResult<Lines<String>> {
        match self.sources.get(&id) {
            Some(source) => Ok(source.lines().clone()),
            None => self
                .world
                .file(id)
                .and_then(|file| file.lines().map_err(Into::into))
                .map_err(|err| match err {
                    FileError::NotFound(_) => CodespanError::FileMissing,
                    other => CodespanError::Io(io::Error::other(other)),
                }),
        }
    }
}

impl<'a> Files<'a> for WorldFiles<'_> {
    type FileId = FileId;
    type Name = String;
    type Source = Lines<String>;

    fn name(&'a self, id: FileId) -> CodespanResult<Self::Name> {
        Ok(self.world.name(id))
    }

    fn source(&'a self, id: FileId) -> CodespanResult<Self::Source> {
        self.lines(id)
    }

    fn line_index(&'a self, id: FileId, given: usize) -> CodespanResult<usize> {
        let lines = self.lines(id)?;
        lines
            .byte_to_line(given)
            .ok_or_else(|| CodespanError::IndexTooLarge { given, max: lines.len_bytes() })
    }

    fn line_range(
        &'a self,
        id: FileId,
        given: usize,
    ) -> CodespanResult<std::ops::Range<usize>> {
        let lines = self.lines(id)?;
        lines
            .line_to_range(given)
            .ok_or_else(|| CodespanError::LineTooLarge { given, max: lines.len_lines() })
    }

    fn column_number(
        &'a self,
        id: FileId,
        _: usize,
        given: usize,
    ) -> CodespanResult<usize> {
        let lines = self.lines(id)?;
        lines.byte_to_column(given).ok_or_else(|| {
            let max = lines.len_bytes();
            if given <= max {
                CodespanError::InvalidCharBoundary { given }
            } else {
                CodespanError::IndexTooLarge { given, max }
            }
        })
    }
}
