//! Portal branding: display name, accent colour, sidebar style, colour
//! scheme, corner style, login page texts, logo and square icon.
//!
//! Read access is public by portal slug (the login page is branded before
//! the user signs in); changes require a tenant admin.

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
    storage::BlobStore,
};

/// Maximum size of an uploaded logo or icon.
const MAX_ASSET_BYTES: usize = 512 * 1024;
const MAX_BG_BYTES: usize = 3 * 1024 * 1024;

#[derive(Serialize, sqlx::FromRow)]
pub struct Branding {
    pub tenant_slug: String,
    pub display_name: String,
    pub accent_color: String,
    pub sidebar_style: String,
    pub color_scheme: String,
    pub corner_style: String,
    pub login_title: String,
    pub login_subtitle: String,
    pub logo_plate: bool,
    pub login_logo_plate: bool,
    pub login_bg_overlay: i16,
    /// `dark`: light text, darkening veil; `light`: dark text, lightening veil.
    pub login_bg_tone: String,
    /// Base under a transparent image: `brand` (gradient), `light`, `white` or `#rrggbb`.
    pub login_bg_base: String,
    pub font_family: String,
    pub font_size: String,
    pub has_logo: bool,
    pub has_icon: bool,
    pub has_login_bg: bool,
    pub asset_version: i32,
}

const BRANDING_SELECT: &str = "SELECT t.slug::text AS tenant_slug,
        COALESCE(NULLIF(b.display_name, ''), t.name) AS display_name,
        COALESCE(b.accent_color, '#1f4e79') AS accent_color,
        COALESCE(b.sidebar_style, 'light') AS sidebar_style,
        COALESCE(b.color_scheme, 'auto') AS color_scheme,
        COALESCE(b.corner_style, 'default') AS corner_style,
        COALESCE(b.login_title, '') AS login_title,
        COALESCE(b.login_subtitle, '') AS login_subtitle,
        COALESCE(b.logo_plate, false) AS logo_plate,
        COALESCE(b.login_logo_plate, true) AS login_logo_plate,
        COALESCE(b.login_bg_overlay, 45::smallint) AS login_bg_overlay,
        COALESCE(b.login_bg_tone, 'dark') AS login_bg_tone,
        COALESCE(b.login_bg_base, 'brand') AS login_bg_base,
        COALESCE(b.font_family, 'inter') AS font_family,
        COALESCE(b.font_size, 'default') AS font_size,
        (b.logo_key IS NOT NULL) AS has_logo,
        (b.icon_key IS NOT NULL) AS has_icon,
        (b.login_bg_key IS NOT NULL) AS has_login_bg,
        COALESCE(b.asset_version, 0) AS asset_version
   FROM tenants t LEFT JOIN tenant_branding b ON b.tenant_id = t.id";

/// Branding plus ready-to-use asset URLs.
fn with_urls(b: Branding) -> Value {
    let base = format!("/api/public/branding/{}", b.tenant_slug);
    let logo = b.has_logo.then(|| format!("{base}/logo?v={}", b.asset_version));
    let icon = b.has_icon.then(|| format!("{base}/icon?v={}", b.asset_version));
    let login_bg = b.has_login_bg.then(|| format!("{base}/login_bg?v={}", b.asset_version));
    let mut v = serde_json::to_value(&b).unwrap_or_default();
    v["logo_url"] = json!(logo);
    v["icon_url"] = json!(icon);
    v["login_bg_url"] = json!(login_bg);
    v
}

async fn load_by_tenant(state: &AppState, tenant_id: Uuid) -> ApiResult<Branding> {
    Ok(sqlx::query_as::<_, Branding>(&format!("{BRANDING_SELECT} WHERE t.id = $1"))
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?)
}

/// Branding of a portal by id, with asset URLs (for pages opened without an account).
pub async fn public_by_tenant(state: &AppState, tenant_id: Uuid) -> ApiResult<Value> {
    Ok(with_urls(load_by_tenant(state, tenant_id).await?))
}

/// Branding of the current user's portal.
pub async fn get_own(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    Ok(Json(with_urls(load_by_tenant(&state, user.tenant_id).await?)))
}

