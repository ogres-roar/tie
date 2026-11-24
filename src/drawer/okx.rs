use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, de::DeserializeOwned};
use sha2::Sha256;
use std::time::Duration;

const BALANCE_PATH: &str = "/api/v5/account/balance";
const ASSET_VALUATION_PATH: &str = "/api/v5/asset/asset-valuation";
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
    pub avail_bal: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResponse<T> {
    code: String,
    msg: String,
    data: Vec<T>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetValuation {
    pub total_bal: String,
    pub details: AssetValuationDetails,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct AssetValuationDetails {
    pub classic: String,
    pub earn: String,
    pub funding: String,
    pub trading: String,
}

impl Okx {
    pub async fn get_balance(self) -> Result<Vec<AccountBalance>, String> {
        self.get(BALANCE_PATH, "balance").await
    }

    pub async fn get_asset_valuation(self) -> Result<Vec<AssetValuation>, String> {
        self.get(ASSET_VALUATION_PATH, "asset valuation").await
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

        let request_path = format!("{path}?ccy=USDT");
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
    use super::{ApiResponse, AssetValuation};

    #[test]
    fn parses_asset_valuation_response() {
        let response: ApiResponse<AssetValuation> = serde_json::from_str(
            r#"{
                "code": "0",
                "msg": "",
                "data": [{
                    "details": {
                        "classic": "124.6",
                        "earn": "1122.73",
                        "funding": "0.09",
                        "trading": "2544.28"
                    },
                    "totalBal": "3790.09",
                    "ts": "1637566660769"
                }]
            }"#,
        )
        .expect("asset valuation response should parse");

        assert_eq!(response.code, "0");
        assert_eq!(response.data[0].total_bal, "3790.09");
        assert_eq!(response.data[0].details.trading, "2544.28");
    }
}
