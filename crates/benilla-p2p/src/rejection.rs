//! Local-only rejection diagnostics. Never format an error's arbitrary inner text.
use crate::Error;
use std::io::ErrorKind;

impl Error {
    pub(crate) fn rejection_reason(&self) -> &'static str {
        match self {
            Self::Crypto => "peer authentication failed",
            Self::Timeout => "peer operation timed out",
            Self::Io(error) => match error.kind() {
                ErrorKind::ConnectionRefused => "peer connection refused",
                ErrorKind::ConnectionReset => "peer connection reset",
                ErrorKind::ConnectionAborted => "peer connection aborted",
                ErrorKind::UnexpectedEof => "peer closed connection during handshake",
                ErrorKind::TimedOut => "peer operation timed out",
                ErrorKind::PermissionDenied => "peer connection permission denied",
                _ => "peer I/O failure",
            },
            // Explicit allowlist: even Protocol's static string may contain private material
            // supplied by a caller. New errors remain generic until reviewed for logging.
            Self::Protocol(reason) => match *reason {
                "unsupported protocol version" => "unsupported protocol version",
                "incompatible network, ruleset, or content" => {
                    "incompatible network, ruleset, or content"
                }
                "duplicate local peer identity" => "duplicate local peer identity",
                "peer ID does not match owner public key" => {
                    "peer ID does not match owner public key"
                }
                "internet handshake failed" => "internet handshake failed",
                "internet connection failed" => "internet connection failed",
                "internet stream failed" => "internet stream failed",
                "invalid hello" | "invalid remote hello" | "invalid owner hello" => {
                    "invalid peer hello"
                }
                "invalid frame length" | "invalid owner authentication frame length" => {
                    "invalid peer handshake frame length"
                }
                _ => "peer protocol failure",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_reasons_distinguish_common_setup_failures() {
        for (error, expected) in [
            (Error::Crypto, "peer authentication failed"),
            (Error::Timeout, "peer operation timed out"),
            (
                Error::Protocol("duplicate local peer identity"),
                "duplicate local peer identity",
            ),
            (
                Error::Protocol("incompatible network, ruleset, or content"),
                "incompatible network, ruleset, or content",
            ),
            (
                Error::Protocol("unsupported protocol version"),
                "unsupported protocol version",
            ),
            (
                Error::Protocol("peer ID does not match owner public key"),
                "peer ID does not match owner public key",
            ),
        ] {
            assert_eq!(error.rejection_reason(), expected);
        }
    }

    #[test]
    fn io_diagnostics_use_kind_without_inner_text() {
        for (kind, expected) in [
            (ErrorKind::ConnectionRefused, "peer connection refused"),
            (ErrorKind::ConnectionReset, "peer connection reset"),
            (ErrorKind::ConnectionAborted, "peer connection aborted"),
            (
                ErrorKind::UnexpectedEof,
                "peer closed connection during handshake",
            ),
            (ErrorKind::TimedOut, "peer operation timed out"),
            (
                ErrorKind::PermissionDenied,
                "peer connection permission denied",
            ),
            (ErrorKind::Other, "peer I/O failure"),
        ] {
            let error = Error::Io(std::io::Error::new(
                kind,
                "private-key/PSK/signature/payload\nforged log entry".repeat(4096),
            ));
            assert_eq!(error.rejection_reason(), expected);
        }
    }

    #[test]
    fn protocol_diagnostics_are_allowlisted_not_truncated_or_prefix_matched() {
        for reason in [
            "private-key/PSK/signature/payload\nforged log entry",
            "peer authentication failed: secret",
            "incompatible network, ruleset, or content\nsecret",
            "an unreviewed future error",
        ] {
            assert_eq!(
                Error::Protocol(reason).rejection_reason(),
                "peer protocol failure"
            );
        }
        for reason in [
            "invalid hello",
            "invalid remote hello",
            "invalid owner hello",
        ] {
            assert_eq!(
                Error::Protocol(reason).rejection_reason(),
                "invalid peer hello"
            );
        }
    }
}
