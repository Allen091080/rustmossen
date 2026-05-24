//! # GitHub App (github-app.ts)
//!
//! GitHub App 相关常量、类型和工作流模板函数。

pub const OFFICIAL_GITHUB_ACTION_USAGE_DOCS_URL: &str =
    "https://github.com/mossen/mossen-action/blob/main/docs/usage.md";
pub const OFFICIAL_GITHUB_WORKFLOW_EXAMPLES_URL: &str =
    "https://github.com/mossen/mossen-action/blob/main/examples/";
pub const CUSTOM_BACKEND_GITHUB_CREDENTIAL_PLACEHOLDER: &str = "__MOSSEN_CODE_CUSTOM_CREDENTIAL__";

pub fn get_workflow_mention_handle(is_custom_backend: bool) -> &'static str {
    if is_custom_backend {
        "@assistant"
    } else {
        "@mossen"
    }
}

pub fn get_workflow_display_name(is_custom_backend: bool, product_name: &str) -> String {
    if is_custom_backend {
        product_name.to_string()
    } else {
        "Mossen".to_string()
    }
}

pub fn get_review_workflow_display_name(is_custom_backend: bool, product_name: &str) -> String {
    if is_custom_backend {
        format!("{} Review", product_name)
    } else {
        "Mossen Review".to_string()
    }
}

pub fn get_github_action_usage_docs_url(is_custom_backend: bool, remote_base_url: &str) -> String {
    if !is_custom_backend {
        return OFFICIAL_GITHUB_ACTION_USAGE_DOCS_URL.to_string();
    }
    format!("{}/docs/github-actions", remote_base_url)
}

pub fn get_github_workflow_examples_url(
    is_custom_backend: bool,
    github_actions_docs_url: &str,
) -> String {
    if !is_custom_backend {
        return OFFICIAL_GITHUB_WORKFLOW_EXAMPLES_URL.to_string();
    }
    format!("{}#examples", github_actions_docs_url)
}

pub fn get_cli_reference_docs_url(remote_base_url: &str) -> String {
    format!("{}/docs/cli-reference", remote_base_url)
}

pub fn get_github_action_bootstrap_url(
    env_override: Option<&str>,
    remote_base_url: &str,
) -> String {
    if let Some(configured) = env_override {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    format!("{}/integrations/github/bootstrap.sh", remote_base_url)
}

/// GitHub workflow readiness check result.
#[derive(Debug, Clone)]
pub struct GitHubWorkflowReadiness {
    pub bootstrap_url: String,
    pub issues: Vec<String>,
    pub ready: bool,
}

/// Check GitHub workflow readiness.
/// `is_placeholder`: a function that checks if a URL is a placeholder.
pub fn get_github_workflow_readiness(
    remote_base_url: &str,
    github_app_url: &str,
    github_actions_docs_url: &str,
    bootstrap_url: &str,
    is_placeholder: impl Fn(&str) -> bool,
) -> GitHubWorkflowReadiness {
    let mut issues = Vec::new();

    if is_placeholder(remote_base_url) {
        issues.push("Hosted platform base URL still points to a placeholder domain.".to_string());
    }
    if is_placeholder(github_app_url) {
        issues.push("GitHub app install URL still points to a placeholder domain.".to_string());
    }
    if is_placeholder(github_actions_docs_url) {
        issues.push("GitHub workflow docs URL still points to a placeholder domain.".to_string());
    }
    if is_placeholder(bootstrap_url) {
        issues.push(
            "GitHub workflow bootstrap runner URL still points to a placeholder domain."
                .to_string(),
        );
    }

    let ready = issues.is_empty();
    GitHubWorkflowReadiness {
        bootstrap_url: bootstrap_url.to_string(),
        issues,
        ready,
    }
}

pub fn get_pr_title(is_custom_backend: bool, product_name: &str) -> String {
    if is_custom_backend {
        format!("Add {} workflow", product_name)
    } else {
        "Add hosted code workflow".to_string()
    }
}

pub fn get_github_app_install_url(github_app_url: &str) -> String {
    github_app_url.to_string()
}

pub fn get_github_actions_setup_docs_url(github_actions_docs_url: &str) -> String {
    github_actions_docs_url.to_string()
}

pub fn get_primary_github_workflow_path(is_custom_backend: bool) -> &'static str {
    if is_custom_backend {
        ".github/workflows/coding-assistant.yml"
    } else {
        ".github/workflows/mossen.yml"
    }
}

