#![forbid(unsafe_code)]

//! HL7 version 2 in ER7, the vertical-bar encoding: a message as its
//! segments, one part each, named by the tag — `MSH`, `EVN`, `PID`, `PV1`,
//! `OBX` — in the order they lie. The delimiters are the ones `MSH`
//! announces: the field separator is its fourth character and the component
//! separator the first of the encoding characters that follow. A segment
//! ends at a carriage return, as the standard says, and at a line feed
//! too, because that is how such messages arrive; an empty line is no
//! segment, and nothing but line breaks stands between two. The announced
//! type is MSH-9: the message
//! structure when the third component gives it, else the message code and
//! trigger event joined by an underscore — `ADT_A01`, `ORU_R01` — else the
//! code alone (ADR-0047).
//!
//! The walk is the Foundation's `message::segment`, the one the HL7 v2
//! contract reads by too: this shape reads the head and brings the
//! delimiters. A contract checks everything else. The `MSH` is read here
//! once ([`encoding`]), and the HL7 v2 contract reads a message's head by
//! it too (ADR-0044, amendment 2026-11-15).

use message::segment::{self, Delimiters};
use message::{Shape, ShapeError, Shaped};
use stream::Stream;

/// The HL7 v2 ER7 shape.
#[derive(Clone, Copy, Debug, Default)]
pub struct Er7;

/// MSH-1 and MSH-2, the encoding characters, as the `MSH` at the head of
/// `bytes` declares them: the field separator, then the component,
/// repetition, escape and subcomponent characters. The walk reads bytes, so
/// each is ASCII, and none of MSH-2's four is the field separator.
///
/// # Errors
/// No `MSH` at the head, one shorter than its encoding characters, or one
/// with a field separator or an encoding character outside ASCII, or an
/// encoding character that is its field separator.
pub fn encoding(bytes: &[u8]) -> Result<[u8; 5], ShapeError> {
    let refused = |reason: &str, at: usize| Err(ShapeError::new("hl7-er7", reason).at(at));
    let short = "an MSH shorter than its encoding characters";
    if !bytes.starts_with(b"MSH") {
        return refused("no MSH at the head", 0);
    }
    let Some(&field) = bytes.get(3) else {
        return refused(short, bytes.len());
    };
    if !field.is_ascii() {
        return refused("an MSH with a field separator outside ASCII", 3);
    }
    let Some(&[component, repetition, escape, subcomponent]) = bytes.get(4..8) else {
        return refused(short, bytes.len());
    };
    let declared = [component, repetition, escape, subcomponent];
    if let Some(at) = declared.iter().position(|byte| !byte.is_ascii()) {
        return refused("an MSH with an encoding character outside ASCII", 4 + at);
    }
    if let Some(at) = declared.iter().position(|&byte| byte == field) {
        let reason = "an MSH with an encoding character that is its field separator";
        return refused(reason, 4 + at);
    }
    Ok([field, component, repetition, escape, subcomponent])
}

/// What the walk cuts ER7 by, given the encoding characters [`encoding`]
/// read: the field separator and the component separator, a segment ended
/// by every line break.
#[must_use]
pub const fn delimiters(encoding: [u8; 5]) -> Delimiters {
    Delimiters::new(b'\r', encoding[0], encoding[1]).ended_by_line_breaks()
}

/// MSH-9 as a name: the structure, else code and event joined, else the
/// code.
fn message_type(msh: &segment::Segment<'_>, delimiters: Delimiters) -> Option<String> {
    let component = |n| {
        msh.component(7, n, &delimiters)
            .filter(|c| !c.is_empty())
            .map(|c| String::from_utf8_lossy(c).into_owned())
    };
    if let Some(structure) = component(2) {
        return Some(structure);
    }
    let code = component(0)?;
    Some(match component(1) {
        Some(event) => format!("{code}_{event}"),
        None => code,
    })
}

