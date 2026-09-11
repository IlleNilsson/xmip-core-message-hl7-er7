#![forbid(unsafe_code)]

//! HL7 version 2 in ER7, the vertical-bar encoding: a message as its
//! segments, one part each, named by the tag — `MSH`, `EVN`, `PID`, `PV1`,
//! `OBX` — in the order they lie. The delimiters are the ones `MSH`
//! announces: the field separator is its fourth character and the component
//! separator the first of the encoding characters that follow. A segment
//! ends at a carriage return, as the standard says; a message with no
//! carriage return at all is read with the line feed instead, because that
//! is how such messages arrive. The announced type is MSH-9: the message
//! structure when the third component gives it, else the message code and
//! trigger event joined by an underscore — `ADT_A01`, `ORU_R01` — else the
//! code alone (ADR-0047).
//!
//! The walk is the Foundation's `message::segment`: this shape reads the
//! head and brings the delimiters. A contract checks everything else.

use message::segment::{self, Delimiters};
use message::{Shape, ShapeError, Shaped};
use stream::Stream;

/// The HL7 v2 ER7 shape.
#[derive(Clone, Copy, Debug, Default)]
pub struct Er7;

/// The delimiters `MSH` announces: its field separator, its component
/// separator, and the carriage return — or the line feed where there is no
/// carriage return.
///
/// # Errors
/// No `MSH` at the head, or one shorter than its encoding characters.
pub fn delimiters(bytes: &[u8]) -> Result<Delimiters, ShapeError> {
    if !bytes.starts_with(b"MSH") {
        return Err(ShapeError::new("hl7-er7", "no MSH at the head").at(0));
    }
    let Some(head) = bytes.get(3..8) else {
        return Err(
            ShapeError::new("hl7-er7", "an MSH shorter than its encoding characters")
                .at(bytes.len()),
        );
    };
    let terminator = if bytes.contains(&b'\r') { b'\r' } else { b'\n' };
    Ok(Delimiters::new(terminator, head[0], head[1]))
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

    fn recognises(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"MSH")
            && bytes
                .get(3)
                .is_some_and(|b| !b.is_ascii_alphanumeric() && !b.is_ascii_whitespace())
    }

    fn shape(&self, stream: &Stream) -> Result<Shaped, ShapeError> {
        let delimiters = delimiters(stream.bytes())?;
        let segments = segment::segments(stream.bytes(), &delimiters)
            .map_err(|stop| stop.refused("hl7-er7"))?;
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
        assert_eq!(delimiters(other), Ok(Delimiters::new(b'\r', b'#', b'*')));

        let by_line_feed = b"MSH|^~\\&|A|B|C|D|20240101||ACK|1|P|2.5\nMSA|AA|1\n";
        let shaped = Er7.shape(&stream(by_line_feed, None)).expect("well-formed");
        assert_eq!(shaped.parts.len(), 2);
        assert_eq!(shaped.parts[1].bytes, b"MSA|AA|1");
        assert_eq!(shaped.message_type.as_deref(), Some("ACK"));

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

        let cut = Er7
            .shape(&stream(
                b"MSH|^~\\&|A|B|C|D|20240101||ADT^A01|1|P|2.5\rPID|1",
                None,
            ))
            .expect_err("no terminator");
        assert_eq!(cut.offset, Some(43));
        assert_eq!(cut.reason, "a segment without its terminator");
    }

    #[test]
    fn the_shape_claims_the_hl7_types_and_recognises_msh_followed_by_a_separator() {
        assert_eq!(Er7.technology(), "hl7-er7");
        assert_eq!(
            Er7.media_types(),
            &["x-application/hl7-v2+er7", "application/hl7-v2+er7"]
        );
        assert!(Er7.recognises(b"MSH|^~\\&|"));
        assert!(Er7.recognises(b"MSH#*~\\&#"));
        assert!(!Er7.recognises(b"MSH1|"));
        assert!(!Er7.recognises(b"MSH "));
        assert!(!Er7.recognises(b"MSH"));
        assert!(!Er7.recognises(b"UNB+"));

        let shapes: [&dyn Shape; 1] = [&Er7];
        let by_media = message::choose(&shapes, &stream(b"x", Some("X-Application/HL7-v2+ER7")));
        assert_eq!(by_media.map(Shape::technology), Some("hl7-er7"));
        let by_look = message::choose(&shapes, &stream(ADMIT, None));
        assert_eq!(by_look.map(Shape::technology), Some("hl7-er7"));
        assert!(message::choose(&shapes, &stream(b"STX=", None)).is_none());
    }
}
