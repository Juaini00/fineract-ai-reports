use anyhow::Result;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{auth::model::IssuedRefreshToken, config::AuthConfig};

const ACCESS_TOKEN_ISSUER: &str = "ai-report";
const ACCESS_TOKEN_AUDIENCE: &str = "ai-report-api";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: Uuid,
    pub sid: Uuid,
    pub role: String,
    pub iss: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
pub struct IssuedAccessToken {
    pub token: String,
    pub expires_in: i64,
}

#[derive(Clone)]
pub struct TokenService {
    config: AuthConfig,
}

impl TokenService {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    pub fn issue_access_token(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        role: &str,
    ) -> Result<IssuedAccessToken> {
        let now = Utc::now();
        let expires_in = self.config.jwt_access_token_expiry_seconds;
        let claims = AccessTokenClaims {
            sub: user_id,
            sid: session_id,
            role: role.to_string(),
            iss: ACCESS_TOKEN_ISSUER.to_string(),
            aud: ACCESS_TOKEN_AUDIENCE.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(expires_in)).timestamp(),
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_access_secret.as_bytes()),
        )?;
        Ok(IssuedAccessToken { token, expires_in })
    }

    pub fn issue_refresh_token(&self) -> IssuedRefreshToken {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let raw_token = hex::encode(bytes);
        IssuedRefreshToken {
            id: Uuid::new_v4(),
            token_hash: hash_token(&raw_token),
            raw_token,
            expires_at: Utc::now()
                + Duration::seconds(self.config.jwt_refresh_token_expiry_seconds),
        }
    }

    pub fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims> {
        let mut validation = Validation::default();
        validation.leeway = 0;
        validation.set_issuer(&[ACCESS_TOKEN_ISSUER]);
        validation.set_audience(&[ACCESS_TOKEN_AUDIENCE]);
        Ok(decode::<AccessTokenClaims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_access_secret.as_bytes()),
            &validation,
        )?
        .claims)
    }
}

pub fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::config::AuthConfig;

    use chrono::Utc;
    use jsonwebtoken::{EncodingKey, Header, encode};

    use super::{
        ACCESS_TOKEN_AUDIENCE, ACCESS_TOKEN_ISSUER, AccessTokenClaims, TokenService, hash_token,
    };

    fn config() -> AuthConfig {
        AuthConfig {
            bootstrap_admin_token: "admin".to_string(),
            bootstrap_admin_enabled: false,
            bootstrap_admin_username: "admin".to_string(),
            bootstrap_admin_password: "password".to_string(),
            bootstrap_admin_email: "admin@example.com".to_string(),
            jwt_access_secret: "access-secret-at-least-long-enough".to_string(),
            jwt_refresh_secret: "refresh-secret-at-least-long-enough".to_string(),
            jwt_access_token_expiry_seconds: 900,
            jwt_refresh_token_expiry_seconds: 604800,
            refresh_cookie_name: "refresh_token".to_string(),
            refresh_cookie_secure: true,
            refresh_cookie_same_site: "strict".to_string(),
            refresh_cookie_path: "/".to_string(),
            api_key_prefix: "air_test".to_string(),
            api_key_default_expiration_days: 0,
        }
    }

    #[test]
    fn access_token_round_trips_claims() {
        let service = TokenService::new(config());
        let user_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let issued = service
            .issue_access_token(user_id, session_id, "admin")
            .unwrap();
        let claims = service.verify_access_token(&issued.token).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.sid, session_id);
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.iss, ACCESS_TOKEN_ISSUER);
        assert_eq!(claims.aud, ACCESS_TOKEN_AUDIENCE);
        assert_eq!(issued.expires_in, 900);
    }

    fn encoded_claims(iss: &str, aud: &str, exp: i64) -> String {
        encode(
            &Header::default(),
            &AccessTokenClaims {
                sub: Uuid::new_v4(),
                sid: Uuid::new_v4(),
                role: "admin".to_string(),
                iss: iss.to_string(),
                aud: aud.to_string(),
                exp,
                iat: Utc::now().timestamp(),
            },
            &EncodingKey::from_secret(config().jwt_access_secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn access_token_rejects_wrong_issuer_audience_and_expiry() {
        let service = TokenService::new(config());
        let now = Utc::now().timestamp();

        assert!(
            service
                .verify_access_token(&encoded_claims("wrong", ACCESS_TOKEN_AUDIENCE, now + 60))
                .is_err()
        );
        assert!(
            service
                .verify_access_token(&encoded_claims(ACCESS_TOKEN_ISSUER, "wrong", now + 60))
                .is_err()
        );
        assert!(
            service
                .verify_access_token(&encoded_claims(
                    ACCESS_TOKEN_ISSUER,
                    ACCESS_TOKEN_AUDIENCE,
                    now - 1,
                ))
                .is_err()
        );
    }

    #[test]
    fn refresh_token_stores_hash_not_raw_token() {
        let issued = TokenService::new(config()).issue_refresh_token();

        assert_ne!(issued.raw_token, issued.token_hash);
        assert_eq!(issued.token_hash, hash_token(&issued.raw_token));
    }
}