impl Shape for Er7 {
    fn technology(&self) -> &'static str {
        "hl7-er7"
    }

    fn media_types(&self) -> &'static [&'static str] {
        &["x-application/hl7-v2+er7", "application/hl7-v2+er7"]
    }

    fn recognizes(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"MSH")
            && bytes
                .get(3)
                .is_some_and(|b| !b.is_ascii_alphanumeric() && !b.is_ascii_whitespace())
    }

    fn shape(&self, stream: &Stream) -> Result<Shaped, ShapeError> {
        let bytes = message::loaded(self.technology(), stream)?;
        let delimiters = delimiters(encoding(bytes)?);
        let segments = segment::segments(bytes, &delimiters)
            .map_err(|stop| ShapeError::refused("hl7-er7", stop))?;
        let message_type =
            segment::first(&segments, "MSH").and_then(|msh| message_type(msh, delimiters));
        let media = stream.media_type().unwrap_or("x-application/hl7-v2+er7");
        Ok(Shaped {
            parts: segment::parts(&segments, media),
            message_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcore::StreamId;

    const ADMIT: &[u8] = b"MSH|^~\\&|HIS|WARD|LAB|LAB|20240101120000||ADT^A01|MSG-1|P|2.5\r\
EVN|A01|20240101120000\rPID|1||12345^^^HIS^MR||DOE^JOHN||19700101|M\r\
PV1|1|I|W1^B2^1\r";

    fn stream(bytes: &[u8], media: Option<&str>) -> Stream {
        Stream::new(StreamId::new(1), bytes.to_vec(), media.map(str::to_string))
    }

    #[test]
    fn a_message_is_its_segments_named_by_tag_and_msh_9_is_the_announced_type() {
        let shaped = Er7.shape(&stream(ADMIT, None)).expect("well-formed");
        let names: Vec<&str> = shaped
            .parts
            .iter()
            .filter_map(|p| p.name.as_deref())
            .collect();
        assert_eq!(names, ["MSH", "EVN", "PID", "PV1"]);
        assert_eq!(shaped.parts[1].bytes, b"EVN|A01|20240101120000");
        assert_eq!(shaped.parts[3].bytes, b"PV1|1|I|W1^B2^1");
        assert_eq!(
            shaped.parts[0].media_type.as_deref(),
            Some("x-application/hl7-v2+er7")
        );
        assert_eq!(shaped.message_type.as_deref(), Some("ADT_A01"));

        let typed = Er7
            .shape(&stream(
                ADMIT,
                Some("application/hl7-v2+er7; charset=utf-8"),
            ))
            .expect("well-formed");
        assert_eq!(
            typed.parts[2].media_type.as_deref(),
            Some("application/hl7-v2+er7; charset=utf-8")
        );
    }

    #[test]
    fn msh_announces_other_separators_and_the_type_is_the_structure_else_the_code() {
        let other = b"MSH#*~\\&#A#B#C#D#20240101##ORU*R01*ORU_R01#1#P#2.5\r\nOBX#1#ST#X*Y##5\r\n";
        let shaped = Er7.shape(&stream(other, None)).expect("well-formed");
        assert_eq!(shaped.parts.len(), 2);
        assert_eq!(shaped.parts[1].name.as_deref(), Some("OBX"));
        assert_eq!(shaped.message_type.as_deref(), Some("ORU_R01"));
        assert_eq!(encoding(other), Ok(*b"#*~\\&"));
        assert_eq!(
            encoding(other).map(delimiters),
            Ok(Delimiters::new(b'\r', b'#', b'*').ended_by_line_breaks())
        );

        let by_line_feed = b"MSH|^~\\&|A|B|C|D|20240101||ACK|1|P|2.5\nMSA|AA|1\n";
        let shaped = Er7.shape(&stream(by_line_feed, None)).expect("well-formed");
        assert_eq!(shaped.parts.len(), 2);
        assert_eq!(shaped.parts[1].bytes, b"MSA|AA|1");
        assert_eq!(shaped.message_type.as_deref(), Some("ACK"));

        let mixed = b"MSH|^~\\&|A|B|C|D|20240101||ACK|1|P|2.5\rMSA|AA|1\n\nERR|1\r";
        let shaped = Er7.shape(&stream(mixed, None)).expect("well-formed");
        let names: Vec<&str> = shaped
            .parts
            .iter()
            .filter_map(|p| p.name.as_deref())
            .collect();
        assert_eq!(names, ["MSH", "MSA", "ERR"], "every line break ends one");

        let untyped = b"MSH|^~\\&|A|B|C|D|20240101|||1|P|2.5\r";
        let shaped = Er7.shape(&stream(untyped, None)).expect("well-formed");
        assert_eq!(shaped.message_type, None);
    }

    #[test]
    fn a_missing_or_short_msh_or_a_cut_segment_is_refused_where_it_fails() {
        let none = Er7.shape(&stream(b"PID|1\r", None)).expect_err("no MSH");
        assert_eq!(none.offset, Some(0));
        assert_eq!(none.to_string(), "hl7-er7: no MSH at the head at byte 0");

        let short = Er7.shape(&stream(b"MSH|^~", None)).expect_err("short");
        assert_eq!(short.offset, Some(6));
        assert_eq!(short.reason, "an MSH shorter than its encoding characters");
        let refused = |head: &[u8]| encoding(head).map_err(|e| (e.reason, e.offset));
        let at = |reason: &str, at| Err((reason.to_string(), Some(at)));
        let shorter = "an MSH shorter than its encoding characters";
        assert_eq!(refused(b"MSH"), at(shorter, 3));
        let wide = "an MSH with a field separator outside ASCII";
        assert_eq!(refused("MSH\u{e9}^~\\&\u{e9}A\r".as_bytes()), at(wide, 3));
        let outside = "an MSH with an encoding character outside ASCII";
        assert_eq!(refused("MSH|^~\\\u{e9}|A\r".as_bytes()), at(outside, 7));
        let repeated = "an MSH with an encoding character that is its field separator";
        assert_eq!(refused(b"MSH|^~|&|A\r"), at(repeated, 6));

        let cut = Er7
            .shape(&stream(
                b"MSH|^~\\&|A|B|C|D|20240101||ADT^A01|1|P|2.5\rPID|1",
                None,
            ))
            .expect_err("no terminator");
        assert_eq!(cut.offset, Some(43));
        assert_eq!(cut.reason, "a segment without its terminator");

        let spaced = Er7
            .shape(&stream(b"MSH|^~\\&|A|B\r PID|1\r", None))
            .expect_err("only line breaks stand between segments");
        assert_eq!(spaced.offset, Some(13));
        assert_eq!(spaced.reason, "a segment without a tag");
    }

    #[test]
    fn the_shape_claims_the_hl7_types_and_recognizes_msh_followed_by_a_separator() {
        assert_eq!(Er7.technology(), "hl7-er7");
        assert_eq!(
            Er7.media_types(),
            &["x-application/hl7-v2+er7", "application/hl7-v2+er7"]
        );
        assert!(Er7.recognizes(b"MSH|^~\\&|"));
        assert!(Er7.recognizes(b"MSH#*~\\&#"));
        assert!(!Er7.recognizes(b"MSH1|"));
        assert!(!Er7.recognizes(b"MSH "));
        assert!(!Er7.recognizes(b"MSH"));
        assert!(!Er7.recognizes(b"UNB+"));

        let shapes: [&dyn Shape; 1] = [&Er7];
        let by_media = message::choose(&shapes, &stream(b"x", Some("X-Application/HL7-v2+ER7")))
            .expect("held");
        assert_eq!(by_media.map(Shape::technology), Some("hl7-er7"));
        let by_look = message::choose(&shapes, &stream(ADMIT, None)).expect("held");
        assert_eq!(by_look.map(Shape::technology), Some("hl7-er7"));
        assert!(
            message::choose(&shapes, &stream(b"STX=", None))
                .expect("held")
                .is_none()
        );
    }
}
