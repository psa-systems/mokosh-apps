//! Reading a `429` from mokosh-server (PMS-832).
//!
//! The server builds every 429 through `rate_limited_response`
//! (`src/utils/error.rs`), whose body is
//!
//! ```json
//! {"error":"rate_limited","message":"...","retry_after_seconds":45}
//! ```
//!
//! That is NOT the canonical `{"error":{"code","message"}}` envelope the rest
//! of the API uses: `error` here is a string, not an object. `handle_response`
//! therefore fails to parse it and falls back to the raw body, so a 429's
//! `ApiError::Status.message` is the JSON text itself. Rendering it would show
//! a customer a line of JSON, which is why every 429 branch in the SPA writes
//! its own copy.
//!
//! The one thing worth recovering from that body is the wait, so a page can say
//! how long instead of "try again later".

/// The server's `retry_after_seconds` from a 429 body, when it is there.
///
/// Parsed permissively on purpose: this reads an error path, and a body that
/// has changed shape or been truncated by `handle_response`'s 200-character cap
/// must degrade to "no number" rather than panic or mislead. A zero or negative
/// wait is treated as absent, because "try again in 0 seconds" is worse copy
/// than no number at all.
pub fn retry_after_seconds(body: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let seconds = value.get("retry_after_seconds")?.as_i64()?;
    (seconds > 0).then_some(seconds as u64)
}

/// "in 45 seconds" / "in about 2 minutes", or `None` when the body carried no
/// usable wait.
///
/// Rounds up: telling someone to wait a minute when the bucket clears in 90
/// seconds earns a second failed attempt.
pub fn retry_after_phrase(body: &str) -> Option<String> {
    let seconds = retry_after_seconds(body)?;
    Some(if seconds < 60 {
        format!("in {seconds} seconds")
    } else {
        let minutes = seconds.div_ceil(60);
        if minutes == 1 {
            "in about a minute".to_string()
        } else {
            format!("in about {minutes} minutes")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_BODY: &str = r#"{"error":"rate_limited","message":"Too many password reset attempts, please try again later","retry_after_seconds":45}"#;

    #[test]
    fn reads_the_wait_out_of_the_servers_own_429_body() {
        assert_eq!(retry_after_seconds(REAL_BODY), Some(45));
        assert_eq!(
            retry_after_phrase(REAL_BODY).as_deref(),
            Some("in 45 seconds")
        );
    }

    #[test]
    fn a_body_that_is_not_the_rate_limit_shape_yields_nothing() {
        // The canonical error envelope, which is what every other status uses.
        assert_eq!(
            retry_after_seconds(r#"{"error":{"code":"BAD_REQUEST","message":"nope"}}"#),
            None
        );
        // Truncated by `handle_response`'s 200-character cap.
        assert_eq!(
            retry_after_seconds(r#"{"error":"rate_limited","mess"#),
            None
        );
        // An HTML error page from a proxy in front of the API.
        assert_eq!(retry_after_seconds("<html>429</html>"), None);
        assert_eq!(retry_after_seconds(""), None);
    }

    #[test]
    fn a_zero_wait_reads_as_no_number_rather_than_zero_seconds() {
        assert_eq!(
            retry_after_seconds(r#"{"retry_after_seconds":0}"#),
            None,
            "\"try again in 0 seconds\" is worse copy than no number"
        );
        assert_eq!(retry_after_seconds(r#"{"retry_after_seconds":-5}"#), None);
    }

    #[test]
    fn the_wait_rounds_up_so_the_advice_is_never_early() {
        assert_eq!(
            retry_after_phrase(r#"{"retry_after_seconds":60}"#).as_deref(),
            Some("in about a minute")
        );
        assert_eq!(
            retry_after_phrase(r#"{"retry_after_seconds":61}"#).as_deref(),
            Some("in about 2 minutes"),
            "rounding down would earn the customer a second failed attempt"
        );
        assert_eq!(
            retry_after_phrase(r#"{"retry_after_seconds":90}"#).as_deref(),
            Some("in about 2 minutes")
        );
    }
}
