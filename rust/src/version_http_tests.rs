//! Source-derived response-finalizer checks, not a Zig/live HTTP experiment.
use super::finalize_metadata_body;
use crate::metadata::Error;
use std::{
    cell::Cell,
    io::{self, Read},
};
struct Reader<'a> {
    bytes: &'a [u8],
    delivered: &'a Cell<usize>,
    calls: &'a Cell<usize>,
    terminal: Option<io::ErrorKind>,
}
impl Read for Reader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.bytes.is_empty() {
            return self
                .terminal
                .map_or(Ok(0), |kind| Err(io::Error::from(kind)));
        }
        // Single-byte chunks make exactly where the bounded reader stops visible.
        let count = self.bytes.len().min(buf.len()).min(1);
        buf[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        self.delivered.set(self.delivered.get() + count);
        Ok(count)
    }
}
#[test]
fn metadata_body_errors_precede_unsuccessful_status() {
    for successful in [false, true] {
        for (body, terminal, expected) in [
            (&b"abcdef"[..], None, Error::ResponseBodyTooLarge),
            (
                &b"ab"[..],
                Some(io::ErrorKind::TimedOut),
                Error::MetadataFetchTimeout,
            ),
            (
                &b"ab"[..],
                Some(io::ErrorKind::UnexpectedEof),
                Error::Network,
            ),
            (
                &b"abcd"[..],
                Some(io::ErrorKind::TimedOut),
                Error::ResponseBodyTooLarge,
            ),
        ] {
            let delivered = Cell::new(0);
            let calls = Cell::new(0);
            let reader = Reader {
                bytes: body,
                delivered: &delivered,
                calls: &calls,
                terminal,
            };
            assert_eq!(
                finalize_metadata_body(successful, None, reader, 3),
                Err(expected)
            );
            assert_eq!(delivered.get(), body.len().min(4));
            assert!(calls.get() > 0);
        }
    }
}
#[test]
fn metadata_status_applies_after_complete_bounded_body() {
    for successful in [false, true] {
        for body in [&b""[..], &b"abc"[..]] {
            for advertised in [None, Some(0), Some(3), Some(100)] {
                let delivered = Cell::new(0);
                let calls = Cell::new(0);
                let reader = Reader {
                    bytes: body,
                    delivered: &delivered,
                    calls: &calls,
                    terminal: None,
                };
                let expected = if successful {
                    Ok(body.to_vec())
                } else {
                    Err(Error::UnsuccessfulHttpStatus)
                };
                assert_eq!(
                    finalize_metadata_body(successful, advertised, reader, 3),
                    expected
                );
                assert_eq!(delivered.get(), body.len());
                assert_eq!(calls.get(), body.len() + 1);
            }
        }
    }
}
#[test]
fn metadata_sentinel_bound_prevents_extra_reads() {
    for max in [0, 1, 3, 7] {
        let delivered = Cell::new(0);
        let calls = Cell::new(0);
        let reader = Reader {
            bytes: b"0123456789abcdef",
            delivered: &delivered,
            calls: &calls,
            terminal: Some(io::ErrorKind::Other),
        };
        assert_eq!(
            finalize_metadata_body(false, Some(16), reader, max),
            Err(Error::ResponseBodyTooLarge)
        );
        assert_eq!(delivered.get(), max + 1);
        assert_eq!(calls.get(), max + 1);
    }
}
