//! Nginx combined access log parser (ported from logutil).

use std::borrow::Cow;

use nom::bytes::complete::{tag, take_until, take_while, take_while1};
use nom::character::complete::{digit1, space0, space1};
use nom::combinator::{map_res, opt};
use nom::sequence::delimited;
use nom::Parser;

use crate::log::record::{LogFormat, LogRecord};

pub fn looks_like_nginx(line: &str) -> bool {
    let t = line.trim();
    t.contains(" - \"")
        && t.contains('[')
        && t.contains(']')
        && (t.contains("GET ")
            || t.contains("POST ")
            || t.contains("PUT ")
            || t.contains("DELETE "))
}

pub fn parse_line<'a>(line: &'a str) -> Option<LogRecord<'a>> {
    let (_, parsed) = parse_nginx(line).ok()?;
    let mut rec = LogRecord::from_line(line);
    rec.format = LogFormat::Nginx;
    rec.client_ip = Some(Cow::Borrowed(parsed.ip));
    rec.method = Some(Cow::Borrowed(parsed.http_method));
    rec.path = Some(Cow::Borrowed(parsed.url_path));
    rec.status = parsed.status_code;
    rec.duration_ms = parsed.response_time.map(|f| f as f64 * 1000.0);
    rec.message = Cow::Owned(format!(
        "{} {} {}",
        parsed.http_method,
        parsed.url_path,
        parsed.status_code.unwrap_or(0)
    ));
    if parsed.status_code.unwrap_or(0) >= 500 {
        rec.level = Some(crate::log::level::LogLevel::Error);
    } else if parsed.status_code.unwrap_or(0) >= 400 {
        rec.level = Some(crate::log::level::LogLevel::Warn);
    } else {
        rec.level = Some(crate::log::level::LogLevel::Info);
    }
    Some(rec)
}

struct ParsedNginx<'a> {
    ip: &'a str,
    http_method: &'a str,
    url_path: &'a str,
    status_code: Option<u16>,
    response_time: Option<f32>,
}

fn parse_quoted(input: &str) -> nom::IResult<&str, &str> {
    delimited(tag("\""), take_until("\""), tag("\"")).parse(input)
}

fn parse_nginx(input: &str) -> nom::IResult<&str, ParsedNginx<'_>> {
    let (input, ip) = take_while1(|c: char| !c.is_whitespace()).parse(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = tag("-").parse(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = parse_quoted(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = delimited(tag("["), take_until("]"), tag("]")).parse(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = map_res(
        take_while1(|c: char| c.is_ascii_digit() || c == '.'),
        |s: &str| s.parse::<f32>(),
    )
    .parse(input)?;
    let (input, _) = space0(input)?;
    let (input, http_method) = parse_quoted(input)?;
    let (input, _) = space0(input)?;
    let (input, request_line) = parse_quoted(input)?;
    let (input, _) = space0(input)?;
    let (_, (method, url)) = parse_request(request_line).unwrap_or((request_line, ("", "")));
    let (input, status_code) = opt(map_res(digit1, |s: &str| s.parse::<u16>())).parse(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = opt(map_res(digit1, |s: &str| s.parse::<u64>())).parse(input)?;
    let (input, _) = space0(input)?;
    let (input, response_time) = opt(map_res(
        take_while1(|c: char| c.is_ascii_digit() || c == '.'),
        |s: &str| s.parse::<f32>(),
    ))
    .parse(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = opt(parse_quoted).parse(input)?;
    Ok((
        input,
        ParsedNginx {
            ip,
            http_method: if method.is_empty() {
                http_method
            } else {
                method
            },
            url_path: if url.is_empty() { request_line } else { url },
            status_code,
            response_time,
        },
    ))
}

fn parse_request(input: &str) -> nom::IResult<&str, (&str, &str)> {
    let (input, method) = take_while1(|c: char| !c.is_whitespace()).parse(input)?;
    let (input, _) = space1(input)?;
    let (input, url) = take_while(|c: char| c != ' ' && c != '"').parse(input)?;
    Ok((input, (method, url)))
}
