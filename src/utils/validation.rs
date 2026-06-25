//! Shared field validation (PMS-516).
//!
//! One place to express "this field is required / must be an email / must be a
//! non-negative number with at most 2 decimals", so the ~30 forms across
//! `src/pages/*` stop hand-rolling the same checks with divergent messages.
//!
//! A [`Rule`] is evaluated against a field's raw string value by [`validate`],
//! which returns the FIRST failing rule's user-facing message (or `None` when
//! every rule passes). The shared field components (`components/form.rs`) take a
//! `Vec<Rule>` and call this on blur / on change to drive their inline error
//! slot; the form's explicit `error` prop still overrides it so a server
//! `field_message(...)` can be injected into the same slot.
//!
//! Messages are parameterised by the field's human label so they read the same
//! everywhere ("Company name is required.", "Email must be a valid email
//! address."). All "typed" rules SKIP a blank value - combine with [`Rule::Required`]
//! to forbid blank. That mirrors the existing forms, where an optional email or
//! number is only validated when the user actually typed something.
//!
//! This module ships the broadly-applicable rules (required, length, email,
//! uuid, number). Domain-specific validators that today live in a single page
//! (phone E.164, postal, ISO country, H:MM duration) are migrated onto a shared
//! rule as each form adopts the system (PMS-518); until then use [`Rule::Custom`].

/// A validation rule evaluated against a field's raw (untrimmed) string value.
///
/// `Custom` carries a function pointer, so `PartialEq` is hand-written (below)
/// rather than derived: the derive would emit a raw `fn`-pointer `==`, which
/// `unpredictable_function_pointer_comparisons` denies. `PartialEq` is required
/// because a `Vec<Rule>` lives in a Dioxus `Props` struct.
#[derive(Clone)]
pub enum Rule {
    /// Non-blank after trimming.
    Required,
    /// At most `n` characters (counted as Unicode scalar values).
    MaxLen(usize),
    /// At least `n` characters. Skipped when the value is blank.
    MinLen(usize),
    /// A plausible email address (one `@`, non-empty local part, a dotted
    /// host). Skipped when blank. The server stays the source of truth.
    Email,
    /// Parses as a UUID (used by the hidden id behind a picker). Skipped when blank.
    Uuid,
    /// A number, optionally bounded and capped to `max_decimals` decimal places.
    /// Skipped when blank.
    Number {
        min: Option<f64>,
        max: Option<f64>,
        max_decimals: Option<u32>,
    },
    /// Escape hatch: a predicate returning `Some(message)` when invalid, `None`
    /// when valid. Receives the trimmed value. Use for rules not yet promoted to
    /// a first-class variant.
    Custom(fn(&str) -> Option<String>),
}

impl PartialEq for Rule {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Rule::Required, Rule::Required)
            | (Rule::Email, Rule::Email)
            | (Rule::Uuid, Rule::Uuid) => true,
            (Rule::MaxLen(a), Rule::MaxLen(b)) | (Rule::MinLen(a), Rule::MinLen(b)) => a == b,
            (
                Rule::Number {
                    min: amin,
                    max: amax,
                    max_decimals: ad,
                },
                Rule::Number {
                    min: bmin,
                    max: bmax,
                    max_decimals: bd,
                },
            ) => amin == bmin && amax == bmax && ad == bd,
            // Compare custom predicates by function address (the lint-blessed
            // way); two props carrying the same predicate stay PartialEq-equal
            // so Dioxus memoization still works.
            (Rule::Custom(a), Rule::Custom(b)) => std::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
}

/// The label used in a message, falling back to a neutral phrase when a field
/// has no visible label.
fn field_label(label: &str) -> &str {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        "This field"
    } else {
        trimmed
    }
}

/// Count the decimal places in a numeric string (digits after a single `.`).
/// Returns 0 when there is no fractional part.
fn decimal_places(value: &str) -> u32 {
    match value.split_once('.') {
        Some((_, frac)) => frac.chars().filter(|c| c.is_ascii_digit()).count() as u32,
        None => 0,
    }
}

