use crate::helpers::{assert_is_redirect_to, spawn_app, ConfirmationLinks, TestApp};
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn post_accept_users_by_session_based_authentication() {
    let app = spawn_app().await;

    app.post_login(&app.get_json_with_app_test_user()).await;
    let body = "title=Newsletter%20title&text_content=Newsletter%20body%20as%20plain%20text&\
    html_content=%3Cp%3ENewsletter%20body%20as%20HTML%3C%2Fp%3E"
        .to_string();

    let response = app.post_admin_newsletters(body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");
}

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&app.email_server)
        .await;

    let body = "title=Newsletter%20title&text_content=Newsletter%20body%20as%20plain%20text&\
    html_content=%3Cp%3ENewsletter%20body%20as%20HTML%3C%2Fp%3E"
        .to_string();
    app.post_login(&app.get_json_with_app_test_user()).await;
    let response = app.post_admin_newsletters(body.to_string()).await;

    assert_is_redirect_to(&response, "/admin/newsletters");
    let response: String = app.get_admin_newsletters().await;
    assert!(response.contains("<p><i>The newsletter issue has been published!</i></p>"))
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;

    Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    let body = "title=Newsletter%20title&text_content=Newsletter%20body%20as%20plain%20text&\
    html_content=%3Cp%3ENewsletter%20body%20as%20HTML%3C%2Fp%3E"
        .to_string();
    app.post_login(&app.get_json_with_app_test_user()).await;
    let response = app.post_admin_newsletters(body).await;

    assert_is_redirect_to(&response, "/admin/newsletters");
    let response: String = app.get_admin_newsletters().await;
    assert!(response.contains("<p><i>The newsletter issue has been published!</i></p>"))
}

#[tokio::test]
async fn newsletters_returns_400_for_invalid_data() {
    let app = spawn_app().await;
    let test_cases: Vec<(&str, &str)> = vec![
        (
            "text_content=Newsletter%20body%20as%20plain%20text&html_content=%3Cp%3ENewsletter%20body%20as%20HTML%3C%2Fp%3E",
            "missing title"
        ),
        (
            "title=Newsletter%20title",
            "missing text_content and html_content"
        ),
    ];

    app.post_login(&app.get_json_with_app_test_user()).await;
    for (invalid_body, error_message) in test_cases {
        let response = app.post_admin_newsletters(invalid_body.to_string()).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}

#[tokio::test]
async fn request_missing_authorization_are_rejected() {
    let app = spawn_app().await;

    let body = "title=Newsletter%20title&text_content=Newsletter%20body%20as%20plain%20text&\
    html_content=%3Cp%3ENewsletter%20body%20as%20HTML%3C%2Fp%3E"
        .to_string();
    let response = app.post_admin_newsletters(body).await;

    assert_is_redirect_to(&response, "/login")
}

async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=le%20guin&email=ursule_le_guin%40gmail.com";

    let _mock_guard = Mock::given(path("/email/send"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;
    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();

    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    app.get_confirmation_links(email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    let confirmation_link = create_unconfirmed_subscriber(app).await;
    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}
