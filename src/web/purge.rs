use crate::db::DataStorage;
use crate::web::auth::UserAuthorization;
use warp::Rejection;

pub async fn purge_messages(
    authorization: UserAuthorization,
    data_storage: &'static DataStorage,
) -> Result<impl warp::Reply, Rejection> {
    data_storage.purge_messages(&authorization.user_login).await;
    Ok(warp::reply())
}