/// Validate `value` against `rules`, returning the first failing rule's message.
/// `label` is the field's human-readable name, woven into the message.
pub fn validate(value: &str, label: &str, rules: &[Rule]) -> Option<String> {
    let trimmed = value.trim();
    let name = field_label(label);
    for rule in rules {
        let failure = check(rule, value, trimmed, name);
        if failure.is_some() {
            return failure;
        }
    }
    None
}

/// Whether `value` passes every rule.
pub fn is_valid(value: &str, rules: &[Rule]) -> bool {
    validate(value, "", rules).is_none()
}

fn check(rule: &Rule, raw: &str, trimmed: &str, name: &str) -> Option<String> {
    match rule {
        Rule::Required => {
            if trimmed.is_empty() {
                Some(format!("{name} is required."))
            } else {
                None
            }
        }
        Rule::MaxLen(n) => {
            // Count the raw value so trailing spaces still count against a cap.
            if raw.chars().count() > *n {
                Some(format!("{name} must be {n} characters or fewer."))
            } else {
                None
            }
        }
        Rule::MinLen(n) => {
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() < *n {
                Some(format!("{name} must be at least {n} characters."))
            } else {
                None
            }
        }
        Rule::Email => {
            if trimmed.is_empty() {
                return None;
            }
            if is_email(trimmed) {
                None
            } else {
                Some(format!("{name} must be a valid email address."))
            }
        }
        Rule::Uuid => {
            if trimmed.is_empty() {
                return None;
            }
            if uuid::Uuid::parse_str(trimmed).is_ok() {
                None
            } else {
                Some(format!("{name} must be a valid selection."))
            }
        }
        Rule::Number {
            min,
            max,
            max_decimals,
        } => {
            if trimmed.is_empty() {
                return None;
            }
            let Ok(parsed) = trimmed.parse::<f64>() else {
                return Some(format!("{name} must be a number."));
            };
            if !parsed.is_finite() {
                return Some(format!("{name} must be a number."));
            }
            if let Some(min) = min {
                if parsed < *min {
                    return Some(if *min == 0.0 {
                        format!("{name} must not be negative.")
                    } else {
                        format!("{name} must be at least {min}.")
                    });
                }
            }
            if let Some(max) = max {
                if parsed > *max {
                    return Some(format!("{name} is out of range."));
                }
            }
            if let Some(dp) = max_decimals {
                if decimal_places(trimmed) > *dp {
                    let unit = if *dp == 1 { "place" } else { "places" };
                    return Some(format!("{name} must have at most {dp} decimal {unit}."));
                }
            }
            None
        }
        Rule::Custom(f) => f(trimmed),
    }
}

