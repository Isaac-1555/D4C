use anyhow::{Context, Result};
use std::path::Path;

use crate::tools::{PermissionAction, PermissionLevel, PermissionManager, PermissionRequest};
use crate::plan::{Plan, PlanManager, PlanStep};
use crate::provider::{ChatOptions, Message, Provider, ToolCall};

#[derive(Debug, Clone, Default)]
pub struct BuildResult {
    pub step_id: usize,
    pub description: String,
    pub files_touched: Vec<String>,
    pub output: String,
    pub errors: Vec<String>,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct BuildContext {
    pub plan_task: String,
    pub step_index: usize,
    pub total_steps: usize,
    pub previous_output: Vec<String>,
}

pub struct BuildEngine {
    permission_manager: PermissionManager,
}

impl BuildEngine {
    pub fn new() -> Self {
        Self {
            permission_manager: PermissionManager::new(),
        }
    }

    pub fn permission_manager(&self) -> &PermissionManager {
        &self.permission_manager
    }

    pub fn permission_manager_mut(&mut self) -> &mut PermissionManager {
        &mut self.permission_manager
    }

    pub async fn execute_step(
        &mut self,
        provider: &dyn Provider,
        plan: &Plan,
        step: &PlanStep,
        context: &BuildContext,
        system_prompt: &str,
        todo_path: &Path,
    ) -> BuildResult {
        let step_desc = step.description.clone();
        let step_id = step.id;

        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(Message {
                role: "user".into(),
                content: format!(
                    "[SYSTEM INSTRUCTIONS]\n{}\n\nExecute this build step as described below.",
                    system_prompt
                ),
            });
        }
        messages.push(Message {
            role: "user".into(),
            content: format!(
                "Plan: {}\nCurrent step ({} of {}): {}\n\n\
                 Previous steps output:\n{}\n\n\
                 Implement this step. Return shell commands to run and file edits to make.",
                plan.task,
                context.step_index + 1,
                context.total_steps,
                step_desc,
                if context.previous_output.is_empty() {
                    "(none yet)".into()
                } else {
                    context.previous_output.join("\n---\n")
                },
            ),
        });

        let options = ChatOptions::default();

        let mut files = Vec::new();
        let mut output = String::new();
        let mut errors = Vec::new();
        let mut success = true;

        match provider.chat(&messages, &[], &options).await {
            Ok(resp) => {
                if !resp.tool_calls.is_empty() {
                    for tc in &resp.tool_calls {
                        output.push_str(&format!("[tool call: {}]\n", tc.name));
                        let result = self.execute_tool_call(tc);
                        match result {
                            Ok(tool_output) => {
                                output.push_str(&tool_output);
                                output.push('\n');
                                files.extend(extract_files_from_tool(tc));
                            }
                            Err(e) => {
                                errors.push(format!("Tool '{}' failed: {}", tc.name, e));
                                success = false;
                            }
                        }
                    }
                } else if !resp.content.is_empty() {
                    let cmd_result = self.execute_text_commands(&resp.content);
                    match cmd_result {
                        Ok(cmd_output) => {
                            output.push_str(&cmd_output);
                            files.extend(extract_files_from_text(&resp.content));
                        }
                        Err(e) => {
                            errors.push(format!("Command execution failed: {}", e));
                            success = false;
                        }
                    }
                }

                let _ = PlanManager::mark_todo_done(todo_path, step_id);

                BuildResult {
                    step_id,
                    description: step_desc,
                    files_touched: files,
                    output,
                    errors,
                    success,
                }
            }
            Err(e) => BuildResult {
                step_id,
                description: step_desc,
                files_touched: Vec::new(),
                output: String::new(),
                errors: vec![format!("Provider error: {}", e)],
                success: false,
            },
        }
    }

