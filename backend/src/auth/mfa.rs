//! Two-factor authentication secrets and recovery codes (iteration 32, D3).
//!
//! - TOTP secrets are stored encrypted with AES-256-GCM; the key is derived
//!   from `APP_SECRET` (as for SMTP passwords). Changing `APP_SECRET` makes the
//!   stored secrets unreadable: users then have to set up 2FA again.
//! - Recovery codes: 10 one-time codes `XXXX-XXXX` issued when 2FA is turned on
//!   (or re-issued with the password). Only a keyed SHA-256 of each code is
//!   stored; a code works once instead of the authenticator code.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{extract::State, Json};
use rand::{Rng, RngCore};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{verify_password, CurrentUser};
use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Number of recovery codes in a set.
pub const RECOVERY_CODES: usize = 10;
/// Alphabet without look-alikes (0/O, 1/I/L).
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

fn cipher(state: &AppState) -> Aes256Gcm {
    let key = Sha256::digest(format!("workhub-totp-secret:{}", state.cfg.app_secret).as_bytes());
    Aes256Gcm::new_from_slice(&key).expect("32-byte key")
}

/// Encrypt a base32 TOTP secret: 12-byte nonce followed by the ciphertext.
pub fn seal_secret(state: &AppState, secret: &str) -> ApiResult<Vec<u8>> {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let data = cipher(state)
        .encrypt(Nonce::from_slice(&nonce), secret.as_bytes())
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("encryption failed")))?;
    Ok([nonce.as_slice(), &data].concat())
}

/// Decrypt a stored TOTP secret; `None` if it was sealed with another APP_SECRET.
pub fn open_secret(state: &AppState, blob: &[u8]) -> Option<String> {
    if blob.len() < 13 {
        return None;
    }
    let (nonce, data) = blob.split_at(12);
    let plain = cipher(state).decrypt(Nonce::from_slice(nonce), data).ok()?;
    String::from_utf8(plain).ok()
}

/// The user's TOTP secret, decrypted. An unreadable secret (APP_SECRET changed)
/// is logged and treated as missing.
pub async fn load_secret(state: &AppState, user_id: Uuid) -> ApiResult<Option<String>> {
    let blob: Option<Vec<u8>> = sqlx::query_scalar("SELECT totp_secret_enc FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
    Ok(blob.and_then(|b| {
        let s = open_secret(state, &b);
        if s.is_none() {
            tracing::warn!(%user_id, "TOTP secret cannot be decrypted (APP_SECRET changed?)");
        }
        s
    }))
}

/// Start-up: encrypt plain secrets left from before migration 0031.
pub async fn encrypt_legacy_secrets(state: &AppState) -> anyhow::Result<()> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, totp_secret FROM users WHERE totp_secret IS NOT NULL").fetch_all(&state.db).await?;
    for (id, secret) in &rows {
        let blob = seal_secret(state, secret).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sqlx::query("UPDATE users SET totp_secret_enc = $2, totp_secret = NULL WHERE id = $1")
            .bind(id)
            .bind(blob)
            .execute(&state.db)
            .await?;
    }
    if !rows.is_empty() {
        tracing::info!(count = rows.len(), "encrypted TOTP secrets");
    }
    Ok(())
}

/// Upper-case letters and digits of a typed code, without dashes and spaces.
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Looks like a recovery code (8 characters with a letter) rather than a 6-digit TOTP code.
pub fn is_recovery_code(code: &str) -> bool {
    let n = normalize_code(code);
    n.len() == 8 && n.chars().any(|c| c.is_ascii_alphabetic())
}

fn code_hash(state: &AppState, user_id: Uuid, normalized: &str) -> Vec<u8> {
    Sha256::digest(format!("workhub-recovery:{}:{user_id}:{normalized}", state.cfg.app_secret).as_bytes()).to_vec()
}

/// A code always has a letter, so it is never mistaken for a 6–8 digit number.
fn random_code() -> String {
    let mut rng = rand::thread_rng();
    loop {
        let s: String = (0..8).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect();
        if s.chars().any(|c| c.is_ascii_alphabetic()) {
            return format!("{}-{}", &s[..4], &s[4..]);
        }
    }
}

/// Replace the user's recovery codes with a new set; returns the codes to show once.
pub async fn issue_codes(state: &AppState, user_id: Uuid) -> ApiResult<Vec<String>> {
    let mut codes: Vec<String> = Vec::with_capacity(RECOVERY_CODES);
    while codes.len() < RECOVERY_CODES {
        let c = random_code();
        if !codes.contains(&c) {
            codes.push(c);
        }
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    for c in &codes {
        sqlx::query("INSERT INTO user_recovery_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(code_hash(state, user_id, &normalize_code(c)))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(codes)
}

/// Spend a recovery code; true if it was valid and unused.
pub async fn use_code(state: &AppState, user_id: Uuid, code: &str) -> ApiResult<bool> {
    let n = normalize_code(code);
    if n.len() != 8 {
        return Ok(false);
    }
    let done = sqlx::query(
        "UPDATE user_recovery_codes SET used_at = now() WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
    )
    .bind(user_id)
    .bind(code_hash(state, user_id, &n))
    .execute(&state.db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Unused recovery codes of a user.
pub async fn codes_left(state: &AppState, user_id: Uuid) -> ApiResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM user_recovery_codes WHERE user_id = $1 AND used_at IS NULL")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?)
}

/// Turn 2FA off for a user: secret and recovery codes are removed.
pub async fn clear(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    sqlx::query("UPDATE users SET totp_enabled = false, totp_secret = NULL, totp_secret_enc = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM user_recovery_codes WHERE user_id = $1").bind(user_id).execute(&state.db).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct RegenerateRequest {
    pub password: String,
}

/// `POST /api/auth/totp/recovery-codes` — a new set of recovery codes (password required);
/// the previous codes stop working.
pub async fn regenerate(State(state): State<AppState>, user: CurrentUser, Json(req): Json<RegenerateRequest>) -> ApiResult<Json<Value>> {
    let (hash, enabled): (String, bool) = sqlx::query_as("SELECT password_hash, totp_enabled FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password(req.password, hash).await {
        return Err(ApiError::Forbidden);
    }
    if !enabled {
        return Err(ApiError::BadRequest("двухфакторная аутентификация не включена".into()));
    }
    let codes = issue_codes(&state, user.user_id).await?;
    state.audit(user.tenant_id, Some(user.user_id), "auth.recovery_codes_issued", None, None, json!({})).await;
    Ok(Json(json!({ "recovery_codes": codes })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_code_shape() {
        for _ in 0..200 {
            let c = random_code();
            assert_eq!(c.len(), 9);
            assert_eq!(&c[4..5], "-");
            assert!(c.chars().filter(|&ch| ch != '-').all(|ch| ALPHABET.contains(&(ch as u8))));
        }
        assert_eq!(normalize_code(" 4fkt-9qzm "), "4FKT9QZM");
        assert!(is_recovery_code("4fkt 9qzm"));
        assert!(!is_recovery_code("123456"));
        assert!(!is_recovery_code("12345678"));
    }
}
