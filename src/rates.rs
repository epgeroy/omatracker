use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

// Keeps the persisted rate exactly representable by the QML presentation layer.
const MAX_RATE_MINOR: i64 = 1_000_000_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", try_from = "StoredRate")]
pub struct HourlyRate {
    amount_minor: i64,
    currency: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRate {
    amount_minor: i64,
    currency: String,
}

impl TryFrom<StoredRate> for HourlyRate {
    type Error = anyhow::Error;

    fn try_from(value: StoredRate) -> Result<Self> {
        let currency = value.currency.trim().to_ascii_uppercase();
        currency_digits(&currency)?;
        if !(0..=MAX_RATE_MINOR).contains(&value.amount_minor) {
            bail!("hourly rate must be between 0 and {MAX_RATE_MINOR} minor units")
        }
        Ok(Self {
            amount_minor: value.amount_minor,
            currency,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub currency: String,
    pub fraction_digits: usize,
    pub hourly_rate: String,
    pub rate_text: String,
    // A string preserves exact large totals in JSON consumers using doubles.
    pub amount_minor: String,
    pub amount_text: String,
}

fn currency_digits(currency: &str) -> Result<usize> {
    match currency {
        "JPY" | "KRW" | "CLP" | "VND" => Ok(0),
        "BHD" | "KWD" | "OMR" | "TND" => Ok(3),
        "USD" | "EUR" | "GBP" | "CAD" | "AUD" | "NZD" | "CHF" | "CNY" | "INR" | "BRL" | "MXN"
        | "ARS" | "COP" | "PEN" | "ZAR" | "NGN" | "EGP" | "KES" | "SEK" | "NOK" | "DKK" | "PLN"
        | "CZK" | "HUF" | "RON" | "TRY" | "UAH" | "RUB" | "ILS" | "AED" | "SAR" | "QAR" | "SGD"
        | "HKD" | "TWD" | "THB" | "MYR" | "IDR" | "PHP" | "PKR" | "BDT" => Ok(2),
        _ => bail!("unsupported currency {currency}; see the supported currencies in README.md"),
    }
}

fn decimal_text(minor: i128, digits: usize) -> String {
    let scale = 10_i128.pow(digits as u32);
    if digits == 0 {
        minor.to_string()
    } else {
        format!("{}.{:0digits$}", minor / scale, minor % scale)
    }
}

impl HourlyRate {
    pub fn parse(amount: &str, currency: &str) -> Result<Self> {
        let currency = currency.trim().to_ascii_uppercase();
        let digits = currency_digits(&currency)?;
        let amount = amount.trim();
        let (whole, fraction) = amount.split_once('.').unwrap_or((amount, ""));
        if whole.is_empty()
            || !whole.bytes().all(|c| c.is_ascii_digit())
            || !fraction.bytes().all(|c| c.is_ascii_digit())
            || fraction.len() > digits
            || (amount.contains('.') && fraction.is_empty())
        {
            bail!(
                "hourly rate must be a non-negative decimal with at most {digits} fractional digits for {currency}"
            )
        }
        let scale = 10_i64.pow(digits as u32);
        let whole: i64 = whole.parse().context("hourly rate is too large")?;
        let fraction: i64 = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<i64>()? * 10_i64.pow((digits - fraction.len()) as u32)
        };
        let amount_minor = whole
            .checked_mul(scale)
            .and_then(|v| v.checked_add(fraction))
            .context("hourly rate is too large")?;
        Self::try_from(StoredRate {
            amount_minor,
            currency,
        })
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn estimate(&self, seconds: i64) -> Estimate {
        // Both operands are non-negative i64 values, so their product plus the
        // half-unit rounding offset fits in i128 even at the ledger's limits.
        let amount = (i128::from(self.amount_minor) * i128::from(seconds.max(0)) + 1800) / 3600;
        let digits = currency_digits(&self.currency).expect("validated rate currency");
        let hourly_rate = decimal_text(self.amount_minor.into(), digits);
        Estimate {
            currency: self.currency.clone(),
            fraction_digits: digits,
            rate_text: format!("{} {hourly_rate}/h", self.currency),
            hourly_rate,
            amount_minor: amount.to_string(),
            amount_text: format!("{} {}", self.currency, decimal_text(amount, digits)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_parsing_and_currency_precision() {
        assert_eq!(
            HourlyRate::parse(" 75.5 ", "usd")
                .unwrap()
                .estimate(3600)
                .amount_text,
            "USD 75.50"
        );
        assert_eq!(
            HourlyRate::parse("80", "USD")
                .unwrap()
                .estimate(5400)
                .amount_text,
            "USD 120.00"
        );
        assert_eq!(
            HourlyRate::parse("125", "JPY")
                .unwrap()
                .estimate(1800)
                .amount_text,
            "JPY 63"
        );
        assert_eq!(
            HourlyRate::parse("1.001", "KWD")
                .unwrap()
                .estimate(1800)
                .amount_text,
            "KWD 0.501"
        );
        assert_eq!(
            HourlyRate::parse("0", "EUR")
                .unwrap()
                .estimate(5400)
                .amount_text,
            "EUR 0.00"
        );
        for (amount, currency) in [
            ("-1", "USD"),
            ("NaN", "USD"),
            ("1e2", "USD"),
            ("1,50", "USD"),
            ("1.001", "USD"),
            ("1.1", "JPY"),
            ("1.", "USD"),
            (".5", "USD"),
            ("", "USD"),
            ("1", "XXX"),
            ("10000000.01", "USD"),
            ("99999999999999999999999999", "USD"),
        ] {
            assert!(
                HourlyRate::parse(amount, currency).is_err(),
                "{amount} {currency}"
            );
        }
    }

    #[test]
    fn rounding_is_once_at_total_and_large_totals_are_exact() {
        let rate = HourlyRate::parse("0.01", "USD").unwrap();
        assert_eq!(rate.estimate(1799).amount_minor, "0");
        assert_eq!(rate.estimate(1800).amount_minor, "1");
        assert_eq!(rate.estimate(3600).amount_minor, "1");
        let maximum = HourlyRate::parse("10000000", "USD").unwrap();
        assert_eq!(
            maximum.estimate(i64::MAX).amount_minor,
            "2562047788015215501944444"
        );
    }

    #[test]
    fn deserialization_rejects_invalid_persisted_rates() {
        for json in [
            r#"{"amountMinor":-1,"currency":"USD"}"#,
            r#"{"amountMinor":1000000001,"currency":"USD"}"#,
            r#"{"amountMinor":100,"currency":"BAD"}"#,
        ] {
            assert!(serde_json::from_str::<HourlyRate>(json).is_err());
        }
    }
}
