pub mod capabilities;
pub mod diff;
pub mod duplicates;
pub mod encoding;
pub mod find;
pub mod hash;
pub mod info;
pub mod query;
pub mod report;
pub mod schema;
pub mod stats;
pub mod table;
pub mod text;
pub mod watch;

use std::io::{self, Write};
use std::path::Path;

use anyhow::{bail, Result};

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::input::InputRef;
use crate::inspect::capabilities::Capabilities;
use crate::inspect::report::{CapabilitiesReport, CapabilityLine, InspectReport};
use crate::orchestrator::registry::DriverRegistry;
use crate::orchestrator::PreviewOrchestrator;
use crate::sinks::report::write_report;

/// Options for inspector commands (parsed by CLI).
#[derive(Debug, Clone, Default)]
pub struct InspectOptions {
    pub json: bool,
    pub info: bool,
    pub schema: bool,
    pub stats: bool,
    pub type_only: bool,
    pub mime_only: bool,
    pub encoding: bool,
    pub capabilities: bool,
    pub find: Option<String>,
    pub query: Option<String>,
    pub where_clause: Option<String>,
    pub columns: bool,
    pub head: Option<usize>,
    pub tail: Option<usize>,
    pub watch: bool,
    pub follow: bool,
    pub level: Option<String>,
    pub text: bool,
    pub raw: bool,
    pub hash: bool,
    pub duplicates: bool,
    pub diff: Option<(String, String)>,
    pub allow_unsafe: bool,
    pub all: bool,
}

impl InspectOptions {
    pub fn is_inspect_mode(&self) -> bool {
        self.info
            || self.schema
            || self.stats
            || self.type_only
            || self.mime_only
            || self.encoding
            || self.capabilities
            || self.find.is_some()
            || self.query.is_some()
            || self.where_clause.is_some()
            || self.columns
            || self.head.is_some()
            || self.tail.is_some()
            || self.watch
            || self.follow
            || self.text
            || self.raw
            || self.hash
            || self.duplicates
            || self.diff.is_some()
            || self.json // alone not enough — needs a mode; handled by CLI
    }

    pub fn has_action(&self) -> bool {
        self.info
            || self.schema
            || self.stats
            || self.type_only
            || self.mime_only
            || self.encoding
            || self.capabilities
            || self.find.is_some()
            || self.query.is_some()
            || self.where_clause.is_some()
            || self.columns
            || self.head.is_some()
            || self.tail.is_some()
            || self.watch
            || self.follow
            || self.text
            || self.raw
            || self.hash
            || self.duplicates
            || self.diff.is_some()
    }
}

pub fn run_inspect(
    raw_path: Option<&str>,
    options: &InspectOptions,
    config: &OmnicatConfig,
) -> Result<()> {
    let mut config = config.clone();
    if options.all {
        config.inspect.no_limit = true;
        config.inspect.max_bytes = 0;
        config.inspect.max_rows = 0;
    }
    config.inspect.allow_unsafe = options.allow_unsafe;

    if options.capabilities && raw_path.is_none() {
        return print_all_capabilities(options.json);
    }

    if let Some((left, right)) = &options.diff {
        return run_diff(left, right, options, &config);
    }

    let raw_path = raw_path.ok_or_else(|| anyhow::anyhow!("path required"))?;

    if options.follow {
        let input = InputRef::parse(raw_path)?;
        return crate::log::follow::follow_path(
            input.path_for_ops(),
            options.level.as_deref(),
            options.allow_unsafe,
            &crate::log::filter::LogFilter::default(),
        );
    }

    if options.watch {
        let input = InputRef::parse(raw_path)?;
        return watch::watch_file(input.path_for_ops(), options, &config);
    }

    if options.raw {
        let input = InputRef::parse(raw_path)?;
        let mut stdout = io::stdout().lock();
        return text::write_raw(input.path_for_ops(), &mut stdout);
    }

    run_inspect_path_str(raw_path, options, &config)
}

pub fn run_inspect_path(
    path: &Path,
    options: &InspectOptions,
    config: &OmnicatConfig,
) -> Result<()> {
    run_inspect_path_str(&path.to_string_lossy(), options, config)
}

