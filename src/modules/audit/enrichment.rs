//! Advisory ASN / VPN enrichment for one address (PMS-870).
//!
//! Mirrors mokosh-server's `IpEnrichmentResponse`
//! (`src/modules/ip_enrich/routes.rs`), served by `GET /api/v1/ip-enrichment?ip=`
//! behind `RequireAdmin`. It lives beside the audit DTOs because the audit
//! log's actor IP is its only consumer; the server's own module doc names that
//! surface ("context for a human reviewing an IP (e.g. an actor IP in the audit
//! log), never an automatic abuse verdict").
//!
//! BUNYIP-437 is the constraint on how it may be used: a VPN must not
//! auto-classify a request. Nothing here turns a field into a verdict, and
//! [`enrichment_rows`] deliberately exposes no "suspicious" or "blocked"
//! reading of `is_anonymizing` - it is one row of context among the others.
//!
//! The response body is `null` when there is nothing to report (no dataset
//! configured, a private or reserved address, or an address the dataset does
//! not know), which is why the caller decodes `Option<IpEnrichment>` and treats
//! `None` as an answer rather than a failure.

use serde::Deserialize;

/// The advisory enrichment of one address.
///
/// Every field except `category`, `vpn` and `is_anonymizing` is optional on the
/// wire, and all of them carry `#[serde(default)]` so a server that stops
/// sending one decodes to `None` rather than failing the whole lookup.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct IpEnrichment {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub asn: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub isp: Option<String>,
    /// Stable lowercase label of the server's `NetworkCategory`.
    #[serde(default)]
    pub category: String,
    /// Stable lowercase label of the server's `VpnLikelihood`.
    #[serde(default)]
    pub vpn: String,
    /// The one-bit "looks like a VPN / proxy" summary. Rendered as context,
    /// never as a verdict (BUNYIP-437).
    #[serde(default)]
    pub is_anonymizing: bool,
    #[serde(default)]
    pub proxy_type: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub threat: Option<String>,
    /// Always `true` from the server: a reminder in the payload that this is
    /// context, not a verdict. The UI says so in words instead of rendering
    /// this as a raw boolean, so nothing reads it.
    #[serde(default)]
    pub advisory: bool,
}

