//! Built-in structured data (JSON / YAML / TOML / XML / CSV) and
//! text markup (Markdown / HTML / TXT) conversion.
//!
//! Structured formats are converted through `serde_json::Value` as a
//! common intermediate representation.

use std::path::Path;

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};
use serde_json::{Map, Value};

use super::{ConvertError, Result};

pub fn convert(input: &Path, output: &Path, from: &str, to: &str) -> Result<()> {
    let text = std::fs::read_to_string(input)
        .map_err(|e| ConvertError::failed(format!("Input is not valid UTF-8 text: {e}")))?;
    let title = input.file_stem().and_then(|s| s.to_str()).unwrap_or("Document");
    let out = convert_str(&text, from, to, title)?;
    std::fs::write(output, out)?;
    Ok(())
}

pub fn convert_str(text: &str, from: &str, to: &str, title: &str) -> Result<String> {
    match (from, to) {
        ("md", "html") => Ok(markdown_to_html(text, title)),
        ("md", "txt") => Ok(markdown_to_text(text)),
        ("html", "md") => html_to_markdown(text),
        ("html", "txt") => Ok(markdown_to_text(&html_to_markdown(text)?)),
        ("txt", "html") => Ok(wrap_html(
            title,
            &format!("<pre>{}</pre>", quick_xml::escape::escape(text)),
        )),
        _ => {
            let value = parse(text, from)?;
            serialize(&value, to)
        }
    }
}

// ---------------------------------------------------------------------------
// Structured data
// ---------------------------------------------------------------------------

fn parse_err(fmt: &str, e: impl std::fmt::Display) -> ConvertError {
    ConvertError::failed(format!("Invalid {}: {e}", fmt.to_uppercase()))
}

fn ser_err(fmt: &str, e: impl std::fmt::Display) -> ConvertError {
    ConvertError::failed(format!("Cannot write {}: {e}", fmt.to_uppercase()))
}

pub fn parse(text: &str, from: &str) -> Result<Value> {
    match from {
        "json" => serde_json::from_str(text).map_err(|e| parse_err(from, e)),
        "yaml" => serde_norway::from_str(text).map_err(|e| parse_err(from, e)),
        "toml" => {
            let v: Value = toml::from_str(text).map_err(|e| parse_err(from, e))?;
            Ok(unwrap_toml_datetimes(v))
        }
        "csv" => parse_csv(text),
        "xml" => parse_xml(text),
        _ => Err(ConvertError::failed(format!("Unsupported data format: {from}"))),
    }
}

pub fn serialize(value: &Value, to: &str) -> Result<String> {
    match to {
        "json" => serde_json::to_string_pretty(value).map_err(|e| ser_err(to, e)),
        "yaml" => serde_norway::to_string(value).map_err(|e| ser_err(to, e)),
        "toml" => {
            let table = match strip_nulls(value.clone()) {
                Value::Object(map) => Value::Object(map),
                // TOML documents must be tables; wrap other roots.
                other => {
                    let mut map = Map::new();
                    map.insert("items".into(), other);
                    Value::Object(map)
                }
            };
            toml::to_string_pretty(&table).map_err(|e| ser_err(to, e))
        }
        "csv" => write_csv(value),
        "xml" => Ok(write_xml(value)),
        _ => Err(ConvertError::failed(format!("Unsupported data format: {to}"))),
    }
}

/// The `toml` crate exposes datetimes as `{"$__toml_private_datetime": "…"}`
/// when deserialized into a foreign type; flatten them to plain strings.
fn unwrap_toml_datetimes(v: Value) -> Value {
    const KEY: &str = "$__toml_private_datetime";
    match v {
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(Value::String(s)) = map.get(KEY) {
                    return Value::String(s.clone());
                }
            }
            Value::Object(map.into_iter().map(|(k, v)| (k, unwrap_toml_datetimes(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(unwrap_toml_datetimes).collect()),
        other => other,
    }
}

/// TOML has no null: drop null keys and null array items.
fn strip_nulls(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().filter(|v| !v.is_null()).map(strip_nulls).collect()),
        other => other,
    }
}

// --- CSV --------------------------------------------------------------------

