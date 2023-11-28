use crate::authentication::{validate_credentials, Credentials};
use crate::session_state::{reject_anonymous_users, TypedSession};
use crate::utils::{e500, see_other};
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: Secret<String>,
    new_password: Secret<String>,
    new_password_check: Secret<String>,
}

#[tracing::instrument(name = "Changing password", skip(form, session, pool))]
pub async fn change_password(
    form: web::Form<FormData>,
    session: TypedSession,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = reject_anonymous_users(session).await?;

    if validate_current_password(&user_id, form.0.current_password, &pool)
        .await
        .is_err()
    {
        FlashMessage::error("Wrong current password!").send();
        return Ok(redirect_back_to_page());
    }
    let new_password = form.0.new_password;
    let check_password = form.0.new_password_check;
    if new_password.expose_secret().len() <= 12 {
        FlashMessage::error("Password must be longer than 12 symbols").send();
        return Ok(redirect_back_to_page());
    }
    if new_password.expose_secret() != check_password.expose_secret() {
        FlashMessage::error(
            "You entered two different new passwords - the field values must match.",
        )
        .send();
        return Ok(redirect_back_to_page());
    }
    crate::authentication::change_password(user_id, new_password, &pool)
        .await
        .map_err(e500)?;
    FlashMessage::error("Your password has been changed.").send();
    Ok(redirect_back_to_page())
}

fn redirect_back_to_page() -> HttpResponse {
    see_other("/admin/password")
}

async fn validate_current_password(
    user_id: &Uuid,
    current_password: Secret<String>,
    pg_pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let username = match get_username_by_id(user_id, pg_pool).await? {
        None => Err(anyhow::anyhow!("User without username!"))?,
        Some(username) => username,
    };

    validate_credentials(
        Credentials {
            username,
            password: current_password,
        },
        pg_pool,
    )
    .await?;
    Ok(())
}

pub async fn get_username_by_id(
    user_id: &Uuid,
    pool: &PgPool,
) -> Result<Option<String>, anyhow::Error> {
    let username = sqlx::query!(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.username);
    Ok(username)
}