/// `GET /api/public/system` — what the sign-in pages need before any portal is
/// known: whether this is the first start (no portals yet) and whether new
/// portals may be created here. The number of portals is not disclosed.
pub async fn public_system(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let any: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tenants)").fetch_one(&state.db).await?;
    Ok(Json(json!({ "first_run": !any, "signup_enabled": state.cfg.signup_enabled })))
}

/// Public branding by portal slug, used by the login page.
pub async fn get_public(State(state): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let b = sqlx::query_as::<_, Branding>(&format!("{BRANDING_SELECT} WHERE t.slug = $1 AND t.status = 'active'"))
        .bind(slug.trim())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut v = with_urls(b);
    v["registration_open"] = json!(crate::registration::is_open(&state, &slug).await);
    Ok(Json(v))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpdateBranding {
    pub display_name: String,
    pub accent_color: String,
    pub sidebar_style: String,
    pub color_scheme: String,
    pub corner_style: String,
    pub login_title: String,
    pub login_subtitle: String,
    #[serde(default)]
    pub logo_plate: bool,
    #[serde(default = "default_true")]
    pub login_logo_plate: bool,
    #[serde(default = "default_overlay")]
    pub login_bg_overlay: i16,
    #[serde(default = "default_tone")]
    pub login_bg_tone: String,
    #[serde(default = "default_base")]
    pub login_bg_base: String,
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_size")]
    pub font_size: String,
}

/// Fonts the frontend knows how to load (all with Cyrillic).
pub const FONTS: &[&str] = &[
    "system", "inter", "roboto", "pt_sans", "open_sans", "ibm_plex", "montserrat", "golos", "manrope", "onest", "noto_sans",
];

fn default_font() -> String {
    "inter".into()
}

fn default_size() -> String {
    "default".into()
}

fn default_true() -> bool {
    true
}

fn default_overlay() -> i16 {
    45
}

fn default_tone() -> String {
    "dark".into()
}

fn default_base() -> String {
    "brand".into()
}

fn normalize_color(c: &str) -> ApiResult<String> {
    let c = c.trim().to_lowercase();
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok { Ok(c) } else { bad_request("accent color must be #rrggbb") }
}

pub async fn update(State(state): State<AppState>, user: CurrentUser, Json(req): Json<UpdateBranding>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let color = normalize_color(&req.accent_color)?;
    if !matches!(req.sidebar_style.as_str(), "light" | "dark" | "accent")
        || !matches!(req.color_scheme.as_str(), "auto" | "light" | "dark")
        || !matches!(req.corner_style.as_str(), "sharp" | "default" | "round")
    {
        return bad_request("invalid style option");
    }
    if !FONTS.contains(&req.font_family.as_str()) || !matches!(req.font_size.as_str(), "small" | "default" | "large") {
        return bad_request("unknown font");
    }
    if !(0..=85).contains(&req.login_bg_overlay) {
        return bad_request("background darkening must be 0..85%");
    }
    if !matches!(req.login_bg_tone.as_str(), "dark" | "light") {
        return bad_request("login_bg_tone must be dark or light");
    }
    let base = match req.login_bg_base.trim() {
        b @ ("brand" | "light" | "white") => b.to_string(),
        other => normalize_color(other)?,
    };
    if req.display_name.chars().count() > 120 || req.login_title.chars().count() > 120 || req.login_subtitle.chars().count() > 400 {
        return bad_request("text is too long");
    }
    sqlx::query(
        "INSERT INTO tenant_branding (tenant_id, display_name, accent_color, sidebar_style, color_scheme, corner_style,
                                      login_title, login_subtitle, logo_plate, login_logo_plate, login_bg_overlay,
                                      font_family, font_size, login_bg_tone, login_bg_base, updated_at)
         VALUES ($1, NULLIF($2, ''), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
         ON CONFLICT (tenant_id) DO UPDATE SET
            display_name = EXCLUDED.display_name, accent_color = EXCLUDED.accent_color,
            sidebar_style = EXCLUDED.sidebar_style, color_scheme = EXCLUDED.color_scheme,
            corner_style = EXCLUDED.corner_style, login_title = EXCLUDED.login_title,
            login_subtitle = EXCLUDED.login_subtitle, logo_plate = EXCLUDED.logo_plate,
            login_logo_plate = EXCLUDED.login_logo_plate, login_bg_overlay = EXCLUDED.login_bg_overlay,
            font_family = EXCLUDED.font_family, font_size = EXCLUDED.font_size,
            login_bg_tone = EXCLUDED.login_bg_tone, login_bg_base = EXCLUDED.login_bg_base, updated_at = now()",
    )
    .bind(user.tenant_id)
    .bind(req.display_name.trim())
    .bind(&color)
    .bind(&req.sidebar_style)
    .bind(&req.color_scheme)
    .bind(&req.corner_style)
    .bind(req.login_title.trim())
    .bind(req.login_subtitle.trim())
    .bind(req.logo_plate)
    .bind(req.login_logo_plate)
    .bind(req.login_bg_overlay)
    .bind(&req.font_family)
    .bind(&req.font_size)
    .bind(&req.login_bg_tone)
    .bind(&base)
    .execute(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "branding.updated", None, None, json!({ "accent_color": color })).await;
    Ok(Json(with_urls(load_by_tenant(&state, user.tenant_id).await?)))
}

// ---------------------------------------------------------------------------
// Logo and icon
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AssetQuery {
    pub kind: String,
}

fn asset_kind(kind: &str) -> ApiResult<&'static str> {
    match kind {
        "logo" => Ok("logo"),
        "icon" => Ok("icon"),
        "login_bg" => Ok("login_bg"),
        _ => bad_request("kind must be 'logo', 'icon' or 'login_bg'"),
    }
}

