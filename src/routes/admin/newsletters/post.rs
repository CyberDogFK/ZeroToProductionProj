use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::session_state::{reject_anonymous_users, TypedSession};
use crate::utils::e500;
use actix_web::web;
use actix_web::HttpResponse;
use anyhow::Context;
use sqlx::PgPool;

#[tracing::instrument(
name = "Publish a newsletter issue",
skip(body, pool, email_client, session),
fields(username = tracing::field::Empty, user_id = tracing::field::Empty)
)]
pub async fn publish_newsletter(
    body: web::Form<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    session: TypedSession,
) -> Result<HttpResponse, actix_web::Error> {
    reject_anonymous_users(session).await?;

    let subscribers = get_confirmed_subscriber(&pool).await.map_err(e500)?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email_elastic_mail(&subscriber.email, &body.title, &body.html, &body.text)
                    .await
                    .with_context(|| {
                        format!("Failed to send newsletter issue to {}", subscriber.email)
                    })
                    .map_err(e500)?;
            }
            Err(error) => {
                tracing::warn!(
                    error.cause_chaing = ?error,
                    "Skipping a confirmed subscriber. \
                    Their stored contact details are invalid"
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    // content: Content,
    html: String,
    text: String,
}

// #[derive(serde::Deserialize)]
// pub struct Content {
//     html: String,
//     text: String,
// }

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscriber(
    pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let confirmed_subscribers = rows
        .into_iter()
        .map(|r| match SubscriberEmail::parse(r.email) {
            Ok(email) => Ok(ConfirmedSubscriber { email }),
            Err(error) => Err(anyhow::anyhow!(error)),
        })
        .collect();
    Ok(confirmed_subscribers)
}

// Commented to test, it it real unused in project anymore.
// Must be removed, it that cause
// #[derive(thiserror::Error)]
// pub enum PublishError {
//     #[error("Authentication failed.")]
//     AuthError(#[source] anyhow::Error),
//     #[error(transparent)]
//     UnexpectedError(#[from] anyhow::Error),
// }
//
// impl std::fmt::Debug for PublishError {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         error_chain_fmt(self, f)
//     }
// }
//
// impl ResponseError for PublishError {
//     fn error_response(&self) -> HttpResponse<BoxBody> {
//         match self {
//             PublishError::UnexpectedError(_) => {
//                 HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
//             }
//             PublishError::AuthError(_) => {
//                 let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
//                 let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
//                 response
//                     .headers_mut()
//                     .insert(header::WWW_AUTHENTICATE, header_value);
//                 response
//             }
//         }
//     }
// }