/// Infers numbers and booleans so CSV → JSON produces typed values.
fn infer_scalar(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::String(String::new());
    }
    match t {
        "true" | "TRUE" | "True" => return Value::Bool(true),
        "false" | "FALSE" | "False" => return Value::Bool(false),
        _ => {}
    }
    // Keep leading-zero identifiers like "007" or ZIP codes as strings.
    let leading_zero = t.len() > 1 && t.starts_with('0') && !t.starts_with("0.");
    if !leading_zero {
        if let Ok(i) = t.parse::<i64>() {
            return Value::from(i);
        }
        if let Ok(f) = t.parse::<f64>() {
            if f.is_finite() {
                return Value::from(f);
            }
        }
    }
    Value::String(s.to_string())
}

fn parse_csv(text: &str) -> Result<Value> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| parse_err("csv", e))?
        .iter()
        .map(str::to_string)
        .collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| parse_err("csv", e))?;
        let mut row = Map::new();
        for (i, field) in record.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("column{}", i + 1));
            row.insert(key, infer_scalar(field));
        }
        rows.push(Value::Object(row));
    }
    Ok(Value::Array(rows))
}

/// Flattens nested objects into `parent.child` columns.
fn flatten_into(prefix: &str, value: &Value, out: &mut Map<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten_into(&key, v, out);
            }
        }
        other => {
            out.insert(prefix.to_string(), other.clone());
        }
    }
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn write_csv(value: &Value) -> Result<String> {
    // Accept an array of rows, a single object, or an object wrapping one array.
    let rows: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        Value::Object(map) => match map.values().collect::<Vec<_>>().as_slice() {
            [Value::Array(items)] => items.clone(),
            _ => vec![value.clone()],
        },
        scalar => vec![scalar.clone()],
    };

    let flat: Vec<Map<String, Value>> = rows
        .iter()
        .map(|row| {
            let mut m = Map::new();
            match row {
                Value::Object(_) => flatten_into("", row, &mut m),
                other => {
                    m.insert("value".into(), other.clone());
                }
            }
            m
        })
        .collect();

    let mut headers: Vec<String> = Vec::new();
    for row in &flat {
        for k in row.keys() {
            if !headers.contains(k) {
                headers.push(k.clone());
            }
        }
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(&headers).map_err(|e| ser_err("csv", e))?;
    for row in &flat {
        let record: Vec<String> = headers.iter().map(|h| row.get(h).map(cell).unwrap_or_default()).collect();
        writer.write_record(&record).map_err(|e| ser_err("csv", e))?;
    }
    let bytes = writer.into_inner().map_err(|e| ser_err("csv", e))?;
    String::from_utf8(bytes).map_err(|e| ser_err("csv", e))
}

// --- XML --------------------------------------------------------------------
// Mapping: attributes → "@name", text alongside children/attributes → "#text",
// repeated child elements → arrays. The root element becomes the single key.

struct Frame {
    name: String,
    map: Map<String, Value>,
    text: String,
}

fn push_child(map: &mut Map<String, Value>, key: String, value: Value) {
    match map.get_mut(&key) {
        Some(Value::Array(items)) => items.push(value),
        Some(existing) => {
            let prev = existing.take();
            *existing = Value::Array(vec![prev, value]);
        }
        None => {
            map.insert(key, value);
        }
    }
}

fn finish(frame: Frame) -> Value {
    let text = frame.text.trim();
    if frame.map.is_empty() {
        return Value::String(text.to_string());
    }
    let mut map = frame.map;
    if !text.is_empty() {
        map.insert("#text".into(), Value::String(text.to_string()));
    }
    Value::Object(map)
}

fn open_frame(e: &quick_xml::events::BytesStart) -> Result<Frame> {
    let name = e.name().as_ref().to_string();
    let mut map = Map::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|err| parse_err("xml", err))?;
        let key = format!("@{}", attr.key.as_ref());
        let value = attr
            .normalized_value(quick_xml::XmlVersion::default())
            .map_err(|err| parse_err("xml", err))?;
        map.insert(key, Value::String(value.into_owned()));
    }
    Ok(Frame { name, map, text: String::new() })
}

fn resolve_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => return None,
    })
}

