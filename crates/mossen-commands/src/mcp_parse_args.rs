//! MCP subcommand parsers module
//!
//! This module provides parsers for MCP subcommands.

/// Parse add command arguments
pub fn parse_add_args(args: &[&str]) -> AddArgs {
    let mut name: Option<String> = None;
    let mut command_or_url: Option<String> = None;
    let mut extra_args: Vec<String> = Vec::new();
    let mut transport: Option<String> = None;
    let mut scope: Option<String> = None;
    let mut env: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut client_id: Option<String> = None;
    let mut client_secret = false;
    let mut callback_port: Option<u16> = None;
    let mut xaa = false;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-s" | "--scope" if i + 1 < args.len() => {
                scope = Some(args[i + 1].to_string());
                i += 2;
            }
            "-t" | "--transport" if i + 1 < args.len() => {
                transport = Some(args[i + 1].to_string());
                i += 2;
            }
            "-e" | "--env" if i + 1 < args.len() => {
                env.push(args[i + 1].to_string());
                i += 2;
            }
            "-H" | "--header" if i + 1 < args.len() => {
                headers.push(args[i + 1].to_string());
                i += 2;
            }
            "--client-id" if i + 1 < args.len() => {
                client_id = Some(args[i + 1].to_string());
                i += 2;
            }
            "--client-secret" => {
                client_secret = true;
                i += 1;
            }
            "--callback-port" if i + 1 < args.len() => {
                callback_port = args[i + 1].parse().ok();
                i += 2;
            }
            "--xaa" => {
                xaa = true;
                i += 1;
            }
            _ => {
                if name.is_none() {
                    name = Some(args[i].to_string());
                } else if command_or_url.is_none() {
                    command_or_url = Some(args[i].to_string());
                } else {
                    extra_args.push(args[i].to_string());
                }
                i += 1;
            }
        }
    }

    AddArgs {
        name,
        command_or_url,
        extra_args,
        transport,
        scope,
        env,
        headers,
        client_id,
        client_secret,
        callback_port,
        xaa,
    }
}

/// Arguments for MCP add command
#[derive(Debug, Default)]
pub struct AddArgs {
    pub name: Option<String>,
    pub command_or_url: Option<String>,
    pub extra_args: Vec<String>,
    pub transport: Option<String>,
    pub scope: Option<String>,
    pub env: Vec<String>,
    pub headers: Vec<String>,
    pub client_id: Option<String>,
    pub client_secret: bool,
    pub callback_port: Option<u16>,
    pub xaa: bool,
}

/// Parse install command arguments
pub fn parse_install_args(args: &[&str]) -> InstallArgs {
    let mut scope: Option<String> = None;
    let mut target: Option<String> = None;
    let mut confirm = false;
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--scope" | "-s" if i + 1 < args.len() => {
                scope = Some(args[i + 1].to_string());
                i += 2;
            }
            "--confirm" => {
                confirm = true;
                i += 1;
            }
            "--dry-run" | "-n" => {
                dry_run = true;
                i += 1;
            }
            _ if target.is_none() => {
                target = Some(args[i].to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    InstallArgs {
        scope,
        target,
        confirm,
        dry_run,
    }
}

/// Arguments for MCP install command
#[derive(Debug, Default)]
pub struct InstallArgs {
    pub scope: Option<String>,
    pub target: Option<String>,
    pub confirm: bool,
    pub dry_run: bool,
}

/// Parse template command arguments
pub fn parse_template_args(args: &[&str]) -> TemplateArgs {
    let mut template: Option<String> = None;
    let mut scope: Option<String> = None;
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--scope" | "-s" if i + 1 < args.len() => {
                scope = Some(args[i + 1].to_string());
                i += 2;
            }
            "--dry-run" | "-n" => {
                dry_run = true;
                i += 1;
            }
            _ if template.is_none() => {
                template = Some(args[i].to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    TemplateArgs {
        template,
        scope,
        dry_run,
    }
}

/// Arguments for MCP template command
#[derive(Debug, Default)]
pub struct TemplateArgs {
    pub template: Option<String>,
    pub scope: Option<String>,
    pub dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_add_args_basic() {
        let args = vec!["my-server", "npx", "my-mcp-server"];
        let result = parse_add_args(&args);
        assert_eq!(result.name, Some("my-server".to_string()));
        assert_eq!(result.command_or_url, Some("npx".to_string()));
        assert_eq!(result.extra_args, vec!["my-mcp-server".to_string()]);
    }

    #[test]
    fn test_parse_add_args_with_transport() {
        let args = vec![
            "my-server",
            "https://example.com/mcp",
            "--transport",
            "http",
        ];
        let result = parse_add_args(&args);
        assert_eq!(result.name, Some("my-server".to_string()));
        assert_eq!(
            result.command_or_url,
            Some("https://example.com/mcp".to_string())
        );
        assert_eq!(result.transport, Some("http".to_string()));
    }

    #[test]
    fn test_parse_install_args() {
        let args = vec!["--scope", "user", "--dry-run"];
        let result = parse_install_args(&args);
        assert_eq!(result.scope, Some("user".to_string()));
        assert!(result.dry_run);
    }

    #[test]
    fn test_parse_template_args() {
        let args = vec!["github", "--scope", "local"];
        let result = parse_template_args(&args);
        assert_eq!(result.template, Some("github".to_string()));
        assert_eq!(result.scope, Some("local".to_string()));
    }
}
