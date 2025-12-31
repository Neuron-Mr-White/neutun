use std::net::{IpAddr, SocketAddr};
use thiserror::Error;
mod server;
#[allow(unused_imports)]
pub use self::server::spawn;
mod proxy;
pub use self::proxy::proxy_stream;
use crate::network::server::{HostQuery, HostQueryResponse};
use crate::ClientId;
use reqwest::StatusCode;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IOError: {0}")]
    IoError(#[from] std::io::Error),

    #[error("RequestError: {0}")]
    Request(#[from] reqwest::Error),

    #[error("ResolverError: {0}")]
    Resolver(#[from] trust_dns_resolver::error::ResolveError),

    #[error("Does not serve host")]
    DoesNotServeHost,
}

/// An instance of our server
#[derive(Debug, Clone)]
pub struct Instance {
    pub ip: IpAddr,
}

impl Instance {
    /// get all instances where our app runs
    #[allow(dead_code)]
    async fn get_instances() -> Result<Vec<Instance>, Error> {
        tracing::warn!("warning! gossip mode disabled!");
        Ok(vec![])
    }

    /// query the instance and see if it runs our host
    #[allow(dead_code)]
    async fn serves_host(self, host: &str) -> Result<(Instance, ClientId), Error> {
        let addr = SocketAddr::new(self.ip.clone(), crate::CONFIG.internal_network_port);
        let url = format!("http://{}", addr.to_string());
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .timeout(std::time::Duration::from_secs(2))
            .query(&HostQuery {
                host: host.to_string(),
            })
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "failed to send a host query");
                e
            })?;
        let status = response.status();
        let result: HostQueryResponse = response.json().await?;

        let found_client = result
            .client_id
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_default();
        tracing::debug!(status=%status, found=%found_client, "got net svc response");

        match (status, result.client_id) {
            (StatusCode::OK, Some(client_id)) => Ok((self, client_id)),
            _ => Err(Error::DoesNotServeHost),
        }
    }
}

/// get the ip address we need to connect to that runs our host
#[tracing::instrument]
pub async fn instance_for_host(host: &str) -> Result<(Instance, ClientId), Error> {
    // Disabled gossip lookup as Fly.io logic is removed.
    Err(Error::DoesNotServeHost)
}
