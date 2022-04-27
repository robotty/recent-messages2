use crate::web::auth::UserAuthorization;
use crate::web::error::ApiError;
use crate::web::WebAppData;
use axum::Extension;
use http::StatusCode;

pub async fn purge_messages(
    Extension(authorization): Extension<UserAuthorization>,
    app_data: Extension<WebAppData>,
) -> Result<StatusCode, ApiError> {
    app_data
        .data_storage
        .purge_messages(&authorization.user_login)
        .await;
    Ok(StatusCode::NO_CONTENT)
}