/// Detect the image type from its content (never trust the client's MIME type).
/// SVG is accepted only without scripts, event handlers or external references.
pub fn sniff_image(data: &[u8]) -> ApiResult<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png");
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Ok("image/jpeg");
    }
    if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Ok("image/webp");
    }
    let text = std::str::from_utf8(data).map_err(|_| ApiError::BadRequest("unsupported image format".into()))?;
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if (head.starts_with("<?xml") || head.starts_with("<svg") || head.starts_with("<!--")) && text.contains("<svg") {
        let lower = text.to_lowercase();
        let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
        let dangerous = ["<script", "javascript:", "<foreignobject", "<iframe", "<embed", "<object", "<!entity", "data:text/html"];
        if dangerous.iter().any(|d| compact.contains(d)) || has_event_handler(&lower) || has_external_ref(&lower) {
            return bad_request("SVG contains scripts or external references");
        }
        return Ok("image/svg+xml");
    }
    bad_request("supported formats: PNG, JPEG, WebP, SVG")
}

/// Detects `on*=` attributes such as `onload=` or `onclick =`.
fn has_event_handler(lower: &str) -> bool {
    let b = lower.as_bytes();
    let mut i = 0;
    while let Some(pos) = lower[i..].find("on") {
        let start = i + pos;
        let prev_ok = start == 0 || matches!(b[start - 1], b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'"' | b'\'');
        let mut j = start + 2;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if prev_ok && j > start + 2 && j < b.len() && b[j] == b'=' {
            return true;
        }
        i = start + 2;
    }
    false
}

/// Detects references to external resources (`href="http..."`, `url(http...)`).
fn has_external_ref(lower: &str) -> bool {
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    ["href=\"http", "href='http", "href=\"//", "href='//", "url(http", "url(//", "url('http", "url(\"http", "@import"]
        .iter()
        .any(|p| compact.contains(p))
}

