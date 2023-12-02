use actix_web::http::header::ContentType;
use actix_web::HttpResponse;
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

pub async fn send_newsletters_form(
    flash_message: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msg_html = String::new();
    for m in flash_message.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"
        <!DOCTYPE html/>
        <head>
            <meta http-equiv="content-type" content="text/html; charset=utf-8">
            <title>Send newsletters</title>
        </head>
        <body>
            {msg_html}
            <form action="/admin/newsletters" method="post">
                <label>Title
                    <input
                        type="text"
                        placeholder="Enter title of letter"
                        name="title"
                    >
                </label>
                <br>
                <label>Text body
                    <input
                        type="text"
                        placeholder="Enter text of your letter"
                        name="text_content"
                    >
                </label>
                <br>
                <label>HTML body
                    <input
                        type="text"
                        placeholder="Enter html text of your letter"
                        name="html_content"
                    >
                </label>
                <button type="submit">Send to all subscribers</button>
            </form>
        </body>
        "#,
        )))
}