fn parse_xml(text: &str) -> Result<Value> {
    use quick_xml::events::Event as X;

    let mut reader = quick_xml::Reader::from_str(text);
    let mut root = Map::new();
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        match reader.read_event().map_err(|e| parse_err("xml", e))? {
            X::Start(e) => stack.push(open_frame(&e)?),
            X::Empty(e) => {
                let frame = open_frame(&e)?;
                let name = frame.name.clone();
                let value = finish(frame);
                match stack.last_mut() {
                    Some(parent) => push_child(&mut parent.map, name, value),
                    None => push_child(&mut root, name, value),
                }
            }
            X::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            X::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            X::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        top.text.push(ch);
                    } else {
                        let name = r.xml10_content();
                        top.text.push_str(resolve_entity(&name).unwrap_or(""));
                    }
                }
            }
            X::End(_) => {
                let frame = stack.pop().ok_or_else(|| parse_err("xml", "unbalanced closing tag"))?;
                let name = frame.name.clone();
                let value = finish(frame);
                match stack.last_mut() {
                    Some(parent) => push_child(&mut parent.map, name, value),
                    None => push_child(&mut root, name, value),
                }
            }
            X::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(parse_err("xml", "unexpected end of document"));
    }
    Ok(Value::Object(root))
}

fn xml_name(key: &str) -> String {
    let mut name: String = key
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') { c } else { '_' })
        .collect();
    if name.is_empty() || !name.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        name.insert(0, '_');
    }
    name
}

fn xml_scalar(v: &Value) -> String {
    quick_xml::escape::escape(cell(v)).into_owned()
}

fn write_element(out: &mut String, name: &str, value: &Value, depth: usize) {
    let indent = "  ".repeat(depth);
    let tag = xml_name(name);
    match value {
        Value::Array(items) => {
            for item in items {
                write_element(out, name, item, depth);
            }
        }
        Value::Object(map) => {
            out.push_str(&format!("{indent}<{tag}"));
            for (k, v) in map.iter().filter(|(k, _)| k.starts_with('@')) {
                out.push_str(&format!(" {}=\"{}\"", xml_name(&k[1..]), xml_scalar(v)));
            }
            let children: Vec<_> = map.iter().filter(|(k, _)| !k.starts_with('@') && *k != "#text").collect();
            let text = map.get("#text").map(xml_scalar).unwrap_or_default();
            if children.is_empty() {
                if text.is_empty() {
                    out.push_str("/>\n");
                } else {
                    out.push_str(&format!(">{text}</{tag}>\n"));
                }
            } else {
                out.push_str(">\n");
                if !text.is_empty() {
                    out.push_str(&format!("{indent}  {text}\n"));
                }
                for (k, v) in children {
                    write_element(out, k, v, depth + 1);
                }
                out.push_str(&format!("{indent}</{tag}>\n"));
            }
        }
        Value::Null => out.push_str(&format!("{indent}<{tag}/>\n")),
        scalar => out.push_str(&format!("{indent}<{tag}>{}</{tag}>\n", xml_scalar(scalar))),
    }
}

fn write_xml(value: &Value) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match value {
        // A single-key object maps naturally onto a root element.
        Value::Object(map) if map.len() == 1 && !map.values().next().unwrap().is_array() => {
            let (k, v) = map.iter().next().unwrap();
            write_element(&mut out, k, v, 0);
        }
        Value::Array(items) => {
            out.push_str("<root>\n");
            for item in items {
                write_element(&mut out, "item", item, 1);
            }
            out.push_str("</root>\n");
        }
        other => write_element(&mut out, "root", other, 0),
    }
    out
}

// ---------------------------------------------------------------------------
// Markdown / HTML / text
// ---------------------------------------------------------------------------

fn md_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
}

