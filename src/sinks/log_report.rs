//! Human and JSON output for log reports.

use std::io::Write;

use anyhow::Result;

use crate::log::report::LogReport;

pub fn write_log_report(report: &LogReport, json: bool, out: &mut dyn Write) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    match report {
        LogReport::Lines { lines } => {
            for line in lines {
                writeln!(out, "{line}")?;
            }
        }
        LogReport::Stats { counters } => {
            writeln!(out, "Log statistics")?;
            writeln!(out, "{}", "─".repeat(40))?;
            writeln!(out, "{:<14} {}", "Messages", counters.messages)?;
            if let (Some(a), Some(b)) = (counters.first_ts, counters.last_ts) {
                writeln!(
                    out,
                    "{:<14} {} → {}",
                    "Time range",
                    a.format("%H:%M:%S"),
                    b.format("%H:%M:%S")
                )?;
            }
            if let Some(d) = counters.duration_human() {
                writeln!(out, "{:<14} {}", "Duration", d)?;
            }
            if !counters.levels.is_empty() {
                writeln!(out)?;
                writeln!(out, "Levels")?;
                writeln!(out, "{}", "─".repeat(20))?;
                let mut levels: Vec<_> = counters.levels.iter().collect();
                levels.sort_by(|a, b| b.1.cmp(a.1));
                for (k, v) in levels {
                    writeln!(out, "{:<14} {}", k, v)?;
                }
            }
            if !counters.services.is_empty() {
                writeln!(out)?;
                writeln!(out, "Services")?;
                writeln!(out, "{}", "─".repeat(20))?;
                let mut svcs: Vec<_> = counters.services.iter().collect();
                svcs.sort_by(|a, b| b.1.cmp(a.1));
                for (k, v) in svcs.iter().take(20) {
                    writeln!(out, "{:<14} {}", k, v)?;
                }
            }
        }
        LogReport::Timeline { timeline } => {
            write!(out, "{}", timeline.render_ascii(30))?;
        }
        LogReport::Rate { rate } => {
            write!(out, "{}", rate.render())?;
        }
        LogReport::Top { field, items } => {
            writeln!(out, "Top {field}")?;
            writeln!(out, "{}", "─".repeat(44))?;
            for (i, item) in items.iter().enumerate() {
                writeln!(out, "{:>6}  {}", item.count, item.key)?;
                if i >= 49 {
                    break;
                }
            }
        }
        LogReport::Slow { entries } => {
            writeln!(out, "Slow operations")?;
            writeln!(out, "{}", "─".repeat(44))?;
            for e in entries {
                writeln!(out, "{:>7.2}ms  {}", e.duration_ms, e.label)?;
            }
        }
        LogReport::Http { http } => {
            writeln!(out, "HTTP summary")?;
            writeln!(out, "{:<14} {}", "Requests", http.requests)?;
            writeln!(out, "{:<14} {}", "2xx", http.s2xx)?;
            writeln!(out, "{:<14} {}", "3xx", http.s3xx)?;
            writeln!(out, "{:<14} {}", "4xx", http.s4xx)?;
            writeln!(out, "{:<14} {}", "5xx", http.s5xx)?;
            if !http.methods.is_empty() {
                writeln!(out)?;
                writeln!(out, "Methods")?;
                for (m, c) in &http.methods {
                    writeln!(out, "{:<14} {}", m, c)?;
                }
            }
        }
        LogReport::Trace(t) => {
            writeln!(out, "Trace {}", t.id)?;
            for line in &t.lines {
                if let Some(ref ts) = line.timestamp {
                    write!(out, "{ts} ")?;
                }
                if let Some(ref svc) = line.service {
                    write!(out, "[{svc}] ")?;
                }
                writeln!(out, "{}", line.message)?;
            }
            if let Some(d) = t.total_duration_ms {
                writeln!(out, "\nDuration: {:.2}ms", d)?;
            }
        }
        LogReport::Context { lines } => {
            for line in lines {
                let mark = if line.marker { ">>" } else { "  " };
                writeln!(out, "{mark} {}", line.text)?;
            }
        }
        LogReport::Query { result } => {
            write!(out, "{result}")?;
        }
    }
    Ok(())
}
