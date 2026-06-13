//! Contracts DTOs.
//!
//! Client-side mirror of mokosh-server's `modules/contracts/models.rs`.
//! Response types derive `Deserialize` (decoded from the API) and the
//! `Clone, Debug, PartialEq` Dioxus Props require; request types add
//! `Serialize`. Optional fields carry `#[serde(default)]` so a response
//! that omits a column still decodes. `Decimal` fields are bare (no
//! `serde(with)`) to match the server, which under the `rust_decimal`
//! `serde` feature serializes `rust_decimal` as a JSON *string* (not a
//! number). The client enables the same `serde` feature, so a string
//! decodes losslessly into `Decimal` (MAPPS-138).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// CONTRACT
// ============================================================================

/// Contract response. Mirrors `ContractResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ContractResponse {
    pub id: Uuid,
    #[serde(default)]
    pub contract_number: Option<String>,
    pub name: String,
    pub company_id: Uuid,
    #[serde(default)]
    pub contract_type: String,
    #[serde(default)]
    pub status: String,
    pub start_date: NaiveDate,
    #[serde(default)]
    pub end_date: Option<NaiveDate>,
    #[serde(default)]
    pub auto_renew: bool,
    #[serde(default)]
    pub billing_cycle: String,
    #[serde(default)]
    pub billing_amount: Option<Decimal>,
    #[serde(default)]
    pub sla_id: Option<Uuid>,
    #[serde(default)]
    pub signed_date: Option<NaiveDate>,
    #[serde(default)]
    pub signed_by_contact_id: Option<Uuid>,
    #[serde(default)]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create-contract request. Mirrors `CreateContractRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateContractRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_number: Option<String>,
    pub name: String,
    pub company_id: Uuid,
    pub contract_type: String,
    pub status: String,
    pub start_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<NaiveDate>,
    pub auto_renew: bool,
    pub billing_cycle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_amount: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sla_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_by_contact_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Update-contract request. Mirrors `UpdateContractRequest`. Every field
/// is optional; only the keys the user actually changed are sent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UpdateContractRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_renew: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_cycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_amount: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sla_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_by_contact_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// ============================================================================
// CONTRACT ITEM
// ============================================================================

/// Contract line-item response. Mirrors `ContractItemResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ContractItemResponse {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub item_type: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub total_price: Decimal,
    #[serde(default)]
    pub billing_frequency: String,
    #[serde(default)]
    pub work_type_id: Option<Uuid>,
    #[serde(default)]
    pub included_hours: Option<Decimal>,
    #[serde(default)]
    pub overage_rate: Option<Decimal>,
    #[serde(default)]
    pub rollover_enabled: bool,
    #[serde(default)]
    pub max_rollover_hours: Option<Decimal>,
    #[serde(default)]
    pub sort_order: i32,
}

/// Upsert contract line-item request. Mirrors `UpsertContractItemRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpsertContractItemRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub item_type: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub billing_frequency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_type_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub included_hours: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_rate: Option<Decimal>,
    pub rollover_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rollover_hours: Option<Decimal>,
    pub sort_order: i32,
}

// ============================================================================
// HOUR BALANCE
// ============================================================================

/// Hour-balance period response. Mirrors `ContractHourBalanceResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ContractHourBalanceResponse {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub contract_item_id: Uuid,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub hours_included: Decimal,
    pub hours_used: Decimal,
    pub hours_remaining: Decimal,
    pub rollover_hours: Decimal,
}

// ============================================================================
// RATE CARD
// ============================================================================

/// Rate-card response. Mirrors `RateCardResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RateCardResponse {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

/// Upsert rate-card request. Mirrors `UpsertRateCardRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpsertRateCardRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_default: bool,
}

/// Rate-card line-item response. Mirrors `RateCardItemResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RateCardItemResponse {
    pub id: Uuid,
    pub rate_card_id: Uuid,
    pub work_type_id: Uuid,
    pub hourly_rate: Decimal,
    #[serde(default)]
    pub after_hours_rate: Option<Decimal>,
    #[serde(default)]
    pub emergency_rate: Option<Decimal>,
}

/// Upsert rate-card line-item request. Mirrors `UpsertRateCardItemRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpsertRateCardItemRequest {
    pub work_type_id: Uuid,
    pub hourly_rate: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_hours_rate: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emergency_rate: Option<Decimal>,
}
