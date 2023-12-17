use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::{configuration::Settings, startup::get_connection_pool};
use sqlx::{PgPool, Postgres, Transaction};
use std::ops::DerefMut;
use std::time::Duration;
use chrono::{Timelike, Utc};
use tracing::{field::display, Level, Span};
use uuid::Uuid;

pub async fn run_worker_until_stopped(configuration: Settings) -> Result<(), anyhow::Error> {
    let connection_pool = get_connection_pool(&configuration.database);

    let email_client = configuration.email_client.client();
    worker_loop(connection_pool, email_client).await
}

async fn worker_loop(pool: PgPool, email_client: EmailClient) -> Result<(), anyhow::Error> {
    loop {
        match try_execute_task(&pool, &email_client).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok(ExecutionOutcome::TaskCompleted) => {}
            Ok(ExecutionOutcome::TaskPostponed) => {}
        }
    }
}

pub enum ExecutionOutcome {
    TaskCompleted,
    TaskPostponed,
    EmptyQueue,
}

#[tracing::instrument(
skip_all,
fields(
newsletter_issue_id = tracing::field::Empty,
subscriber_email = tracing::field::Empty
),
err
)]
pub async fn try_execute_task(
    pool: &PgPool,
    email_client: &EmailClient,
) -> Result<ExecutionOutcome, anyhow::Error> {
    let task = dequeue_task(pool).await?;
    if task.is_none() {
        return Ok(ExecutionOutcome::EmptyQueue);
    }
    let (transaction, issue_delivery_queue) = task.unwrap();

    // tokio::time::sleep(Duration::from_secs(1)).await;
    if let Some(time) = issue_delivery_queue.execute_last_time {
        tracing::event!(
            name: "Execute last time",
            Level::INFO,
            "Execute last time {:?}", time);
        let now = chrono::offset::Utc::now();
        let now_seconds = now.timestamp_micros();
        let time_seconds = time.timestamp_micros()
            + (Duration::from_secs(issue_delivery_queue.execute_after_duration as u64).as_micros() as i64);
        tracing::event!(
            name: "Time seconds",
            Level::INFO,
            ?now_seconds,
            ?time_seconds,
            "Time seconds",
        );
        if time_seconds > now_seconds {
            transaction.commit().await?;
            return Ok(ExecutionOutcome::TaskPostponed);
        }
    }

    Span::current()
        .record("newsletter_issue_id", &display(issue_delivery_queue.newsletter_issue_id))
        .record("subscriber_email", &display(&issue_delivery_queue.subscriber_email));
    let execution_outcome = match SubscriberEmail::parse(issue_delivery_queue.subscriber_email.clone()) {
        Ok(email) => {
            let issue = get_issue(pool, issue_delivery_queue.newsletter_issue_id).await?;
            if let Err(e) = email_client
                .send_email_elastic_mail(
                    &email,
                    &issue.title,
                    &issue.html_content,
                    &issue.text_content,
                )
                .await
            {
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Failed to deliver issue to a confirmed subscriber. \
                    Skipping.\
                    Remaining number of tries: {}",
                    issue_delivery_queue.left_sending_tries
                );
                ExecutionOutcome::TaskPostponed
            } else {
                ExecutionOutcome::TaskCompleted
            }
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skipping a confirmed subscriber. \
                Their stored contact details are invalid.",
            );
            ExecutionOutcome::TaskPostponed
        }
    };

    if let ExecutionOutcome::TaskCompleted = execution_outcome {
        delete_task(
            transaction,
            issue_delivery_queue.newsletter_issue_id,
            &issue_delivery_queue.subscriber_email
        ).await?;
    } else if issue_delivery_queue.left_sending_tries <= 0 {
        delete_task(
            transaction,
            issue_delivery_queue.newsletter_issue_id,
            &issue_delivery_queue.subscriber_email
        ).await?;
    } else {
        let new_tries = issue_delivery_queue.left_sending_tries - 1;
        update_issue_delivery_left_tries(
            transaction,
            issue_delivery_queue.newsletter_issue_id,
            issue_delivery_queue.subscriber_email.as_ref(),
            new_tries
        ).await?;
    }
    Ok(ExecutionOutcome::TaskCompleted)
}

struct NewsletterIssue {
    title: String,
    text_content: String,
    html_content: String,
}

#[tracing::instrument(skip_all)]
async fn get_issue(pool: &PgPool, issue_id: Uuid) -> Result<NewsletterIssue, anyhow::Error> {
    let issue = sqlx::query_as!(
        NewsletterIssue,
        r#"
        SELECT title, text_content, html_content
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id
    )
        .fetch_one(pool)
        .await?;
    Ok(issue)
}

pub type PgTransaction = Transaction<'static, Postgres>;

struct IssueDeliveryQueue {
    newsletter_issue_id: Uuid,
    subscriber_email: String,
    left_sending_tries: i32,
    execute_after_duration: i32,
    execute_last_time: Option<chrono::DateTime<Utc>>,
}

#[tracing::instrument(skip_all)]
async fn dequeue_task(
    pool: &PgPool,
) -> Result<Option<(PgTransaction, IssueDeliveryQueue)>, anyhow::Error> {
    let mut transaction = pool.begin().await?;

    let r = sqlx::query_as!(
        IssueDeliveryQueue,
        r#"
        SELECT
            newsletter_issue_id,
            subscriber_email,
            left_sending_tries,
            execute_after_duration,
            execute_last_time
        FROM
            issue_delivery_queue
        FOR UPDATE
        SKIP LOCKED
        LIMIT 1
        "#,
    )
        .fetch_optional(transaction.deref_mut())
        .await?;

    if let Some(r) = r {
        Ok(Some((
            transaction,
            r,
        )))
    } else {
        Ok(None)
    }
}

#[tracing::instrument(skip_all)]
async fn delete_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
) -> Result<(), anyhow::Error> {
    sqlx::query!(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND
            subscriber_email = $2
        "#,
        issue_id,
        email
    )
        .execute(transaction.deref_mut())
        .await?;
    transaction.commit().await?;
    Ok(())
}

#[tracing::instrument(skip(transaction))]
pub async fn update_issue_delivery_left_tries(
    mut transaction: PgTransaction,
    newsletter_issue_id: Uuid,
    subscriber_email: &str,
    number_of_tries: i32,
) -> Result<(), sqlx::Error> {
    let rows_affected = sqlx::query!(
        r#"
        UPDATE
            issue_delivery_queue
        SET
            left_sending_tries = $1,
            execute_last_time = now()
        WHERE
            newsletter_issue_id = $2 AND
            subscriber_email = $3
        "#,
        number_of_tries,
        newsletter_issue_id,
        subscriber_email
    )
        .execute(transaction.deref_mut())
        .await?
        .rows_affected();
    tracing::event!(
        name: "database",
        Level::INFO,
        "Rows affected {}", rows_affected);
    transaction.commit().await?;
    Ok(())
}

pub async fn get_issue_delivery_left_tries(
    transaction: &mut PgTransaction,
    newsletter_issue_id: Uuid,
    email: &str,
) -> Result<i32, sqlx::Error> {
    let left_tries = sqlx::query!(
        r#"
        SELECT
            left_sending_tries
        FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND
            subscriber_email = $2
        "#,
        newsletter_issue_id,
        email
    )
        .fetch_one(transaction.deref_mut())
        .await?;
    Ok(left_tries.left_sending_tries)
}