/// Turn a server label (`"data-center"`, `"residential"`) into display text.
///
/// The server's labels are a stable wire vocabulary, not copy, so they are
/// formatted rather than mapped: a value this build has never seen still
/// renders as words instead of falling through to "Unknown" and hiding what the
/// server actually said.
pub fn humanize_label(raw: &str) -> String {
    let spaced = raw.trim().replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// The label/value pairs to render, in reading order, skipping what the server
/// did not send.
///
/// `category` and `vpn` are always present because the server sends a label for
/// every enum value; the rest are `Option` on the wire and an absent one is
/// omitted rather than rendered as an empty row or a "-", which would read as
/// "the dataset says nothing here" when it means "this build asked and got no
/// value".
pub fn enrichment_rows(e: &IpEnrichment) -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = Vec::new();

    if let Some(asn) = e.asn.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        rows.push(("ASN", format!("AS{asn}")));
    }
    // Organization and ISP are the same kind of answer from two dataset
    // columns. Both are shown when they disagree, because which one is
    // populated varies by address and a reviewer chasing an actor wants
    // whichever name the dataset actually has.
    let org = e
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let isp = e.isp.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(org) = org {
        rows.push(("Organization", org.to_string()));
    }
    if let Some(isp) = isp.filter(|isp| Some(*isp) != org) {
        rows.push(("ISP", isp.to_string()));
    }

    if !e.category.trim().is_empty() {
        rows.push(("Network", humanize_label(&e.category)));
    }
    if !e.vpn.trim().is_empty() {
        rows.push(("VPN likelihood", humanize_label(&e.vpn)));
    }
    if let Some(proxy) = e
        .proxy_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        rows.push(("Proxy type", humanize_label(proxy)));
    }
    if let Some(provider) = e
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        rows.push(("Provider", provider.to_string()));
    }
    if let Some(threat) = e.threat.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        rows.push(("Threat", humanize_label(threat)));
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> IpEnrichment {
        IpEnrichment {
            ip: "203.0.113.7".into(),
            asn: Some("15169".into()),
            organization: Some("Google LLC".into()),
            isp: Some("Google LLC".into()),
            category: "hosting".into(),
            vpn: "data-center".into(),
            is_anonymizing: true,
            proxy_type: Some("DCH".into()),
            provider: Some("NordVPN".into()),
            threat: None,
            advisory: true,
        }
    }

    #[test]
    fn a_label_the_build_has_never_seen_still_renders_as_words() {
        assert_eq!(humanize_label("data-center"), "Data center");
        assert_eq!(humanize_label("residential"), "Residential");
        // The point of formatting rather than matching: a value added to the
        // server's enum after this build shipped is shown, not swallowed.
        assert_eq!(humanize_label("satellite_uplink"), "Satellite uplink");
        assert_eq!(humanize_label(""), "");
    }

    #[test]
    fn every_acceptance_field_is_rendered() {
        let rows = enrichment_rows(&full());
        let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
        for required in ["ASN", "Organization", "Network", "VPN likelihood"] {
            assert!(
                labels.contains(&required),
                "PMS-870 requires {required} to be shown, got {labels:?}"
            );
        }
        assert_eq!(
            rows.iter()
                .find(|(l, _)| *l == "ASN")
                .map(|(_, v)| v.clone()),
            Some("AS15169".to_string()),
            "the ASN reads as an AS number rather than a bare integer"
        );
    }

    #[test]
    fn an_isp_identical_to_the_organization_is_not_repeated() {
        let rows = enrichment_rows(&full());
        assert!(
            !rows.iter().any(|(l, _)| *l == "ISP"),
            "the dataset returns the same name in both columns for most addresses; \
             showing it twice reads as two independent confirmations, got {rows:?}"
        );

        let mut differing = full();
        differing.isp = Some("Google Fiber".into());
        let rows = enrichment_rows(&differing);
        assert!(
            rows.iter().any(|(l, v)| *l == "ISP" && v == "Google Fiber"),
            "an ISP that disagrees with the organization is its own answer, got {rows:?}"
        );
    }

    #[test]
    fn absent_fields_are_omitted_rather_than_rendered_empty() {
        let sparse = IpEnrichment {
            ip: "198.51.100.4".into(),
            asn: None,
            organization: None,
            isp: None,
            category: "residential".into(),
            vpn: "unlikely".into(),
            is_anonymizing: false,
            proxy_type: None,
            provider: None,
            threat: Some("   ".into()),
            advisory: true,
        };
        let rows = enrichment_rows(&sparse);
        let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec!["Network", "VPN likelihood"],
            "an absent field is left out, not rendered as an empty row that reads \
             as the dataset having nothing to say"
        );
        assert!(
            rows.iter().all(|(_, v)| !v.trim().is_empty()),
            "no rendered value is blank, got {rows:?}"
        );
    }

    #[test]
    fn nothing_derives_a_verdict_from_the_signal() {
        // BUNYIP-437: `is_anonymizing` is the one field that could be read as a
        // judgement. It contributes no row, no wording and no severity here;
        // the VPN likelihood is shown as the label the server sent and the
        // reader draws their own conclusion.
        let mut anonymizing = full();
        anonymizing.is_anonymizing = true;
        let mut plain = full();
        plain.is_anonymizing = false;

        assert_eq!(
            enrichment_rows(&anonymizing),
            enrichment_rows(&plain),
            "flipping is_anonymizing must not change what is rendered"
        );

        let rendered = enrichment_rows(&anonymizing)
            .iter()
            .map(|(l, v)| format!("{l} {v}"))
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for verdict in ["suspicious", "blocked", "malicious", "denied", "risk"] {
            assert!(
                !rendered.contains(verdict),
                "the enrichment must read as context, not a verdict, but rendered {verdict:?}"
            );
        }
    }

    #[test]
    fn a_null_body_decodes_as_nothing_to_report() {
        // The server answers 200 with a JSON `null` when it has nothing for the
        // address. That is an answer, not a failure, and the caller's
        // `Option<IpEnrichment>` is what makes the difference legible.
        let decoded: Option<IpEnrichment> =
            serde_json::from_str("null").expect("a null body decodes");
        assert!(decoded.is_none());
    }

    #[test]
    fn a_response_missing_optional_keys_still_decodes() {
        let decoded: Option<IpEnrichment> = serde_json::from_str(
            r#"{"ip":"203.0.113.7","category":"hosting","vpn":"vpn","is_anonymizing":true,"advisory":true}"#,
        )
        .expect("a response with only the always-present keys decodes");
        let decoded = decoded.expect("a JSON object is an enrichment");
        assert_eq!(decoded.asn, None);
        assert_eq!(decoded.category, "hosting");
    }
}
