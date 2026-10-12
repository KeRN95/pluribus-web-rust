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
use utoipa::ToSchema;

use crate::{
    ApiError, ApiUser, JsonBody,
    openapi::{Internal, Unauthorized, Validation, kinds},
};

/// The most characters a device's token may have: Firebase's are some 160, and its own SDKs keep
/// room for many more.
pub const TOKEN_MOST: usize = 4096;

/// The platforms the app runs on, each as the app names it and as the schema spells it
/// (device_tokens.platform): what `PUT /me/devices` takes, and the OpenAPI document's `Platform`.
pub const PLATFORMS: [(&str, &str); 2] = [("ios", "IOS"), ("android", "ANDROID")];

/// `PUT /me/devices`' body: the app's platform, `ios` or `android`, and the token Firebase gave it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceRequest {
    #[schema(value_type = kinds::Platform)]
    pub platform: String,
    /// The token Firebase Cloud Messaging gave the app on the device, 1 to 4096 characters.
    pub token: String,
}

/// `PUT /me/devices` `{"platform", "token"}`: the device's token kept for the user
/// (db::save_device_token), seen now; a token kept already, a phone that changed accounts, the
/// user's from now on. A platform that is neither `ios` nor `android`, a validation error on
/// `platform`; a token empty or longer than TOKEN_MOST, one on `token`. 204.
#[utoipa::path(
    put,
    path = "/me/devices",
    tag = "devices",
    operation_id = "registerDevice",
    summary = "Keep the device's push token",
    description = "Keeps the token Firebase Cloud Messaging gave the app on the device for the user, as the app starts and whenever Firebase gives it another, so that the user's notifications reach the phone by push.",
    request_body = DeviceRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Kept"),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<DeviceRequest>,
) -> Result<StatusCode, ApiError> {
    let platform = PLATFORMS.iter().find(|(named, _)| *named == body.platform).map(|(_, spelt)| *spelt);
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
#[utoipa::path(
    delete,
    path = "/me/devices/{token}",
    tag = "devices",
    operation_id = "forgetDevice",
    summary = "Forget the device's push token",
    description = "Forgets the device's token, if it is the user's, as the app signs out on the device (before `/auth/sign-out`), so that nothing more reaches it by push.",
    params(("token" = String, Path, description = "The token the app sent to `PUT /me/devices`")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Forgotten, or never the user's"),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn forget(State(state): State<AppState>, ApiUser(user): ApiUser, Path(token): Path<String>) -> Result<StatusCode, ApiError> {
    db::forget_device_token(&state.db, token.trim(), Some(user.id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
