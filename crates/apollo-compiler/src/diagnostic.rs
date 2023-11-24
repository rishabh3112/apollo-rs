//! Pretty-printable diagnostic reports for custom errors that reference GraphQL documents.
use crate::validation::FileId;
use crate::validation::NodeLocation;
use crate::SourceFile;
use crate::SourceMap;
use ariadne::ColorGenerator;
use ariadne::ReportKind;
use std::fmt;
use std::io;
use std::ops::Range;
use std::sync::Arc;
use std::sync::OnceLock;

type MappedSpan = (FileId, Range<usize>);

/// Translate a byte-offset location into a char-offset location for use with ariadne.
fn map_span(sources: &SourceMap, location: NodeLocation) -> Option<MappedSpan> {
    let source = sources.get(&location.file_id)?;
    let mapped_source = source.mapped_source();
    let start = mapped_source.map_index(location.offset());
    let end = mapped_source.map_index(location.end_offset());
    Some((location.file_id, start..end))
}

/// Builder for [`DiagnosticReport`]s.
pub struct ReportBuilder {
    sources: SourceMap,
    colors: ColorGenerator,
    report: ariadne::ReportBuilder<'static, MappedSpan>,
}

impl ReportBuilder {
    fn new(sources: SourceMap, location: Option<NodeLocation>) -> Self {
        let (file_id, range) = location
            .and_then(|location| map_span(&sources, location))
            .unwrap_or((FileId::NONE, 0..0));
        Self {
            sources,
            colors: ColorGenerator::new(),
            report: ariadne::Report::build(ReportKind::Error, file_id, range.start),
        }
    }

    /// Set the main message for the report.
    pub fn with_message(&mut self, message: impl ToString) {
        self.report.set_message(message);
    }

    /// Set the help message for the report, usually a suggestion on how to fix the error.
    pub fn with_help(&mut self, help: impl ToString) {
        self.report.set_help(help);
    }

    /// Set a note for the report, providing additional information that isn't related to a
    /// source location (when a label should be used).
    pub fn with_note(&mut self, note: impl ToString) {
        self.report.set_note(note);
    }

    /// Add a label at a given location. If the location is `None`, the message is discarded.
    pub fn with_label_opt(&mut self, location: Option<NodeLocation>, message: impl ToString) {
        if let Some(mapped_span) = location.and_then(|location| map_span(&self.sources, location)) {
            self.report.add_label(
                ariadne::Label::new(mapped_span)
                    .with_message(message)
                    .with_color(self.colors.next()),
            );
        }
    }

    /// Enable or disable colors in the output.
    pub fn with_color(self, color: bool) -> Self {
        let Self {
            sources,
            colors,
            report,
        } = self;
        let report = report.with_config(ariadne::Config::default().with_color(color));
        Self {
            sources,
            colors,
            report,
        }
    }

    /// Return the report.
    pub fn finish(self) -> DiagnosticReport {
        DiagnosticReport {
            sources: self.sources,
            report: self.report.finish(),
        }
    }
}

/// A diagnostic report that can be printed to a CLI with pretty colours and labeled lines of
/// GraphQL source code.
pub struct DiagnosticReport {
    sources: SourceMap,
    report: ariadne::Report<'static, MappedSpan>,
}

impl DiagnosticReport {
    /// Returns a builder for creating diagnostic reports.
    ///
    /// Provide GraphQL source files and the main location for the diagnostic.
    pub fn builder(sources: SourceMap, location: Option<NodeLocation>) -> ReportBuilder {
        ReportBuilder::new(sources, location)
    }
}

impl fmt::Display for DiagnosticReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Adaptor<'a, 'b>(&'a mut fmt::Formatter<'b>);

        impl io::Write for Adaptor<'_, '_> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let s = std::str::from_utf8(buf).map_err(|_| io::ErrorKind::Other)?;
                self.0.write_str(s).map_err(|_| io::ErrorKind::Other)?;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        self.report
            // .report(self.sources, color)
            .write(Cache(&self.sources), Adaptor(f))
            .map_err(|_| fmt::Error)
    }
}

struct Cache<'a>(&'a SourceMap);

impl ariadne::Cache<FileId> for Cache<'_> {
    fn fetch(&mut self, file_id: &FileId) -> Result<&ariadne::Source, Box<dyn fmt::Debug + '_>> {
        struct NotFound(FileId);
        impl fmt::Debug for NotFound {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "source file not found: {:?}", self.0)
            }
        }
        if let Some(source_file) = self.0.get(file_id) {
            Ok(source_file.ariadne())
        } else if *file_id == FileId::NONE || *file_id == FileId::HACK_TMP {
            static EMPTY: OnceLock<ariadne::Source> = OnceLock::new();
            Ok(EMPTY.get_or_init(|| ariadne::Source::from("")))
        } else {
            Err(Box::new(NotFound(*file_id)))
        }
    }

    fn display<'a>(&self, file_id: &'a FileId) -> Option<Box<dyn fmt::Display + 'a>> {
        if *file_id != FileId::NONE {
            struct Path(Arc<SourceFile>);
            impl fmt::Display for Path {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.path().display().fmt(f)
                }
            }
            let source_file = self.0.get(file_id)?;
            Some(Box::new(Path(source_file.clone())))
        } else {
            struct NoSourceFile;
            impl fmt::Display for NoSourceFile {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("(no source file)")
                }
            }
            Some(Box::new(NoSourceFile))
        }
    }
}