fn run_inspect_path_str(
    raw_path: &str,
    options: &InspectOptions,
    config: &OmnicatConfig,
) -> Result<()> {
    let input = InputRef::parse(raw_path)?;
    let path = input.path_for_ops();
    let display = input.display_name();
    let kind = resolve_kind(path, config);

    if options.capabilities {
        let caps = Capabilities::for_kind(kind);
        let report = InspectReport::Capabilities(CapabilitiesReport {
            path: Some(display),
            handler: Some(kind.name().into()),
            capabilities: caps
                .labels()
                .into_iter()
                .map(|(name, supported)| CapabilityLine {
                    name: name.into(),
                    supported,
                })
                .collect(),
        });
        return emit(&report, options, config);
    }

    if options.text {
        let extracted = text::extract_text(path, kind, config)?;
        let safe = text::sanitize_text(&extracted, options.allow_unsafe);
        let report = InspectReport::Text {
            path: display,
            text: safe,
        };
        return emit(&report, options, config);
    }

    if options.mime_only {
        let t = info::build_type(path, &display, kind)?;
        if options.json {
            let report = InspectReport::Type(t);
            return emit(&report, options, config);
        }
        let mut stdout = io::stdout().lock();
        if let Some(m) = t.mime.or(t.detected) {
            writeln!(stdout, "{m}")?;
        } else {
            writeln!(stdout, "application/octet-stream")?;
        }
        return Ok(());
    }

    if options.type_only {
        let report = InspectReport::Type(info::build_type(path, &display, kind)?);
        return emit(&report, options, config);
    }

    if options.encoding {
        let max = if config.inspect.no_limit {
            0
        } else {
            config.inspect.max_bytes
        };
        let report = InspectReport::Encoding(encoding::encoding_report(path, max)?);
        return emit(&report, options, config);
    }

    if options.info {
        let report = InspectReport::Info(Box::new(info::build_info(path, &display, kind, config)?));
        return emit(&report, options, config);
    }

    if options.hash {
        let report = InspectReport::Hash(hash::build_hash(path, &display, config)?);
        return emit(&report, options, config);
    }

    if options.duplicates {
        let report =
            InspectReport::Duplicates(duplicates::build_duplicates(path, &display, config)?);
        return emit(&report, options, config);
    }

    if options.schema {
        let report = InspectReport::Schema(schema::build_schema(path, &display, kind, config)?);
        return emit(&report, options, config);
    }

    if options.stats {
        let report = InspectReport::Stats(stats::build_stats(path, &display, kind, config)?);
        return emit(&report, options, config);
    }

    if let Some(q) = &options.find {
        let report = InspectReport::Find(find::build_find(path, &display, kind, q, config)?);
        return emit(&report, options, config);
    }

    let query = options
        .query
        .clone()
        .or_else(|| options.where_clause.clone());
    if let Some(q) = query {
        let report = InspectReport::Query(query::build_query(path, &display, kind, &q, config)?);
        return emit(&report, options, config);
    }

    if options.columns || options.head.is_some() || options.tail.is_some() {
        let report = table::table_view(
            path,
            &display,
            kind,
            config,
            options.columns,
            options.head,
            options.tail,
        )?;
        return emit(&report, options, config);
    }

    bail!("no inspect action specified");
}

fn run_diff(
    left: &str,
    right: &str,
    options: &InspectOptions,
    config: &OmnicatConfig,
) -> Result<()> {
    let left_in = InputRef::parse(left)?;
    let right_in = InputRef::parse(right)?;
    let lk = resolve_kind(left_in.path_for_ops(), config);
    let rk = resolve_kind(right_in.path_for_ops(), config);
    let report = InspectReport::Diff(diff::build_diff(
        left_in.path_for_ops(),
        &left_in.display_name(),
        right_in.path_for_ops(),
        &right_in.display_name(),
        lk,
        rk,
        config,
    )?);
    emit(&report, options, config)
}

fn resolve_kind(path: &Path, config: &OmnicatConfig) -> HandlerKind {
    PreviewOrchestrator::resolve(path, config)
        .and_then(|r| match r {
            crate::orchestrator::ResolvedHandler::Builtin(k) => Some(k),
            _ => None,
        })
        .or_else(|| DriverRegistry::detect_builtin(path))
        .unwrap_or(HandlerKind::Fallback)
}

fn emit(report: &InspectReport, options: &InspectOptions, _config: &OmnicatConfig) -> Result<()> {
    let mut stdout = io::stdout().lock();
    write_report(report, options.json, &mut stdout)?;
    Ok(())
}

/// Row cap for table/query. `0` or `no_limit` means unlimited.
pub(crate) fn effective_max_rows(config: &OmnicatConfig) -> usize {
    if config.inspect.no_limit || config.inspect.max_rows == 0 {
        usize::MAX
    } else {
        config.inspect.max_rows
    }
}

fn print_all_capabilities(json: bool) -> Result<()> {
    #[derive(serde::Serialize)]
    struct AllCaps {
        handlers: Vec<HandlerCaps>,
    }
    #[derive(serde::Serialize)]
    struct HandlerCaps {
        handler: String,
        capabilities: Vec<CapabilityLine>,
    }

    let handlers: Vec<HandlerCaps> = HandlerKind::all()
        .iter()
        .map(|kind| {
            let caps = Capabilities::for_kind(*kind);
            HandlerCaps {
                handler: kind.name().into(),
                capabilities: caps
                    .labels()
                    .into_iter()
                    .map(|(name, supported)| CapabilityLine {
                        name: name.into(),
                        supported,
                    })
                    .collect(),
            }
        })
        .collect();

    let mut stdout = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut stdout, &AllCaps { handlers })?;
        writeln!(stdout)?;
    } else {
        for h in handlers {
            writeln!(stdout, "{}", h.handler)?;
            writeln!(stdout, "{}", "─".repeat(20))?;
            for c in h.capabilities {
                let mark = if c.supported { "✓" } else { "✗" };
                writeln!(stdout, "{mark} {}", c.name)?;
            }
            writeln!(stdout)?;
        }
    }
    Ok(())
}