fn wrap_html(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n\
<style>body{{max-width:720px;margin:48px auto;padding:0 20px;font:16px/1.6 system-ui,sans-serif;color:#111}}\
pre,code{{font-family:ui-monospace,monospace;font-size:14px}}pre{{white-space:pre-wrap}}\
table{{border-collapse:collapse}}td,th{{border:1px solid #e5e5e5;padding:4px 8px}}</style>\n\
</head>\n<body>\n{}\n</body>\n</html>\n",
        quick_xml::escape::escape(title),
        body
    )
}

pub fn markdown_to_html(md: &str, title: &str) -> String {
    let mut body = String::new();
    html::push_html(&mut body, Parser::new_ext(md, md_options()));
    wrap_html(title, body.trim_end())
}

pub fn html_to_markdown(html: &str) -> Result<String> {
    htmd::convert(html).map_err(|e| ConvertError::failed(format!("HTML conversion failed: {e}")))
}

pub fn markdown_to_text(md: &str) -> String {
    let mut out = String::new();
    let mut list_depth = 0usize;
    for event in Parser::new_ext(md, md_options()) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                out.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
                out.push_str("- ");
            }
            Event::End(TagEnd::Item) => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::BlockQuote(_)) => {
                if list_depth == 0 {
                    out.push_str("\n\n");
                } else if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Event::End(TagEnd::TableCell) => out.push('\t'),
            Event::End(TagEnd::TableRow | TagEnd::TableHead) => out.push('\n'),
            Event::Rule => out.push_str("\n---\n\n"),
            _ => {}
        }
    }
    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_to_yaml() {
        let yaml = convert_str(r#"{"name":"AnyFile","tags":["a","b"],"n":3}"#, "json", "yaml", "t").unwrap();
        assert!(yaml.contains("name: AnyFile"));
        assert!(yaml.contains("- a"));
        let back = parse(&yaml, "yaml").unwrap();
        assert_eq!(back["n"], 3);
    }

    #[test]
    fn yaml_to_toml_and_back() {
        let toml = convert_str("title: x\nowner:\n  name: y\nnums: [1, 2]\nmissing: null\n", "yaml", "toml", "t").unwrap();
        assert!(toml.contains("title = \"x\""));
        assert!(toml.contains("[owner]"));
        let back = parse(&toml, "toml").unwrap();
        assert_eq!(back["owner"]["name"], "y");
    }

    #[test]
    fn toml_datetimes_become_strings() {
        let json = convert_str("when = 1979-05-27T07:32:00Z\n", "toml", "json", "t").unwrap();
        assert!(json.contains("\"when\": \"1979-05-27T07:32:00Z\""));
    }

    #[test]
    fn csv_round_trip() {
        let json = convert_str("id,name,zip\n1,Ada,02139\n2,Linus,94105\n", "csv", "json", "t").unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["id"], 1);
        assert_eq!(v[0]["zip"], "02139");
        let csv = convert_str(&json, "json", "csv", "t").unwrap();
        assert!(csv.starts_with("id,name,zip"));
        assert!(csv.contains("2,Linus,94105"));
    }

    #[test]
    fn nested_json_to_csv_flattens() {
        let csv = convert_str(r#"[{"a":1,"b":{"c":2}},{"a":3,"d":true}]"#, "json", "csv", "t").unwrap();
        assert_eq!(csv.lines().next().unwrap(), "a,b.c,d");
        assert!(csv.contains("3,,true"));
    }

    #[test]
    fn xml_round_trip() {
        let src = r#"<?xml version="1.0"?><library id="7"><book lang="en">Dune &amp; more</book><book>Neuromancer</book><empty/></library>"#;
        let v = parse(src, "xml").unwrap();
        assert_eq!(v["library"]["@id"], "7");
        assert_eq!(v["library"]["book"][0]["#text"], "Dune & more");
        assert_eq!(v["library"]["book"][1], "Neuromancer");
        let xml = serialize(&v, "xml").unwrap();
        assert!(xml.contains("<library id=\"7\">"));
        assert!(xml.contains("<book lang=\"en\">Dune &amp; more</book>"));
    }

    #[test]
    fn md_to_html() {
        let html = convert_str("# Title\n\nSome **bold** text.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n", "md", "html", "Doc").unwrap();
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<title>Doc</title>"));
    }

    #[test]
    fn md_and_html_to_text() {
        let txt = convert_str("# Title\n\n- one\n- two\n\nEnd.", "md", "txt", "t").unwrap();
        assert_eq!(txt, "Title\n\n- one\n- two\n\nEnd.\n");
        let md = convert_str("<h2>Hi</h2><p>A <em>b</em></p>", "html", "md", "t").unwrap();
        assert!(md.contains("## Hi"));
        assert!(md.contains("_b_") || md.contains("*b*"));
    }
}
