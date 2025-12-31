use super::{AuthResult, AuthService};
use async_trait::async_trait;
use crate::CONFIG;
use crate::connected_clients::Connections;

pub struct SimpleAuthService;

#[async_trait]
impl AuthService for SimpleAuthService {
    type Error = ();
    type AuthKey = String;

    async fn auth_sub_domain(
        &self,
        auth_key: &String,
        subdomain: &str,
    ) -> Result<AuthResult, Self::Error> {
        // 1. Validate Master Key
        if let Some(master_key) = &CONFIG.master_key {
            if auth_key != master_key {
                return Ok(AuthResult::PaymentRequired); // Or a more appropriate "Forbidden" mapping if available
            }
        }

        // 2. Check for collision
        if Connections::find_by_host(&subdomain.to_string()).is_some() {
             return Ok(AuthResult::ReservedByOther);
        }

        Ok(AuthResult::Available)
    }
}
