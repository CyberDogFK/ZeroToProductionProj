use crate::domain::SubscriberEmail;
use secrecy::ExposeSecret;
use secrecy::Secret;
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::ConnectOptions;

#[derive(serde::Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    pub email_client: EmailClientSettings,
}

#[derive(serde::Deserialize, Clone)]
pub struct ApplicationSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct EmailClientSettings {
    pub base_url: String,
    pub sender_email: String,
    pub authorization_token: Secret<String>,
    pub timeout_milliseconds: u64,
}

impl EmailClientSettings {
    pub fn sender(&self) -> Result<SubscriberEmail, String> {
        SubscriberEmail::parse(self.sender_email.clone())
    }

    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.timeout_milliseconds)
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: Secret<String>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

pub fn get_configurations() -> Result<Settings, config::ConfigError> {
    let mut settings = config::Config::default();
    let base_path = std::env::current_dir().expect("Failed to determine current dir");
    let configuration_directory = base_path.join("configuration");
    // read the default config file
    settings.merge(config::File::from(configuration_directory.join("base")).required(true))?;

    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");
    settings.merge(
        config::File::from(configuration_directory.join(environment.as_str())).required(true),
    )?;

    // Add setting from environment, could rewrite previous settings, with APP in the beginning
    // E.g. APP_APPLICATION__PORT=5001
    settings.merge(config::Environment::with_prefix("app").separator("__"))?;

    settings.try_into()
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };

        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(self.password.expose_secret())
            .port(self.port)
            .ssl_mode(ssl_mode)
    }

    pub fn with_db(&self) -> PgConnectOptions {
        self.without_db()
            .database(&self.database_name)
            .log_statements(tracing::log::LevelFilter::Trace)
    }
    // pub fn connection_string(&self) -> Secret<String> {
    //     Secret::new(format!(
    //         "postgres://{}:{}@{}:{}/{}",
    //         self.username,
    //         self.password.expose_secret(),
    //         self.host,
    //         self.port,
    //         self.database_name
    //     ))
    // }
    //
    // pub fn connection_string_without_db(&self) -> Secret<String> {
    //     Secret::new(format!(
    //         "postgres://{}:{}@{}:{}",
    //         self.username,
    //         self.password.expose_secret(),
    //         self.host,
    //         self.port
    //     ))
    // }
}

pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{} is not supported environment. Use either 'local' or 'production'",
                other
            )),
        }
    }
}