pub async fn upload_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AssetQuery>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let kind = asset_kind(&q.kind)?;
    let mut data: Option<Bytes> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        if field.file_name().is_some() {
            data = Some(field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?);
            break;
        }
    }
    let data = data.ok_or_else(|| ApiError::BadRequest("no file uploaded".into()))?;
    // Photos for the login background are larger than logos.
    let limit = if kind == "login_bg" { MAX_BG_BYTES } else { MAX_ASSET_BYTES };
    if data.len() > limit {
        return Err(ApiError::TooLarge(if kind == "login_bg" { "image must not exceed 3 MB" } else { "image must not exceed 512 KB" }.into()));
    }
    let mime = sniff_image(&data)?;

    let key = format!("{}/branding/{kind}-{}", user.tenant_id, Uuid::new_v4().simple());
    state.store.put(&key, data).await?;

    // Insert defaults row if missing, then swap the asset and remember the old key.
    sqlx::query("INSERT INTO tenant_branding (tenant_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(user.tenant_id)
        .execute(&state.db)
        .await?;
    let sql = format!(
        "UPDATE tenant_branding b SET {kind}_key = $2, {kind}_mime = $3, asset_version = b.asset_version + 1, updated_at = now()
           FROM (SELECT {kind}_key AS old_key FROM tenant_branding WHERE tenant_id = $1) o
          WHERE b.tenant_id = $1 RETURNING o.old_key"
    );
    let old: Option<String> = sqlx::query_scalar(&sql)
        .bind(user.tenant_id)
        .bind(&key)
        .bind(mime)
        .fetch_one(&state.db)
        .await?;
    if let Some(old) = old {
        let _ = state.store.delete(&old).await;
    }
    state.audit(user.tenant_id, Some(user.user_id), "branding.asset_uploaded", None, None, json!({ "kind": kind, "mime": mime })).await;
    Ok(Json(with_urls(load_by_tenant(&state, user.tenant_id).await?)))
}

pub async fn delete_asset(State(state): State<AppState>, user: CurrentUser, Query(q): Query<AssetQuery>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let kind = asset_kind(&q.kind)?;
    let sql = format!(
        "UPDATE tenant_branding b SET {kind}_key = NULL, {kind}_mime = NULL, asset_version = b.asset_version + 1
           FROM (SELECT {kind}_key AS old_key FROM tenant_branding WHERE tenant_id = $1) o
          WHERE b.tenant_id = $1 RETURNING o.old_key"
    );
    let old: Option<Option<String>> = sqlx::query_scalar(&sql).bind(user.tenant_id).fetch_optional(&state.db).await?;
    if let Some(Some(old)) = old {
        let _ = state.store.delete(&old).await;
    }
    state.audit(user.tenant_id, Some(user.user_id), "branding.asset_removed", None, None, json!({ "kind": kind })).await;
    Ok(Json(with_urls(load_by_tenant(&state, user.tenant_id).await?)))
}

/// Serve a logo or icon. Responses are immutable per `asset_version` and
/// sandboxed so that an SVG can never run script in the app's origin.
pub async fn serve_asset(State(state): State<AppState>, Path((slug, kind)): Path<(String, String)>) -> ApiResult<Response> {
    let kind = asset_kind(&kind)?;
    let sql = format!(
        "SELECT b.{kind}_key, b.{kind}_mime FROM tenant_branding b JOIN tenants t ON t.id = b.tenant_id
          WHERE t.slug = $1 AND b.{kind}_key IS NOT NULL"
    );
    let (key, mime): (String, String) = sqlx::query_as(&sql)
        .bind(slug.trim())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let data = state.store.get(&key).await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; sandbox")
        .body(Body::from(data))
        .expect("valid response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_raster_formats() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\nrest").unwrap(), "image/png");
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap(), "image/jpeg");
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 ").unwrap(), "image/webp");
        assert!(sniff_image(b"GIF89a").is_err());
    }

    #[test]
    fn accepts_clean_svg() {
        let svg = br##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#123456"/></svg>"##;
        assert_eq!(sniff_image(svg).unwrap(), "image/svg+xml");
    }

    #[test]
    fn rejects_dangerous_svg() {
        for bad in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg onload="alert(1)"></svg>"#,
            r#"<svg><rect ONCLICK = "x"/></svg>"#,
            r#"<svg><a href="javascript:alert(1)"><rect/></a></svg>"#,
            r#"<svg><image href="https://evil.example/x.png"/></svg>"#,
            r#"<svg><foreignObject><div/></foreignObject></svg>"#,
            r#"<svg><style>@import url(http://x)</style></svg>"#,
        ] {
            assert!(sniff_image(bad.as_bytes()).is_err(), "should reject: {bad}");
        }
    }

    #[test]
    fn words_containing_on_are_fine() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><text font-family="Montserrat">Construction</text></svg>"#;
        assert!(sniff_image(svg).is_ok());
    }
}
