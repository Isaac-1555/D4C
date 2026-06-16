import type { SourceInfo } from "./source-info.ts";

export type SlashCommandSource = "extension" | "prompt" | "skill";

export interface SlashCommandInfo {
	name: string;
	description?: string;
	source: SlashCommandSource;
	sourceInfo: SourceInfo;
}

export interface BuiltinSlashCommand {
	name: string;
	description: string;
}

export const BUILTIN_SLASH_COMMANDS: ReadonlyArray<BuiltinSlashCommand> = [
	{ name: "settings", description: "Open settings menu" },
	{ name: "model", description: "Select model (opens selector UI)" },
	{ name: "hotkeys", description: "Show all keyboard shortcuts" },
	{ name: "login", description: "Configure provider authentication" },
	{ name: "logout", description: "Remove provider authentication" },
	{ name: "new", description: "Start a new session" },
	{ name: "compact", description: "Manually compact the session context" },
	{ name: "resume", description: "Resume a different session" },
	{ name: "reload", description: "Reload keybindings, skills, prompts, and themes" },
	{ name: "plan", description: "Ask clarifying questions and create a build plan" },
	{ name: "build", description: "Execute the plan created by /plan" },
	{ name: "quit", description: process.env.APP_NAME || "d4c" },
	{ name: "mcp", description: "Manage MCP servers (status, connect, setup)" },
	{ name: "mcp-auth", description: "Authenticate with an MCP server (OAuth)" },
	{ name: "websearch", description: "Open web search curator" },
	{ name: "search", description: "Browse stored web search results" },
	{ name: "curator", description: "Toggle web search curator on/off" },
	{ name: "google-account", description: "Show active Google account" },
	{ name: "todos", description: "Manage todo list" },
];