pub fn get_review_github_workflow_path(is_custom_backend: bool) -> &'static str {
    if is_custom_backend {
        ".github/workflows/coding-assistant-review.yml"
    } else {
        ".github/workflows/mossen-review.yml"
    }
}

pub fn get_workflow_search_paths(is_custom_backend: bool) -> Vec<&'static str> {
    vec![
        get_primary_github_workflow_path(is_custom_backend),
        get_review_github_workflow_path(is_custom_backend),
    ]
}

pub fn get_default_github_actions_secret_name(
    is_custom_backend: bool,
    has_auth_token: bool,
    has_api_key: bool,
) -> &'static str {
    if !is_custom_backend {
        return "MOSSEN_CODE_API_KEY";
    }
    if has_auth_token && !has_api_key {
        return "MOSSEN_CODE_CUSTOM_AUTH_TOKEN";
    }
    "MOSSEN_CODE_CUSTOM_API_KEY"
}

/// Build GitHub workflow runner step YAML for the given kind.
fn get_github_workflow_runner_step(
    kind: &str,
    is_custom_backend: bool,
    product_name: &str,
    action_usage_docs_url: &str,
    cli_reference_docs_url: &str,
) -> String {
    if !is_custom_backend {
        let actor_name = if kind == "review" {
            get_review_workflow_display_name(false, product_name)
        } else {
            get_workflow_display_name(false, product_name)
        };
        let actor_id = if kind == "review" {
            "mossen-review"
        } else {
            "mossen"
        };

        let review_only_inputs = if kind == "review" {
            "          plugin_marketplaces: 'https://github.com/mossen/mossen.git'\n          plugins: 'code-review@mossen-plugins'\n          prompt: '/code-review:code-review ${{ github.repository }}/pull/${{ github.event.pull_request.number }}'\n"
        } else {
            ""
        };

        let comment_only_inputs = if kind == "comment" {
            "          # This is an optional setting that allows Mossen to read CI results on PRs\n          additional_permissions: |\n            actions: read\n\n          # Optional: Give a custom prompt to Mossen. If this is not specified, Mossen will perform the instructions specified in the comment that tagged it.\n          # prompt: 'Update the pull request description to include a summary of changes.'\n\n          # Optional: Add mossen_args to customize behavior and configuration\n"
        } else {
            ""
        };

        return format!(
            "      - name: Run {actor_name}\n        id: {actor_id}\n        uses: mossen/mossen-action@v1\n        with:\n          mossen_code_api_key: ${{{{ secrets.MOSSEN_CODE_API_KEY }}}}\n{review_only}{comment_only}          # See {docs}\n          # or {cli} for available options\n",
            actor_name = actor_name,
            actor_id = actor_id,
            review_only = review_only_inputs,
            comment_only = comment_only_inputs,
            docs = action_usage_docs_url,
            cli = cli_reference_docs_url,
        );
    }

    let workflow_kind = if kind == "review" {
        "review"
    } else {
        "comment"
    };
    let actor_name = if kind == "review" {
        get_review_workflow_display_name(true, product_name)
    } else {
        get_workflow_display_name(true, product_name)
    };

    format!(
        r#"      - name: Run {actor_name}
        env:
          GITHUB_TOKEN: ${{{{ secrets.GITHUB_TOKEN }}}}
          MOSSEN_CODE_USE_CUSTOM_BACKEND: '1'
          MOSSEN_CODE_SUBPROCESS_ENV_SCRUB: '1'
          MOSSEN_CODE_CUSTOM_BASE_URL: ${{{{ vars.MOSSEN_CODE_CUSTOM_BASE_URL }}}}
          MOSSEN_CODE_CUSTOM_MODEL: ${{{{ vars.MOSSEN_CODE_CUSTOM_MODEL }}}}
          MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL: ${{{{ vars.MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL }}}}
          MOSSEN_CODE_CUSTOM_NAME: ${{{{ vars.MOSSEN_CODE_CUSTOM_NAME }}}}
          MOSSEN_CODE_PLATFORM_BASE_URL: ${{{{ vars.MOSSEN_CODE_PLATFORM_BASE_URL }}}}
          MOSSEN_CODE_GITHUB_RUNNER_URL: ${{{{ vars.MOSSEN_CODE_GITHUB_ACTION_BOOTSTRAP_URL }}}}
          MOSSEN_CODE_CUSTOM_HEADERS: ${{{{ secrets.MOSSEN_CODE_CUSTOM_HEADERS }}}}
          {credential_placeholder}
          MOSSEN_CODE_GITHUB_WORKFLOW_KIND: '{workflow_kind}'
        run: |
          curl -fsSL "$MOSSEN_CODE_GITHUB_RUNNER_URL" -o /tmp/run-coding-assistant-github.sh
          bash /tmp/run-coding-assistant-github.sh

          # See {docs}
          # or {cli} for available options
"#,
        actor_name = actor_name,
        credential_placeholder = CUSTOM_BACKEND_GITHUB_CREDENTIAL_PLACEHOLDER,
        workflow_kind = workflow_kind,
        docs = action_usage_docs_url,
        cli = cli_reference_docs_url,
    )
}

