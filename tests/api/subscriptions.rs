use crate::helpers::spawn_app;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn user_tries_subscribe_twice_when_he_is_not_confirmed_and_get_two_confirms_token() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    let first_response = app.post_subscriptions(body.into()).await;

    let saved_id = sqlx::query!("SELECT id FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .unwrap().id;
    let first_token =
        get_subscription_token_for_subscriber_id(&app.db_pool, saved_id).await;

    assert_eq!(first_response.status().as_u16(), 200);

    let second_response = app.post_subscriptions(body.into()).await;
    let second_token =
        get_subscription_token_for_subscriber_id(&app.db_pool, saved_id).await;

    assert_eq!(second_response.status().as_u16(), 200);

    let received_requests = &app.email_server.received_requests().await.unwrap();

    let first_email_request = &received_requests[0];
    let first_confirmation_links = app.get_confirmation_links(first_email_request);

    let second_email_request = &received_requests[1];
    let second_confirmation_links = app.get_confirmation_links(second_email_request);

    let all_saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_all(&app.db_pool)
        .await
        .unwrap();

    assert_eq!(all_saved.len(), 1, "You must not saved again same user");
    assert_ne!(first_token, second_token, "With every request saved token must change");
    println!("first_token {} \n second_token {}", first_token, second_token);
    println!("first confirmation link {},\n second confirmation link {}", first_confirmation_links.html, second_confirmation_links.html);
    assert_ne!(
        first_confirmation_links.html,
        second_confirmation_links.html,
        "Send different links with different token"
    )
}

async fn get_subscription_token_for_subscriber_id(db_pool: &PgPool, subscriber_id: Uuid) -> String {
    sqlx::query!(
        r#"SELECT subscription_token FROM subscription_tokens WHERE subscriber_id = $1"#,
        subscriber_id
    )
    .fetch_one(db_pool)
    .await
    .unwrap()
    .subscription_token
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_with_a_link() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    // get the first intercepted request
    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let confirmation_links = app.get_confirmation_links(email_request);

    assert_eq!(confirmation_links.html, confirmation_links.plain_text)
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_for_valid_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;
}

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    let response = app.post_subscriptions(body.into()).await;

    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persist_the_new_subscriber() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let app = spawn_app().await;

    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = app.post_subscriptions(invalid_body.into()).await;
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    let app = spawn_app().await;
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursule&email=definitely-not-an-email", "invalid email"),
    ];

    for (body, description) in test_cases {
        let response = app.post_subscriptions(body.into()).await;
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 200 OK when the payload was {}, {}. Response {}",
            body,
            description,
            response.text().await.unwrap()
        )
    }
}
