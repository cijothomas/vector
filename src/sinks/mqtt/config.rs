use std::time::Duration;

use codecs::JsonSerializerConfig;
use rumqttc::{MqttOptions, Transport, TlsConfiguration};
use snafu::ResultExt;
use vector_config::configurable_component;

use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        mqtt::sink::{TlsSnafu, MqttConnector, MqttError, MqttSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

/// Configuration for the `mqtt` sink
#[configurable_component(sink("mqtt"))]
#[derive(Clone, Debug)]
pub struct MqttSinkConfig {
    /// MQTT server address
    pub host: String,

    /// TCP port
    #[serde(default = "default_port")]
    pub port: u16,

    /// MQTT username
    pub user: Option<String>,

    /// MQTT password
    pub password: Option<String>,

    /// MQTT client ID
    #[serde(default = "default_client_id")]
    pub client_id: String,

    /// Connection keep-alive interval
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u16,

    /// Clean MQTT session on login?
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,

    /// TLS configuration
    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// MQTT publish topic (templates allowed)
    pub topic: String,

    /// Encoding configuration
    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    pub acknowledgements: AcknowledgementsConfig,
}

const fn default_port() -> u16 {
    1883
}

fn default_client_id() -> String {
    "vector".into()
}

const fn default_keep_alive() -> u16 {
    60
}

const fn default_clean_session() -> bool {
    false
}

impl GenerateConfig for MqttSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            host: "localhost".into(),
            port: default_port(),
            user: None,
            password: None,
            client_id: default_client_id(),
            keep_alive: default_keep_alive(),
            clean_session: default_clean_session(),
            tls: None,
            topic: "vector".into(),
            encoding: JsonSerializerConfig::new().into(),
            acknowledgements: AcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for MqttSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = self.build_connector()?;
        let sink = MqttSink::new(self, connector.clone())?;

        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl MqttSinkConfig {
    fn build_connector(&self) -> Result<MqttConnector, MqttError> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false)
            .context(TlsSnafu)?;
        let mut options = MqttOptions::new(&self.client_id, &self.host, self.port);
        options.set_keep_alive(Duration::from_secs(self.keep_alive.into()));
        options.set_clean_session(self.clean_session);
        if let (Some(user), Some(password)) = (&self.user, &self.password) {
            options.set_credentials(user, password);
        }
        if let Some(tls) = tls.tls() {
            let ca = tls.authorities_pem().flatten().collect();
            let client_auth = None;
            let alpn = Some(vec!["mqtt".into()]);
            options.set_transport(Transport::Tls(TlsConfiguration::Simple {
                ca, client_auth, alpn
            }));
        }
        MqttConnector::new(options, self.topic.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSinkConfig>();
    }
}