/// Generate the primary workflow YAML content.
pub fn workflow_content(
    is_custom_backend: bool,
    product_name: &str,
    remote_base_url: &str,
    _github_actions_docs_url: &str,
) -> String {
    let display_name = get_workflow_display_name(is_custom_backend, product_name);
    let mention = get_workflow_mention_handle(is_custom_backend);
    let job_name = if is_custom_backend {
        "assistant"
    } else {
        "mossen"
    };
    let actor_label = if is_custom_backend {
        "the assistant"
    } else {
        "Mossen"
    };
    let action_docs = get_github_action_usage_docs_url(is_custom_backend, remote_base_url);
    let cli_docs = get_cli_reference_docs_url(remote_base_url);
    let runner_step = get_github_workflow_runner_step(
        "comment",
        is_custom_backend,
        product_name,
        &action_docs,
        &cli_docs,
    );

    format!(
        r#"name: {display_name}

on:
  issue_comment:
    types: [created]
  pull_request_review_comment:
    types: [created]
  issues:
    types: [opened, assigned]
  pull_request_review:
    types: [submitted]

jobs:
  {job_name}:
    if: |
      (github.event_name == 'issue_comment' && contains(github.event.comment.body, '{mention}')) ||
      (github.event_name == 'pull_request_review_comment' && contains(github.event.comment.body, '{mention}')) ||
      (github.event_name == 'pull_request_review' && contains(github.event.review.body, '{mention}')) ||
      (github.event_name == 'issues' && (contains(github.event.issue.body, '{mention}') || contains(github.event.issue.title, '{mention}')))
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
      issues: read
      id-token: write
      actions: read # Required for {actor_label} to read CI results on PRs
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 1

{runner_step}
"#,
        display_name = display_name,
        job_name = job_name,
        mention = mention,
        actor_label = actor_label,
        runner_step = runner_step,
    )
}