/// A deliberately small email check: exactly one `@`, a non-empty local part, a
/// host with at least one dot and no whitespace. The server does the
/// authoritative validation; this only stops obvious mistakes before the POST.
fn is_email(value: &str) -> bool {
    let Some((local, host)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || host.is_empty() {
        return false;
    }
    if value.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if host.contains('@') {
        return false;
    }
    // A dotted host with non-empty labels on both sides of the last dot.
    match host.rsplit_once('.') {
        Some((sub, tld)) => !sub.is_empty() && !tld.is_empty(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_rejects_blank_and_whitespace() {
        assert_eq!(
            validate("", "Name", &[Rule::Required]).as_deref(),
            Some("Name is required.")
        );
        assert_eq!(
            validate("   ", "Name", &[Rule::Required]).as_deref(),
            Some("Name is required.")
        );
        assert_eq!(validate("Acme", "Name", &[Rule::Required]), None);
    }

    #[test]
    fn blank_label_falls_back() {
        assert_eq!(
            validate("", "", &[Rule::Required]).as_deref(),
            Some("This field is required.")
        );
    }

    #[test]
    fn first_failing_rule_wins() {
        // Required is listed first, so a blank value reports "required",
        // not the email message.
        assert_eq!(
            validate("", "Email", &[Rule::Required, Rule::Email]).as_deref(),
            Some("Email is required.")
        );
    }

    #[test]
    fn typed_rules_skip_blank() {
        // Optional email/number: blank passes (combine with Required to forbid).
        assert_eq!(validate("", "Email", &[Rule::Email]), None);
        assert_eq!(
            validate(
                "",
                "Amount",
                &[Rule::Number {
                    min: None,
                    max: None,
                    max_decimals: None
                }]
            ),
            None
        );
    }

    #[test]
    fn maxlen_counts_raw_value() {
        assert_eq!(
            validate("abcdef", "Code", &[Rule::MaxLen(5)]).as_deref(),
            Some("Code must be 5 characters or fewer.")
        );
        assert_eq!(validate("abcde", "Code", &[Rule::MaxLen(5)]), None);
    }

    #[test]
    fn minlen_skips_blank_but_enforces_nonblank() {
        assert_eq!(validate("", "PIN", &[Rule::MinLen(4)]), None);
        assert_eq!(
            validate("12", "PIN", &[Rule::MinLen(4)]).as_deref(),
            Some("PIN must be at least 4 characters.")
        );
    }

    #[test]
    fn email_rule() {
        assert_eq!(validate("a@b.co", "Email", &[Rule::Email]), None);
        for bad in ["a@b", "ab.co", "a@@b.co", "a b@c.co", "@b.co", "a@.co"] {
            assert!(
                validate(bad, "Email", &[Rule::Email]).is_some(),
                "expected {bad} to fail"
            );
        }
    }

    #[test]
    fn uuid_rule() {
        assert_eq!(
            validate(
                "00000000-0000-0000-0000-000000000000",
                "Company",
                &[Rule::Uuid]
            ),
            None
        );
        assert_eq!(
            validate("not-a-uuid", "Company", &[Rule::Uuid]).as_deref(),
            Some("Company must be a valid selection.")
        );
    }

    #[test]
    fn number_rule_range_and_decimals() {
        let money = Rule::Number {
            min: Some(0.0),
            max: Some(1000.0),
            max_decimals: Some(2),
        };
        assert_eq!(
            validate("12.50", "Amount", std::slice::from_ref(&money)),
            None
        );
        assert_eq!(
            validate("-1", "Amount", std::slice::from_ref(&money)).as_deref(),
            Some("Amount must not be negative.")
        );
        assert_eq!(
            validate("5000", "Amount", std::slice::from_ref(&money)).as_deref(),
            Some("Amount is out of range.")
        );
        assert_eq!(
            validate("1.234", "Amount", std::slice::from_ref(&money)).as_deref(),
            Some("Amount must have at most 2 decimal places.")
        );
        assert_eq!(
            validate("abc", "Amount", std::slice::from_ref(&money)).as_deref(),
            Some("Amount must be a number.")
        );
    }

    #[test]
    fn number_min_nonzero_message() {
        let rule = Rule::Number {
            min: Some(1.0),
            max: None,
            max_decimals: None,
        };
        assert_eq!(
            validate("0", "Qty", std::slice::from_ref(&rule)).as_deref(),
            Some("Qty must be at least 1.")
        );
    }

    #[test]
    fn custom_rule() {
        fn no_spaces(v: &str) -> Option<String> {
            if v.contains(' ') {
                Some("Slug must not contain spaces.".to_string())
            } else {
                None
            }
        }
        let rule = Rule::Custom(no_spaces);
        assert_eq!(validate("good", "Slug", std::slice::from_ref(&rule)), None);
        assert_eq!(
            validate("a b", "Slug", std::slice::from_ref(&rule)).as_deref(),
            Some("Slug must not contain spaces.")
        );
    }

    #[test]
    fn is_valid_helper() {
        assert!(is_valid("a@b.co", &[Rule::Required, Rule::Email]));
        assert!(!is_valid("", &[Rule::Required, Rule::Email]));
    }
}
