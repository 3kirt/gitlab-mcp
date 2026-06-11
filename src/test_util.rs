//! Shared fixtures for the wiremock unit-test suites.

use wiremock::MockServer;

use crate::client::GitlabClient;

/// A `GitlabClient` pointed at a local wiremock server.
pub(crate) fn mock_client(server: &MockServer) -> GitlabClient {
    GitlabClient::new(server.uri(), "test-token").unwrap()
}
