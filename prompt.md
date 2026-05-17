# Introduction

I want to build an MCP for the Gitlab API.  It should have a similar structure to netbox-mcp in ~/source/repos/netbox-mcp.  Unlike netbox-mcp, I want this MCP to be capable of all CRUD operations.  You should follow the netbox-mcp project as an example.

# Language

It should be written in Rust, the same as netbox-mcp.

# Scope

To limit the scope of the initial implementation, I want to just focus on covering the Issues API first (See reference)

# Libraries

- Use the same MCP library as netbox-mcp
- Consider using https://docs.gitlab.com/api/rest/third_party_clients/#rust for Gitlab

# References

- https://docs.gitlab.com/api/rest/
- https://docs.rs/gitlab/0.1811.0/gitlab/
- https://docs.gitlab.com/api/issues/