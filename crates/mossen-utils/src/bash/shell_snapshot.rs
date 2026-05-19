//! Shell environment snapshot creation utilities.
//!
//! Translated from `ShellSnapshot.ts` (583 lines).

use crate::bash::shell_quote::quote;

/// VCS directories to exclude from grep searches.
const VCS_DIRECTORIES_TO_EXCLUDE: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];

/// Creates a shell function that invokes `binary_path` with a specific argv[0].
fn create_argv0_shell_function(
    func_name: &str,
    argv0: &str,
    binary_path: &str,
    prepend_args: &[&str],
) -> String {
    let quoted_path = quote(&[binary_path]);
    let arg_suffix = if !prepend_args.is_empty() {
        format!("{} \"$@\"", prepend_args.join(" "))
    } else {
        "\"$@\"".to_string()
    };

    [
        format!("function {} {{", func_name),
        "  if [[ -n $ZSH_VERSION ]]; then".to_string(),
        format!("    ARGV0={} {} {}", argv0, quoted_path, arg_suffix),
        "  elif [[ \"$OSTYPE\" == \"msys\" ]] || [[ \"$OSTYPE\" == \"cygwin\" ]] || [[ \"$OSTYPE\" == \"win32\" ]]; then".to_string(),
        format!("    ARGV0={} {} {}", argv0, quoted_path, arg_suffix),
        "  elif [[ $BASHPID != $$ ]]; then".to_string(),
        format!("    exec -a {} {} {}", argv0, quoted_path, arg_suffix),
        "  else".to_string(),
        format!("    (exec -a {} {} {})", argv0, quoted_path, arg_suffix),
        "  fi".to_string(),
        "}".to_string(),
    ]
    .join("\n")
}

/// Ripgrep shell integration result.
pub struct RipgrepShellIntegration {
    pub integration_type: RipgrepIntegrationType,
    pub snippet: String,
}

/// Type of ripgrep integration.
pub enum RipgrepIntegrationType {
    Alias,
    Function,
}

/// Creates ripgrep shell integration (alias or function).
pub fn create_ripgrep_shell_integration(
    rg_path: &str,
    rg_args: &[&str],
    argv0: Option<&str>,
) -> RipgrepShellIntegration {
    if let Some(argv0_val) = argv0 {
        return RipgrepShellIntegration {
            integration_type: RipgrepIntegrationType::Function,
            snippet: create_argv0_shell_function("rg", argv0_val, rg_path, &[]),
        };
    }

    let quoted_path = quote(&[rg_path]);
    let quoted_args: Vec<String> = rg_args.iter().map(|arg| quote(&[arg])).collect();
    let alias_target = if !rg_args.is_empty() {
        format!("{} {}", quoted_path, quoted_args.join(" "))
    } else {
        quoted_path
    };

    RipgrepShellIntegration {
        integration_type: RipgrepIntegrationType::Alias,
        snippet: alias_target,
    }
}

/// Creates shell integration for `find` and `grep`.
pub fn create_find_grep_shell_integration(binary_path: &str) -> String {
    let mut exclude_args: Vec<String> = VCS_DIRECTORIES_TO_EXCLUDE
        .iter()
        .map(|d| format!("--exclude-dir={}", d))
        .collect();

    let grep_prepend: Vec<&str> = {
        let mut v = vec!["-G", "--ignore-files", "--hidden", "-I"];
        for ea in &exclude_args {
            v.push(ea.as_str());
        }
        v
    };

    [
        "unalias find 2>/dev/null || true".to_string(),
        "unalias grep 2>/dev/null || true".to_string(),
        create_argv0_shell_function("find", "bfs", binary_path, &["-regextype", "findutils-default"]),
        create_argv0_shell_function(
            "grep",
            "ugrep",
            binary_path,
            &grep_prepend,
        ),
    ]
    .join("\n")
}

