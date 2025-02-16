use codecs::JsonSerializerConfig;
use futures_util::TryFutureExt;
use snafu::ResultExt;
use vector_core::tls::TlsEnableableConfig;

use crate::{
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    sinks::prelude::*,
};

use super::{sink::NatsSink, ConfigSnafu, ConnectSnafu, NatsError};

/// Configuration for the `nats` sink.
#[configurable_component(sink(
    "nats",
    "Publish observability data to subjects on the NATS messaging system."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NatsSinkConfig {
    #[configurable(derived)]
    pub(super) encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    /// A NATS [name][nats_connection_name] assigned to the NATS connection.
    ///
    /// [nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
    #[serde(default = "default_name", alias = "name")]
    #[configurable(metadata(docs::examples = "foo"))]
    pub(super) connection_name: String,

    /// The NATS [subject][nats_subject] to publish messages to.
    ///
    /// [nats_subject]: https://docs.nats.io/nats-concepts/subjects
    #[configurable(metadata(docs::templateable))]
    #[configurable(metadata(
        docs::examples = "{{ host }}",
        docs::examples = "foo",
        docs::examples = "time.us.east",
        docs::examples = "time.*.east",
        docs::examples = "time.>",
        docs::examples = ">"
    ))]
    pub(super) subject: Template,

    /// The NATS [URL][nats_url] to connect to.
    ///
    /// The URL must take the form of `nats://server:port`.
    /// If the port is not specified it defaults to 4222.
    ///
    /// [nats_url]: https://docs.nats.io/using-nats/developer/connecting#nats-url
    #[configurable(metadata(docs::examples = "nats://demo.nats.io"))]
    #[configurable(metadata(docs::examples = "nats://127.0.0.1:4242"))]
    pub(super) url: String,

    #[configurable(derived)]
    pub(super) tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    pub(super) auth: Option<NatsAuthConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig,
}

fn default_name() -> String {
    String::from("vector")
}

impl GenerateConfig for NatsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            acknowledgements: Default::default(),
            auth: None,
            connection_name: "vector".into(),
            encoding: JsonSerializerConfig::default().into(),
            subject: Template::try_from("from.vector").unwrap(),
            tls: None,
            url: "nats://127.0.0.1:4222".into(),
            request: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = NatsSink::new(self.clone()).await?;
        let healthcheck = healthcheck(self.clone()).boxed();
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl std::convert::TryFrom<&NatsSinkConfig> for async_nats::ConnectOptions {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSinkConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

impl NatsSinkConfig {
    pub(super) async fn connect(&self) -> Result<async_nats::Client, NatsError> {
        let options: async_nats::ConnectOptions = self.try_into().context(ConfigSnafu)?;

        options.connect(&self.url).await.context(ConnectSnafu)
    }
}

async fn healthcheck(config: NatsSinkConfig) -> crate::Result<()> {
    config.connect().map_ok(|_| ()).map_err(|e| e.into()).await
}
