//! Price variants: a service has one flat price for everyone, or gender-specific prices where a
//! missing gender means "not offered" (e.g. Ôn ấm tử cung has no male price).

use kernel::Money;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PriceVariant {
    All,
    Female,
    Male,
}

impl PriceVariant {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Female => "female",
            Self::Male => "male",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "female" => Some(Self::Female),
            "male" => Some(Self::Male),
            _ => None,
        }
    }
}

/// A customer's gender as recorded on their profile (may be unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Female,
    Male,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PricingError {
    #[error("a service needs at least one price")]
    NoPrice,
    #[error("a flat price cannot be combined with gender prices")]
    MixedVariants,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PriceLookupError {
    #[error("gender must be chosen for this service")]
    GenderRequired,
    #[error("service is not offered for this gender")]
    NotOffered,
}

/// Validated price set of one service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
pub struct ServicePrices {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<i64>)]
    pub all: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<i64>)]
    pub female: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<i64>)]
    pub male: Option<Money>,
}

impl ServicePrices {
    pub fn validate(self) -> Result<Self, PricingError> {
        match (self.all, self.female, self.male) {
            (None, None, None) => Err(PricingError::NoPrice),
            (Some(_), None, None) | (None, _, _) => Ok(self),
            (Some(_), _, _) => Err(PricingError::MixedVariants),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_none() && self.female.is_none() && self.male.is_none()
    }

    pub fn set(&mut self, variant: PriceVariant, amount: Money) {
        match variant {
            PriceVariant::All => self.all = Some(amount),
            PriceVariant::Female => self.female = Some(amount),
            PriceVariant::Male => self.male = Some(amount),
        }
    }

    /// Price charged for a one-off service, chosen by the customer's gender (AC4.2b/c).
    pub fn price_for(
        &self,
        gender: Option<Gender>,
    ) -> Result<(PriceVariant, Money), PriceLookupError> {
        if let Some(all) = self.all {
            return Ok((PriceVariant::All, all));
        }
        match gender {
            None => Err(PriceLookupError::GenderRequired),
            Some(Gender::Female) => self
                .female
                .map(|p| (PriceVariant::Female, p))
                .ok_or(PriceLookupError::NotOffered),
            Some(Gender::Male) => self
                .male
                .map(|p| (PriceVariant::Male, p))
                .ok_or(PriceLookupError::NotOffered),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vnd(amount: i64) -> Money {
        Money::vnd(amount).unwrap_or_default()
    }

    #[test]
    fn flat_or_gendered_prices_are_valid_but_not_both() {
        let flat = ServicePrices {
            all: Some(vnd(149_000)),
            ..Default::default()
        };
        let gendered = ServicePrices {
            female: Some(vnd(79_000)),
            male: Some(vnd(99_000)),
            ..Default::default()
        };
        let female_only = ServicePrices {
            female: Some(vnd(399_000)),
            ..Default::default()
        };
        assert!(flat.validate().is_ok());
        assert!(gendered.validate().is_ok());
        assert!(female_only.validate().is_ok());
        assert_eq!(
            ServicePrices::default().validate(),
            Err(PricingError::NoPrice)
        );
        let mixed = ServicePrices {
            all: Some(vnd(1)),
            male: Some(vnd(2)),
            ..Default::default()
        };
        assert_eq!(mixed.validate(), Err(PricingError::MixedVariants));
    }

    #[test]
    fn price_follows_the_customers_gender() {
        let thu_gian = ServicePrices {
            female: Some(vnd(79_000)),
            male: Some(vnd(99_000)),
            ..Default::default()
        };
        assert_eq!(
            thu_gian.price_for(Some(Gender::Male)),
            Ok((PriceVariant::Male, vnd(99_000)))
        );
        assert_eq!(
            thu_gian.price_for(Some(Gender::Female)),
            Ok((PriceVariant::Female, vnd(79_000)))
        );
        assert_eq!(
            thu_gian.price_for(None),
            Err(PriceLookupError::GenderRequired)
        );
    }

    #[test]
    fn flat_price_ignores_gender_and_missing_gender_is_not_offered() {
        let flat = ServicePrices {
            all: Some(vnd(149_000)),
            ..Default::default()
        };
        assert_eq!(flat.price_for(None), Ok((PriceVariant::All, vnd(149_000))));
        let female_only = ServicePrices {
            female: Some(vnd(399_000)),
            ..Default::default()
        };
        assert_eq!(
            female_only.price_for(Some(Gender::Male)),
            Err(PriceLookupError::NotOffered)
        );
    }
}
