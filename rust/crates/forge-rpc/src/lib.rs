//! forge-rpc: generated gRPC types for Forge daemon/runner protocols.

/// Stable crate label used for bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-rpc"
}

pub mod forged {
    pub mod v1 {
        tonic::include_proto!("forged.v1");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::crate_label;
    use super::forged::v1;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-rpc");
    }

    #[test]
    fn forged_service_types_are_available() {
        type Client = v1::forged_service_client::ForgedServiceClient<tonic::transport::Channel>;
        let _ = std::any::type_name::<Client>();
        let _ = v1::SpawnAgentRequest::default();
    }
}