/// Generate the PR body markdown.
pub fn pr_body(
    is_custom_backend: bool,
    product_name: &str,
    github_actions_setup_docs_url: &str,
) -> String {
    let mention = get_workflow_mention_handle(is_custom_backend);
    let workflow_label = if is_custom_backend {
        format!("{} workflow", product_name)
    } else {
        "hosted code workflow".to_string()
    };
    let integration_label = if is_custom_backend {
        format!("{} integration", product_name)
    } else {
        "hosted coding integration".to_string()
    };
    let runtime_label = if is_custom_backend {
        format!("{} runtime", product_name)
    } else {
        "hosted coding runtime".to_string()
    };
    let runtime_short = if is_custom_backend {
        format!("{} runtime", product_name)
    } else {
        "hosted runtime".to_string()
    };
    let backend_label = if is_custom_backend {
        "platform backend"
    } else {
        "hosted backend"
    };

    format!(
        r#"## 🤖 Installing the {workflow_label}

This PR adds a GitHub Actions workflow that enables the {integration_label} for this repository.

### What does this workflow do?

The {workflow_label} can help with:
- Bug fixes and improvements  
- Documentation updates
- Implementing new features
- Code reviews and suggestions
- Writing tests
- And more!

### How it works

Once this PR is merged, we'll be able to interact with the workflow by mentioning {mention} in a pull request or issue comment.
Once the workflow is triggered, the {runtime_label} will analyze the comment and surrounding context, and execute on the request in a GitHub action.

### Important Notes

- **This workflow won't take effect until this PR is merged**
- **{mention} mentions won't work until after the merge is complete**
- The workflow runs automatically whenever {mention} is mentioned in PR or issue comments
- The {runtime_short} gets access to the entire PR or issue context including files, diffs, and previous comments

### Security

- Our {backend_label} credential is securely stored as a GitHub Actions secret
- Only users with write access to the repository can trigger the workflow
- All workflow runs are stored in the GitHub Actions run history
- The coding runtime's default tools are limited to reading/writing files and interacting with our repo by creating comments, branches, and commits.
- We can add more allowed tools by adding them to the workflow file like:

```
allowed_tools: Bash(npm install),Bash(npm run build),Bash(npm run lint),Bash(npm run test)
```

There's more information in the [workflow setup guide]({docs}).

After merging this PR, let's try mentioning {mention} in a comment on any PR to get started!"#,
        workflow_label = workflow_label,
        integration_label = integration_label,
        mention = mention,
        runtime_label = runtime_label,
        runtime_short = runtime_short,
        backend_label = backend_label,
        docs = github_actions_setup_docs_url,
    )
}

/// Generate the code review plugin workflow YAML content.
pub fn code_review_plugin_workflow_content(
    is_custom_backend: bool,
    product_name: &str,
    remote_base_url: &str,
) -> String {
    let review_display = get_review_workflow_display_name(is_custom_backend, product_name);
    let job_name = if is_custom_backend {
        "assistant-review"
    } else {
        "mossen-review"
    };
    let action_docs = get_github_action_usage_docs_url(is_custom_backend, remote_base_url);
    let cli_docs = get_cli_reference_docs_url(remote_base_url);
    let runner_step = get_github_workflow_runner_step(
        "review",
        is_custom_backend,
        product_name,
        &action_docs,
        &cli_docs,
    );

    format!(
        r#"name: {review_display}

on:
  pull_request:
    types: [opened, synchronize, ready_for_review, reopened]
    # Optional: Only run on specific file changes
    # paths:
    #   - "src/**/*.ts"
    #   - "src/**/*.tsx"
    #   - "src/**/*.js"
    #   - "src/**/*.jsx"

jobs:
  {job_name}:
    # Optional: Filter by PR author
    # if: |
    #   github.event.pull_request.user.login == 'external-contributor' ||
    #   github.event.pull_request.user.login == 'new-developer' ||
    #   github.event.pull_request.author_association == 'FIRST_TIME_CONTRIBUTOR'

    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
      issues: read
      id-token: write

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 1

{runner_step}
"#,
        review_display = review_display,
        job_name = job_name,
        runner_step = runner_step,
    )
}
