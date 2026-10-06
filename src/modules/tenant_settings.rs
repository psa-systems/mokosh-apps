//! Reading one tenant setting through its category (`GET /settings/{category}`).
//!
//! The per-key route answers a key the tenant never set with a 404, and every
//! browser logs that 404 as a console error on a page that worked (MAPPS-1001).
//! The category route answers 200 with the rows that exist, so an unset key is
//! simply absent. Writes still go to the per-key route.

/// One row of `GET /settings/{category}`; only `key` and `value` are read.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct SettingRow {
    pub key: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

/// The stored value of `key`, or `None` when the tenant has not set it.
pub fn value_of(rows: Vec<SettingRow>, key: &str) -> Option<serde_json::Value> {
    rows.into_iter().find(|r| r.key == key).map(|r| r.value)
}

/// Read `category/key`: `Ok(None)` when unset, `Err` only when the read failed.
#[cfg(feature = "app")]
pub async fn get(
    category: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, crate::hooks::fetch::api::ApiError> {
    let rows = crate::hooks::fetch::api::get_authed_typed::<Vec<SettingRow>>(&format!(
        "/settings/{category}"
    ))
    .await?;
    Ok(value_of(rows, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's `TenantSettingResponse` list decodes, extra fields ignored.
    #[test]
    fn the_category_list_decodes_and_finds_its_key() {
        let body = r#"[
            {"id":"6f1c0d4e-0000-4000-8000-000000000001","category":"time_tracking","key":"max_hours_per_day","value":10},
            {"id":"6f1c0d4e-0000-4000-8000-000000000002","category":"time_tracking","key":"other","value":"x"}
        ]"#;
        let rows: Vec<SettingRow> = serde_json::from_str(body).expect("decodes");
        assert_eq!(
            value_of(rows, "max_hours_per_day"),
            Some(serde_json::json!(10))
        );
    }

    #[test]
    fn an_unset_key_is_none_not_an_error() {
        let rows: Vec<SettingRow> = serde_json::from_str("[]").expect("decodes");
        assert_eq!(value_of(rows, "max_hours_per_day"), None);
    }
}