    fn execute_tool_call(&self, tc: &ToolCall) -> Result<String> {
        match tc.name.as_str() {
            "bash" | "shell" | "run" | "execute_command" => {
                let cmd = tc.arguments.get("command")
                    .or_else(|| tc.arguments.get("cmd"))
                    .or_else(|| tc.arguments.get("code"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if cmd.is_empty() {
                    anyhow::bail!("no command found in tool call arguments");
                }
                self.exec_shell(cmd)
            }
            "write" | "create" | "edit" | "write_file" => {
                let path = tc.arguments.get("path")
                    .or_else(|| tc.arguments.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = tc.arguments.get("content")
                    .or_else(|| tc.arguments.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if path.is_empty() {
                    anyhow::bail!("no path in write tool call");
                }
                self.exec_file_write(path, content)
            }
            "read" | "read_file" | "view" => {
                let path = tc.arguments.get("path")
                    .or_else(|| tc.arguments.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if path.is_empty() {
                    anyhow::bail!("no path in read tool call");
                }
                self.exec_file_read(path)
            }
            _ => {
                Ok(format!("Unknown tool call: {}. Arguments: {}", tc.name, tc.arguments))
            }
        }
    }

    fn exec_shell(&self, cmd: &str) -> Result<String> {
        let req = PermissionRequest {
            action: PermissionAction::ShellExec,
            target: cmd.to_string(),
            description: format!("shell: {}", &cmd[..cmd.len().min(80)]),
        };
        let level = self.permission_manager.check_permission(&req);
        if level == PermissionLevel::Deny {
            anyhow::bail!("shell execution denied by permissions: {}", cmd);
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .context("failed to execute shell command")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("[stderr]\n{}", stderr));
        }
        if !output.status.success() {
            anyhow::bail!("command failed (exit {}): {}", output.status, stderr);
        }
        Ok(result)
    }

    fn exec_file_write(&self, path: &str, content: &str) -> Result<String> {
        let req = PermissionRequest {
            action: PermissionAction::FileWrite,
            target: path.to_string(),
            description: format!("write file: {}", path),
        };
        let level = self.permission_manager.check_permission(&req);
        if level == PermissionLevel::Deny {
            anyhow::bail!("file write denied by permissions: {}", path);
        }

        let p = std::path::Path::new(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dirs for {}", path))?;
        }
        std::fs::write(p, content)
            .with_context(|| format!("failed to write {}", path))?;
        Ok(format!("Wrote {} ({} bytes)", path, content.len()))
    }

    fn exec_file_read(&self, path: &str) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path))?;
        Ok(content)
    }

    fn execute_text_commands(&self, text: &str) -> Result<String> {
        let mut output = String::new();
        let mut in_code_block = false;
        let mut current_cmd = String::new();

        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                if in_code_block && !current_cmd.is_empty() {
                    let shell_cmd = current_cmd.trim();
                    if !shell_cmd.is_empty() {
                        match self.exec_shell(shell_cmd) {
                            Ok(cmd_out) => {
                                output.push_str(&format!("$ {}\n{}\n", shell_cmd, cmd_out));
                            }
                            Err(e) => {
                                output.push_str(&format!("$ {}\nError: {}\n", shell_cmd, e));
                            }
                        }
                    }
                    current_cmd.clear();
                }
                in_code_block = !in_code_block;
            } else if in_code_block {
                current_cmd.push_str(line);
                current_cmd.push('\n');
            }
        }

        if output.is_empty() {
            output = text.to_string();
        }
        Ok(output)
    }
}

fn extract_files_from_tool(tc: &ToolCall) -> Vec<String> {
    let mut files = Vec::new();
    if let Some(path) = tc.arguments.get("path")
        .or_else(|| tc.arguments.get("file_path"))
        .and_then(|v| v.as_str())
    {
        files.push(path.to_string());
    }
    if let Some(file) = tc.arguments.get("file")
        .and_then(|v| v.as_str())
    {
        files.push(file.to_string());
    }
    files
}

fn extract_files_from_text(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("$ ") || trimmed.starts_with("```") {
            continue;
        }
        if let Some(path) = extract_path_from_line(trimmed) {
            files.push(path);
        }
    }
    files
}

fn extract_path_from_line(line: &str) -> Option<String> {
    let keywords = ["write ", "edit ", "update ", "create ", "modify "];
    for kw in &keywords {
        if let Some(rest) = line.to_lowercase().strip_prefix(kw) {
            let path = rest.trim().trim_matches('"').trim_matches('\'');
            if !path.is_empty() && !path.contains(' ') {
                return Some(path.to_string());
            }
        }
    }
    None
}
