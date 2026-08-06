//! Log command output reports.

use crate::log::aggregate::counters::LogCounters;
use crate::log::aggregate::rate::RateAgg;
use crate::log::aggregate::timeline::TimelineAgg;
use crate::log::aggregate::Aggregators;
use crate::log::aggregate::HttpAgg;
use crate::log::aggregate::TopKAgg;
use crate::log::context::ContextLine;
use crate::log::correlate::TraceReport;
use crate::log::options::LogOptions;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogReport {
    Lines { lines: Vec<String> },
    Stats { counters: LogCounters },
    Timeline { timeline: TimelineAgg },
    Rate { rate: RateAgg },
    Top { field: String, items: Vec<TopItem> },
    Slow { entries: Vec<SlowItem> },
    Http { http: HttpAgg },
    Trace(TraceReport),
    Context { lines: Vec<ContextLine> },
    Query { result: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TopItem {
    pub key: String,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlowItem {
    pub duration_ms: f64,
    pub label: String,
}

impl LogReport {
    pub fn from_aggregators(agg: &Aggregators, opts: &LogOptions) -> Self {
        if opts.timeline {
            return LogReport::Timeline {
                timeline: agg.timeline.clone(),
            };
        }
        if opts.rate {
            return LogReport::Rate {
                rate: if opts.rate_errors {
                    agg.rate_errors.clone()
                } else {
                    agg.rate.clone()
                },
            };
        }
        if opts.http {
            return LogReport::Http {
                http: agg.http.clone(),
            };
        }
        if opts.slow {
            return LogReport::Slow {
                entries: agg
                    .slow
                    .top(opts.slow_limit)
                    .into_iter()
                    .map(|e| SlowItem {
                        duration_ms: e.duration_ms,
                        label: e.label.clone(),
                    })
                    .collect(),
            };
        }
        if opts.top.is_some() {
            let field = opts.top.clone().unwrap_or_else(|| "message".into());
            let top: &TopKAgg = match field.as_str() {
                "errors" => &agg.top_errors,
                "endpoints" | "path" => &agg.top_endpoint,
                "ips" | "ip" => &agg.top_ip,
                _ => &agg.top_message,
            };
            return LogReport::Top {
                field,
                items: top
                    .top(opts.top_limit)
                    .into_iter()
                    .map(|(k, c)| TopItem { key: k, count: c })
                    .collect(),
            };
        }
        LogReport::Stats {
            counters: agg.counters.clone(),
        }
    }
}
