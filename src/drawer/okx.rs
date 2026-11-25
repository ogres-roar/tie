use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, de::DeserializeOwned};
use sha2::Sha256;
use std::time::Duration;

const BALANCE_PATH: &str = "/api/v5/account/balance";
const BASE_URL: &str = "https://openapi.okx.com";

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Okx {
    api_key: String,
    secret_key: String,
    #[serde(default)]
    passphrase: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    pub total_eq: String,
    pub details: Vec<BalanceDetail>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceDetail {
    pub ccy: String,
    pub eq: String,
    pub eq_usd: String,
}

impl BalanceDetail {
    pub fn spot_price_usd(&self) -> Result<Option<f64>, String> {
        let eq_usd = self
            .eq_usd
            .parse::<f64>()
            .map_err(|error| format!("could not parse {} USD equity: {error}", self.ccy))?;
        if !eq_usd.is_finite() {
            return Err(format!("invalid USD equity for {}", self.ccy));
        }
        if eq_usd <= 1.0 {
            return Ok(None);
        }

        let eq = self
            .eq
            .parse::<f64>()
            .map_err(|error| format!("could not parse {} balance: {error}", self.ccy))?;
        if !eq.is_finite() || eq <= 0.0 {
            return Err(format!(
                "cannot calculate {} spot price from balance",
                self.ccy
            ));
        }

        let price = eq_usd / eq;
        if !price.is_finite() {
            return Err(format!("invalid {} spot price", self.ccy));
        }

        Ok(Some(price))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResponse<T> {
    code: String,
    msg: String,
    data: Vec<T>,
}

impl Okx {
    pub async fn get_balance(self) -> Result<Vec<AccountBalance>, String> {
        self.get(BALANCE_PATH, "balance").await
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        endpoint_name: &str,
    ) -> Result<Vec<T>, String> {
        if self.passphrase.is_empty() {
            return Err(
                "OKX REST authentication requires `passphrase`; add it to the OKX item in conf/layout.toml."
                    .to_owned(),
            );
        }

        let request_path = format!("{path}");
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let prehash = format!("{timestamp}GET{request_path}");
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes())
            .map_err(|error| format!("could not initialize OKX request signature: {error}"))?;
        mac.update(prehash.as_bytes());
        let signature = STANDARD.encode(mac.finalize().into_bytes());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("could not create OKX HTTP client: {error}"))?;
        let response = client
            .get(format!("{BASE_URL}{request_path}"))
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", signature)
            .header("OK-ACCESS-TIMESTAMP", timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase)
            .send()
            .await
            .map_err(|error| format!("OKX {endpoint_name} request failed: {error}"))?;
        let status = response.status();
        let body = response
            .json::<ApiResponse<T>>()
            .await
            .map_err(|error| format!("could not parse OKX {endpoint_name} response: {error}"))?;

        if !status.is_success() || body.code != "0" {
            return Err(format!(
                "OKX {endpoint_name} request returned {} ({}): {}",
                status, body.code, body.msg
            ));
        }

        Ok(body.data)
    }
}

#[cfg(test)]
mod tests {
    use super::BalanceDetail;

    #[test]
    fn computes_spot_price_for_assets_over_one_usd() {
        let balance = BalanceDetail {
            ccy: "BTC".to_owned(),
            eq: "2".to_owned(),
            eq_usd: "200".to_owned(),
        };
        assert_eq!(balance.spot_price_usd().unwrap(), Some(100.0));
    }

    #[test]
    fn omits_assets_valued_at_one_usd_or_less() {
        let balance = BalanceDetail {
            ccy: "BTC".to_owned(),
            eq: "2".to_owned(),
            eq_usd: "1".to_owned(),
        };
        assert_eq!(balance.spot_price_usd().unwrap(), None);
    }

    #[test]
    fn rejects_zero_balance_when_calculating_price() {
        let balance = BalanceDetail {
            ccy: "BTC".to_owned(),
            eq: "0".to_owned(),
            eq_usd: "2".to_owned(),
        };
        assert!(balance.spot_price_usd().is_err());
    }
}
