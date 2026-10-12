//! The phone app's devices, which push notifications go to (pluribus-services' push.rs): the app
//! sends the token Firebase Cloud Messaging gave it as it starts and whenever Firebase gives it
//! another (`PUT /me/devices`), for the account signed in, and takes it back as its user signs out
//! (`DELETE /me/devices/{token}`), before `POST /auth/sign-out`. Both signed in (`ApiUser`), both
//! 204.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use pluribus_core::i18n::{t, t1};
use pluribus_db as db;
use pluribus_services::AppState;
use serde::Deserialize;

use crate::{ApiError, ApiUser, JsonBody};

/// The most characters a device's token may have: Firebase's are some 160, and its own SDKs keep
/// room for many more.
pub const TOKEN_MOST: usize = 4096;

/// `PUT /me/devices`' body: the app's platform, `ios` or `android`, and the token Firebase gave it.
#[derive(Debug, Deserialize)]
pub struct DeviceRequest {
    pub platform: String,
    pub token: String,
}

/// `PUT /me/devices` `{"platform", "token"}`: the device's token kept for the user
/// (db::save_device_token), seen now; a token kept already, a phone that changed accounts, the
/// user's from now on. A platform that is neither `ios` nor `android`, a validation error on
/// `platform`; a token empty or longer than TOKEN_MOST, one on `token`. 204.
pub async fn register(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<DeviceRequest>,
) -> Result<StatusCode, ApiError> {
    let platform = match body.platform.as_str() {
        "ios" => Some("IOS"),
        "android" => Some("ANDROID"),
        _ => None,
    };
    let token = body.token.trim();
    let mut wrong = Vec::new();
    if platform.is_none() {
        wrong.push(("platform", t("Duhet të jetë „ios“ ose „android“.").to_string()));
    }
    if token.is_empty() || token.chars().count() > TOKEN_MOST {
        wrong.push(("token", t1("Tokeni i pajisjes duhet të ketë 1 deri në {} shenja.", TOKEN_MOST)));
    }
    let Some(platform) = platform.filter(|_| wrong.is_empty()) else {
        return Err(ApiError::Validation(wrong));
    };
    db::save_device_token(&state.db, user.id, platform, token).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /me/devices/{token}`: the device's token forgotten, if it is the user's
/// (db::forget_device_token), so that the phone hears nothing more of the account. 204 either way.
pub async fn forget(State(state): State<AppState>, ApiUser(user): ApiUser, Path(token): Path<String>) -> Result<StatusCode, ApiError> {
    db::forget_device_token(&state.db, token.trim(), Some(user.id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