/// Get the config file path for a given shell.
pub fn get_config_file(shell_path: &str, home_dir: &str) -> String {
    let file_name = if shell_path.contains("zsh") {
        ".zshrc"
    } else if shell_path.contains("bash") {
        ".bashrc"
    } else {
        ".profile"
    };
    format!("{}/{}", home_dir, file_name)
}

/// Generates user-specific snapshot content (functions, options, aliases).
pub fn get_user_snapshot_content(config_file: &str) -> String {
    let is_zsh = config_file.ends_with(".zshrc");
    let mut content = String::new();

    // User functions
    if is_zsh {
        content.push_str("\
\n      echo \"# Functions\" >> \"$SNAPSHOT_FILE\"\
\n      typeset -f > /dev/null 2>&1\
\n      typeset +f | grep -vE '^_[^_]' | while read func; do\
\n        typeset -f \"$func\" >> \"$SNAPSHOT_FILE\"\
\n      done\n");
    } else {
        content.push_str("\
\n      echo \"# Functions\" >> \"$SNAPSHOT_FILE\"\
\n      declare -f > /dev/null 2>&1\
\n      declare -F | cut -d' ' -f3 | grep -vE '^_[^_]' | while read func; do\
\n        encoded_func=$(declare -f \"$func\" | base64 )\
\n        echo \"eval \\\"\\\\$(echo '$encoded_func' | base64 -d)\\\" > /dev/null 2>&1\" >> \"$SNAPSHOT_FILE\"\
\n      done\n");
    }

    // Shell options
    if is_zsh {
        content.push_str("\
\n      echo \"# Shell Options\" >> \"$SNAPSHOT_FILE\"\
\n      setopt | sed 's/^/setopt /' | head -n 1000 >> \"$SNAPSHOT_FILE\"\n");
    } else {
        content.push_str("\
\n      echo \"# Shell Options\" >> \"$SNAPSHOT_FILE\"\
\n      shopt -p | head -n 1000 >> \"$SNAPSHOT_FILE\"\
\n      set -o | grep \"on\" | awk '{print \"set -o \" $1}' | head -n 1000 >> \"$SNAPSHOT_FILE\"\
\n      echo \"shopt -s expand_aliases\" >> \"$SNAPSHOT_FILE\"\n");
    }

    // User aliases
    content.push_str("\
\n      echo \"# Aliases\" >> \"$SNAPSHOT_FILE\"\
\n      if [[ \"$OSTYPE\" == \"msys\" ]] || [[ \"$OSTYPE\" == \"cygwin\" ]]; then\
\n        alias | grep -v \"='winpty \" | sed 's/^alias //g' | sed 's/^/alias -- /' | head -n 1000 >> \"$SNAPSHOT_FILE\"\
\n      else\
\n        alias | sed 's/^alias //g' | sed 's/^/alias -- /' | head -n 1000 >> \"$SNAPSHOT_FILE\"\
\n      fi\n");

    content
}

