use super::*;
use dashmap::DashMap;
use std::fmt::Formatter;

#[derive(Clone)]
pub struct ConnectedClient {
    pub id: ClientId,
    pub host: String, // subdomain
    pub domain: String, // root domain
    pub is_anonymous: bool,
    pub wildcard: bool,
    pub tx: UnboundedSender<ControlPacket>,
}

impl ConnectedClient {
    pub fn full_host(&self) -> String {
        format!("{}.{}", self.host, self.domain)
    }
}

impl std::fmt::Debug for ConnectedClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedClient")
            .field("id", &self.id)
            .field("sub", &self.host)
            .field("domain", &self.domain)
            .field("anon", &self.is_anonymous)
            .finish()
    }
}

pub struct Connections {
    clients: Arc<DashMap<ClientId, ConnectedClient>>,
    hosts: Arc<DashMap<String, ConnectedClient>>,
}

impl Connections {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            hosts: Arc::new(DashMap::new()),
        }
    }

    pub fn update_host(client: &ConnectedClient) {
        CONNECTIONS
            .hosts
            .insert(client.full_host(), client.clone());
    }

    pub fn remove(client: &ConnectedClient) {
        client.tx.close_channel();

        // ensure another client isn't using this host
        let full_host = client.full_host();
        if CONNECTIONS
            .hosts
            .get(&full_host)
            .map_or(false, |c| c.id == client.id)
        {
            tracing::debug!("dropping sub-domain: {}", &full_host);
            CONNECTIONS.hosts.remove(&full_host);
        };

        CONNECTIONS.clients.remove(&client.id);
        tracing::debug!("rm client: {}", &client.id);
    }

    pub fn client_for_host(host: &String) -> Option<ClientId> {
        CONNECTIONS.hosts.get(host).map(|c| c.id.clone())
    }

    pub fn get(client_id: &ClientId) -> Option<ConnectedClient> {
        CONNECTIONS
            .clients
            .get(&client_id)
            .map(|c| c.value().clone())
    }

    pub fn find_by_host(host: &String) -> Option<ConnectedClient> {
        CONNECTIONS.hosts.get(host).map(|c| c.value().clone())
    }

    pub fn find_wildcard(domain: &str) -> Option<ConnectedClient> {
        for entry in CONNECTIONS.clients.iter() {
            if entry.value().wildcard && entry.value().domain == domain {
                return Some(entry.value().clone());
            }
        }
        None
    }

    pub fn get_all_clients(&self) -> Vec<ConnectedClient> {
        self.clients.iter().map(|c| c.value().clone()).collect()
    }

    pub fn add(client: ConnectedClient) {
        CONNECTIONS
            .clients
            .insert(client.id.clone(), client.clone());
        CONNECTIONS.hosts.insert(client.full_host(), client);
    }
}
