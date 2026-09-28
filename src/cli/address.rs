//! The v2 address shape.
//!
//! v2 spells these `addressLine1`, `stateCode` and `countryCode`. v1's
//! `--billing-country-id` and `--billing-state-id` carried *ids*; v2 takes
//! codes, so the flags are renamed rather than transplanted.
//!
//! Every argument carries an explicit `id`. clap derives an argument's id from
//! its **field name**, not its long name, so two flattened structs that both
//! declare `line1` collide at runtime with "argument names must be unique" —
//! a panic in the built binary that no compile check catches.

use serde_json::{Map, Value};

#[derive(clap::Args, Debug, Default, Clone)]
pub struct BillingArgs {
    /// AVS billing street line 1.
    #[arg(long = "billing-line1", id = "billing_line1", value_name = "LINE1")]
    pub line1: Option<String>,
    /// AVS billing street line 2.
    #[arg(long = "billing-line2", id = "billing_line2", value_name = "LINE2")]
    pub line2: Option<String>,
    /// AVS billing city.
    #[arg(long = "billing-city", id = "billing_city", value_name = "CITY")]
    pub city: Option<String>,
    /// AVS billing state code (e.g. `CO`).
    #[arg(long = "billing-state", id = "billing_state", value_name = "STATE")]
    pub state: Option<String>,
    /// AVS billing postal / ZIP code.
    #[arg(
        long = "billing-postal-code",
        id = "billing_postal_code",
        value_name = "POSTAL_CODE"
    )]
    pub postal_code: Option<String>,
    /// AVS billing country code (e.g. `US`).
    #[arg(
        long = "billing-country",
        id = "billing_country",
        value_name = "COUNTRY"
    )]
    pub country: Option<String>,
}

#[derive(clap::Args, Debug, Default, Clone)]
pub struct ShippingArgs {
    /// Shipping street line 1.
    #[arg(long = "shipping-line1", id = "shipping_line1", value_name = "LINE1")]
    pub line1: Option<String>,
    /// Shipping street line 2.
    #[arg(long = "shipping-line2", id = "shipping_line2", value_name = "LINE2")]
    pub line2: Option<String>,
    /// Shipping city.
    #[arg(long = "shipping-city", id = "shipping_city", value_name = "CITY")]
    pub city: Option<String>,
    /// Shipping state code (e.g. `CO`).
    #[arg(long = "shipping-state", id = "shipping_state", value_name = "STATE")]
    pub state: Option<String>,
    /// Shipping postal / ZIP code.
    #[arg(
        long = "shipping-postal-code",
        id = "shipping_postal_code",
        value_name = "POSTAL_CODE"
    )]
    pub postal_code: Option<String>,
    /// Shipping country code (e.g. `US`).
    #[arg(
        long = "shipping-country",
        id = "shipping_country",
        value_name = "COUNTRY"
    )]
    pub country: Option<String>,
}

impl BillingArgs {
    pub fn to_address(&self) -> Option<Value> {
        address(
            &self.line1,
            &self.line2,
            &self.city,
            &self.state,
            &self.postal_code,
            &self.country,
        )
    }
}

impl ShippingArgs {
    pub fn to_address(&self) -> Option<Value> {
        address(
            &self.line1,
            &self.line2,
            &self.city,
            &self.state,
            &self.postal_code,
            &self.country,
        )
    }
}

/// `None` when no component was supplied, so an absent address is an absent
/// key rather than an empty object the API would have to interpret.
fn address(
    line1: &Option<String>,
    line2: &Option<String>,
    city: &Option<String>,
    state: &Option<String>,
    postal_code: &Option<String>,
    country: &Option<String>,
) -> Option<Value> {
    let mut map = Map::new();
    let mut put = |key: &str, value: &Option<String>| {
        if let Some(v) = value.as_ref().filter(|s| !s.is_empty()) {
            map.insert(key.to_string(), Value::String(v.clone()));
        }
    };
    put("addressLine1", line1);
    put("addressLine2", line2);
    put("city", city);
    put("stateCode", state);
    put("postalCode", postal_code);
    put("countryCode", country);
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v2's key names, not v1's. A wrong key here is silently dropped by an
    /// `additionalProperties: false` schema.
    #[test]
    fn uses_the_v2_address_key_names() {
        let b = BillingArgs {
            line1: Some("1 Main St".into()),
            city: Some("Austin".into()),
            state: Some("TX".into()),
            postal_code: Some("78701".into()),
            country: Some("US".into()),
            line2: None,
        };
        let a = b.to_address().unwrap();
        assert_eq!(a["addressLine1"], "1 Main St");
        assert_eq!(a["stateCode"], "TX");
        assert_eq!(a["countryCode"], "US");
        assert_eq!(a["postalCode"], "78701");
        assert_eq!(a["city"], "Austin");
    }

    /// An absent component must not become a null; the API distinguishes them.
    #[test]
    fn omits_absent_components_entirely() {
        let b = BillingArgs {
            city: Some("Austin".into()),
            ..Default::default()
        };
        let a = b.to_address().unwrap();
        assert!(a.get("addressLine1").is_none());
        assert!(a.get("addressLine2").is_none());
        assert_eq!(a.as_object().unwrap().len(), 1);
    }

    /// No components at all is an absent address, not an empty object.
    #[test]
    fn no_components_is_no_address() {
        assert!(BillingArgs::default().to_address().is_none());
        assert!(ShippingArgs::default().to_address().is_none());
    }

    /// An empty string is not a value. `--billing-city ""` must not send a
    /// blank city the schema would accept and the processor would reject.
    #[test]
    fn an_empty_component_is_treated_as_absent() {
        let b = BillingArgs {
            city: Some(String::new()),
            ..Default::default()
        };
        assert!(b.to_address().is_none());
    }
}