/// Generates the snapshot shell script.
pub fn get_snapshot_script(
    shell_path: &str,
    snapshot_file_path: &str,
    config_file_exists: bool,
    home_dir: &str,
    path_value: &str,
    rg_integration: &RipgrepShellIntegration,
    find_grep_integration: Option<&str>,
) -> String {
    let config_file = get_config_file(shell_path, home_dir);
    let is_zsh = config_file.ends_with(".zshrc");

    let user_content = if config_file_exists {
        get_user_snapshot_content(&config_file)
    } else if !is_zsh {
        r#"echo "shopt -s expand_aliases" >> "$SNAPSHOT_FILE""#.to_string()
    } else {
        String::new()
    };

    let source_line = if config_file_exists {
        format!("source \"{}\" < /dev/null", config_file)
    } else {
        "# No user config file to source".to_string()
    };

    // rg integration content
    let rg_content = match rg_integration.integration_type {
        RipgrepIntegrationType::Function => {
            format!(
                "\n      echo \"# Check for rg availability\" >> \"$SNAPSHOT_FILE\"\
                 \n      echo \"if ! (unalias rg 2>/dev/null; command -v rg) >/dev/null 2>&1; then\" >> \"$SNAPSHOT_FILE\"\
                 \n      cat >> \"$SNAPSHOT_FILE\" << 'RIPGREP_FUNC_END'\
                 \n  {}\
                 \nRIPGREP_FUNC_END\
                 \n      echo \"fi\" >> \"$SNAPSHOT_FILE\"\n",
                rg_integration.snippet
            )
        }
        RipgrepIntegrationType::Alias => {
            let escaped_snippet = rg_integration.snippet.replace('\'', "'\\''");
            format!(
                "\n      echo \"# Check for rg availability\" >> \"$SNAPSHOT_FILE\"\
                 \n      echo \"if ! (unalias rg 2>/dev/null; command -v rg) >/dev/null 2>&1; then\" >> \"$SNAPSHOT_FILE\"\
                 \n      echo '  alias rg='\"'{}' >> \"$SNAPSHOT_FILE\"\
                 \n      echo \"fi\" >> \"$SNAPSHOT_FILE\"\n",
                escaped_snippet
            )
        }
    };

    let find_grep_content = match find_grep_integration {
        Some(integration) => format!(
            "\n      echo \"# Shadow find/grep with embedded bfs/ugrep\" >> \"$SNAPSHOT_FILE\"\
             \n      cat >> \"$SNAPSHOT_FILE\" << 'FIND_GREP_FUNC_END'\
             \n{}\
             \nFIND_GREP_FUNC_END\n",
            integration
        ),
        None => String::new(),
    };

    format!(
        "SNAPSHOT_FILE={}\n      {}\n\n\
         \n      # First, create/clear the snapshot file\
         \n      echo \"# Snapshot file\" >| \"$SNAPSHOT_FILE\"\
         \n\
         \n      # Unset all aliases to avoid conflicts\
         \n      echo \"# Unset all aliases to avoid conflicts with functions\" >> \"$SNAPSHOT_FILE\"\
         \n      echo \"unalias -a 2>/dev/null || true\" >> \"$SNAPSHOT_FILE\"\
         \n\
         \n      {}\
         \n\
         \n      {}\
         \n\
         \n      {}\
         \n\
         \n      # Add PATH to the file\
         \n      echo \"export PATH={}\" >> \"$SNAPSHOT_FILE\"\
         \n\
         \n      # Exit check\
         \n      if [ ! -f \"$SNAPSHOT_FILE\" ]; then\
         \n        echo \"Error: Snapshot file was not created at $SNAPSHOT_FILE\" >&2\
         \n        exit 1\
         \n      fi\n",
        quote(&[snapshot_file_path]),
        source_line,
        user_content,
        rg_content,
        find_grep_content,
        quote(&[path_value]),
    )
}

/// 对应 TS `createAndSaveSnapshot`：构造 snapshot 脚本并写入临时文件，返回路径。
///
/// 调用方传入 snapshot 脚本所需的所有内容；本函数负责拼装脚本骨架并落盘。
pub async fn create_and_save_snapshot(
    snapshot_file_path: &str,
    user_content: &str,
    rg_content: &str,
    find_grep_content: &str,
    path_value: &str,
) -> std::io::Result<String> {
    let body = build_snapshot_script(
        snapshot_file_path,
        "",
        user_content,
        rg_content,
        find_grep_content,
        path_value,
    );
    tokio::fs::write(snapshot_file_path, body.as_bytes()).await?;
    Ok(snapshot_file_path.to_string())
}

fn build_snapshot_script(
    snapshot_file_path: &str,
    source_line: &str,
    user_content: &str,
    rg_content: &str,
    find_grep_content: &str,
    path_value: &str,
) -> String {
    format!(
        "#!/usr/bin/env bash\n# auto-generated by Mossen\n\
         snapshot_file={}\n\
         {}\n\
         {}\n\
         {}\n\
         {}\n\
         export PATH={}\n",
        quote(&[snapshot_file_path]),
        source_line,
        user_content,
        rg_content,
        find_grep_content,
        quote(&[path_value]),
    )
}
