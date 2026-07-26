use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct McpManager {
    servers: HashMap<String, ManagedServer>,
}

struct ManagedServer {
    config: McpServerConfig,
    child: Option<Child>,
    tools: Vec<McpTool>,
    connected: bool,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.insert(
            config.name.clone(),
            ManagedServer {
                config,
                child: None,
                tools: Vec::new(),
                connected: false,
            },
        );
    }

    pub fn list_servers(&self) -> Vec<(&str, bool)> {
        self.servers
            .iter()
            .map(|(name, server)| (name.as_str(), server.connected))
            .collect()
    }

    pub fn connect(&mut self, name: &str) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", name))?;

        if server.connected {
            return Ok(());
        }

        let child = Command::new(&server.config.command)
            .args(&server.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        server.child = Some(child);
        server.connected = true;

        tracing::info!("Connected to MCP server: {}", name);
        Ok(())
    }

    pub fn disconnect(&mut self, name: &str) -> Result<()> {
        if let Some(server) = self.servers.get_mut(name) {
            if let Some(mut child) = server.child.take() {
                let _ = child.kill();
            }
            server.connected = false;
            server.tools.clear();
            tracing::info!("Disconnected from MCP server: {}", name);
        }
        Ok(())
    }

    pub fn get_tools(&self, name: &str) -> Vec<McpTool> {
        self.servers
            .get(name)
            .map(|s| s.tools.clone())
            .unwrap_or_default()
    }

    pub fn all_tools(&self) -> Vec<McpTool> {
        self.servers
            .values()
            .filter(|s| s.connected)
            .flat_map(|s| s.tools.clone())
            .collect()
    }
}

impl Drop for McpManager {
    fn drop(&mut self) {
        let names: Vec<String> = self.servers.keys().cloned().collect();
        for name in names {
            if let Some(server) = self.servers.get_mut(&name) {
                if let Some(mut child) = server.child.take() {
                    let _ = child.kill();
                }
            }
        }
    }
}
