pub mod counters;
pub mod http;
pub mod rate;
pub mod slow;
pub mod timeline;
pub mod topk;

pub use counters::LogCounters;
pub use http::HttpAgg;
pub use rate::RateAgg;
pub use slow::SlowAgg;
pub use timeline::TimelineAgg;
pub use topk::TopKAgg;

use counters::LogCounters as Counters;
use http::HttpAgg as Http;
use rate::RateAgg as Rate;
use slow::SlowAgg as Slow;
use timeline::TimelineAgg as Timeline;
use topk::TopKAgg as TopK;

use crate::log::record::LogRecord;

/// All streaming aggregators updated in one pass.
#[derive(Debug, Default)]
pub struct Aggregators {
    pub counters: Counters,
    pub timeline: Timeline,
    pub rate: Rate,
    pub rate_errors: Rate,
    pub top_message: TopK,
    pub top_errors: TopK,
    pub top_endpoint: TopK,
    pub top_ip: TopK,
    pub slow: Slow,
    pub http: Http,
}

impl Aggregators {
    pub fn new(timeline_interval: i64, slow_limit: usize) -> Self {
        Self {
            timeline: TimelineAgg::new(timeline_interval),
            rate: RateAgg::new(timeline_interval, false),
            rate_errors: RateAgg::new(timeline_interval, true),
            top_message: TopKAgg::new("message"),
            top_errors: TopKAgg::new("message"),
            top_endpoint: TopKAgg::new("path"),
            top_ip: TopKAgg::new("ip"),
            slow: SlowAgg::new(slow_limit),
            ..Default::default()
        }
    }

    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        self.counters.observe(rec);
        self.timeline.observe(rec);
        self.rate.observe(rec);
        self.rate_errors.observe(rec);
        self.top_message.observe(rec);
        if rec.level.is_some_and(|l| l.is_errorish()) {
            self.top_errors.observe(rec);
        }
        self.top_endpoint.observe(rec);
        self.top_ip.observe(rec);
        self.slow.observe(rec);
        self.http.observe(rec);
    }
}
