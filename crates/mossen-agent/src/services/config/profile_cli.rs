//! Profile CLI flag handler — handles --list-model-profiles, --get-model-profile, etc.

use serde_json::{json, Value};

use super::facade::{resolve_mossen_config, set_mossen_config_override};
use super::profiles::{
    delete_profile, desensitize_profile, desensitize_profiles, get_active_profile_name,
    get_current_profile, get_default_profile, get_fallback_profile, get_profile_by_name,
    get_profiles, list_all_profiles, migrate_fallback_profile, set_active_profile, set_profile,
    test_profile, validate_profile_name, ProfileProvider, ProfileSchema, ProfileSource,
    PROFILE_PROVIDER_VALUES,
};
use super::types::ConfigOverrideScope;

/// Check if any model profile CLI flag is present.
pub fn is_model_profile_flag_present(args: &[String]) -> bool {
    const FLAGS: &[&str] = &[
        "--list-model-profiles",
        "--get-model-profile",
        "--set-model-profile",
        "--add-model-profile",
        "--update-model-profile",
        "--set-model-profile-key",
        "--delete-model-profile",
        "--test-model-profile",
        "--migrate-fallback-profile",
    ];
    args.iter().any(|arg| FLAGS.contains(&arg.as_str()))
}

fn find_flag(args: &[String], flag: &str) -> Option<usize> {
    args.iter().position(|a| a == flag)
}

fn get_option_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    let next = args.get(idx + 1)?;
    if next.starts_with("--") {
        return None;
    }
    Some(next.as_str())
}

fn parse_scope(args: &[String]) -> ConfigOverrideScope {
    match get_option_value(args, "--scope") {
        Some("project") => ConfigOverrideScope::Project,
        _ => ConfigOverrideScope::User,
    }
}

/// Handle model profile CLI flags. Returns (handled, exit_code).
pub async fn handle_model_profile_cli_flag(args: &[String]) -> (bool, i32) {
    if !is_model_profile_flag_present(args) {
        return (false, 0);
    }

    // --list-model-profiles
    if find_flag(args, "--list-model-profiles").is_some() {
        let settings_profiles = get_profiles();
        let desensitized_settings = desensitize_profiles(&settings_profiles);
        let all = list_all_profiles();
        let fallback = get_fallback_profile();
        let current = get_current_profile();
        let default_p = get_default_profile();
        let all_with_source: Vec<Value> = all
            .iter()
            .map(|item| {
                json!({
                    "name": item.name,
                    "source": item.source,
                    "profile": desensitize_profile(&item.profile),
                })
            })
            .collect();
        let output = json!({
            "profiles": desensitized_settings,
            "activeProfile": get_active_profile_name(),
            "allProfiles": all_with_source,
            "fallbackProfile": fallback.as_ref().map(|fb| json!({
                "name": fb.name,
                "source": fb.source,
                "profile": desensitize_profile(&fb.profile),
            })),
            "currentProfile": current.as_ref().map(|c| json!({
                "name": c.name,
                "source": c.source,
                "profile": desensitize_profile(&c.profile),
            })),
            "defaultProfile": default_p.as_ref().map(|d| json!({
                "name": d.name,
                "source": d.source,
                "profile": desensitize_profile(&d.profile),
            })),
            "count": desensitized_settings.len(),
            "countAll": all_with_source.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
        return (true, 0);
    }

    // --get-model-profile [<name>]
    if let Some(idx) = find_flag(args, "--get-model-profile") {
        let explicit_name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        if let Some(target_name) = explicit_name {
            if let Some(p) = get_profile_by_name(target_name) {
                let output = json!({"name": target_name, "source": "settings", "profile": desensitize_profile(&p)});
                println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                return (true, 0);
            }
            if let Some(fb) = get_fallback_profile() {
                if fb.name == *target_name {
                    let output = json!({"name": target_name, "source": fb.source, "profile": desensitize_profile(&fb.profile)});
                    println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                    return (true, 0);
                }
            }
            eprintln!("error: profile \"{}\" not found", target_name);
            return (true, 1);
        }
        let current = get_current_profile();
        match current {
            None => {
                println!("{}", json!({"name": null, "source": null, "profile": null}));
            }
            Some(c) => {
                let output = json!({"name": c.name, "source": c.source, "profile": desensitize_profile(&c.profile)});
                println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
            }
        }
        return (true, 0);
    }

    let scope = parse_scope(args);

    // --set-model-profile <name>
    if let Some(idx) = find_flag(args, "--set-model-profile") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        match name {
            None => {
                eprintln!("error: --set-model-profile requires a <name> argument");
                return (true, 1);
            }
            Some(name) => match set_active_profile(name, scope) {
                Ok((active, profile, source)) => {
                    let output = json!({
                        "ok": true,
                        "activeProfile": active,
                        "source": source,
                        "profile": desensitize_profile(&profile),
                        "scope": format!("{:?}", scope).to_lowercase(),
                    });
                    println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                    return (true, 0);
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    return (true, 1);
                }
            },
        }
    }

    // --add-model-profile <name>
    if let Some(idx) = find_flag(args, "--add-model-profile") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        match name {
            None => {
                eprintln!("error: --add-model-profile requires a <name> argument");
                return (true, 1);
            }
            Some(name) => {
                if get_profile_by_name(name).is_some() {
                    eprintln!("error: profile \"{}\" already exists; use --update-model-profile to modify", name);
                    return (true, 1);
                }
                let provider = get_option_value(args, "--provider");
                let base_url = get_option_value(args, "--baseURL");
                let model = get_option_value(args, "--model");
                let api_key = get_option_value(args, "--apiKey");
                let display_name = get_option_value(args, "--name");

                let mut missing = Vec::new();
                if provider.is_none() { missing.push("--provider"); }
                if base_url.is_none() { missing.push("--baseURL"); }
                if model.is_none() { missing.push("--model"); }
                if api_key.is_none() { missing.push("--apiKey"); }
                if !missing.is_empty() {
                    eprintln!("error: --add-model-profile missing required: {}", missing.join(", "));
                    return (true, 1);
                }

                let provider = provider.unwrap();
                if !PROFILE_PROVIDER_VALUES.contains(&provider) {
                    eprintln!("error: --provider must be one of {}, got \"{}\"", PROFILE_PROVIDER_VALUES.join("|"), provider);
                    return (true, 1);
                }

                let schema = json!({
                    "provider": provider,
                    "baseURL": base_url.unwrap(),
                    "model": model.unwrap(),
                    "apiKey": api_key.unwrap(),
                    "name": display_name,
                });

                match set_profile(name, &schema, scope) {
                    Ok(_) => {
                        let output = json!({"ok": true, "action": "add", "name": name});
                        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                        return (true, 0);
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return (true, 1);
                    }
                }
            }
        }
    }

    // --update-model-profile <name>
    if let Some(idx) = find_flag(args, "--update-model-profile") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        match name {
            None => {
                eprintln!("error: --update-model-profile requires a <name> argument");
                return (true, 1);
            }
            Some(name) => {
                let existing = match get_profile_by_name(name) {
                    Some(p) => p,
                    None => {
                        eprintln!("error: profile \"{}\" not found; use --add-model-profile to create", name);
                        return (true, 1);
                    }
                };
                let base_url = get_option_value(args, "--baseURL").unwrap_or(&existing.base_url);
                let model = get_option_value(args, "--model").unwrap_or(&existing.model);
                let api_key = get_option_value(args, "--apiKey").unwrap_or(&existing.api_key);
                let display_name = get_option_value(args, "--name").or(existing.name.as_deref());
                let provider_str = get_option_value(args, "--provider").unwrap_or("openai-compatible");

                let schema = json!({
                    "provider": provider_str,
                    "baseURL": base_url,
                    "model": model,
                    "apiKey": api_key,
                    "name": display_name,
                });

                match set_profile(name, &schema, scope) {
                    Ok(_) => {
                        let output = json!({"ok": true, "action": "update", "name": name});
                        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                        return (true, 0);
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return (true, 1);
                    }
                }
            }
        }
    }

    // --set-model-profile-key <name> <key>
    if let Some(idx) = find_flag(args, "--set-model-profile-key") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        let key = args.get(idx + 2).filter(|s| !s.starts_with("--"));
        match (name, key) {
            (Some(name), Some(key)) => {
                let existing = match get_profile_by_name(name) {
                    Some(p) => p,
                    None => {
                        eprintln!("error: profile \"{}\" not found", name);
                        return (true, 1);
                    }
                };
                let updated = json!({
                    "provider": "openai-compatible",
                    "baseURL": existing.base_url,
                    "model": existing.model,
                    "apiKey": key,
                    "name": existing.name,
                });
                match set_profile(name, &updated, scope) {
                    Ok(_) => {
                        let output = json!({"ok": true, "action": "set-key", "name": name});
                        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                        return (true, 0);
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return (true, 1);
                    }
                }
            }
            _ => {
                eprintln!("error: --set-model-profile-key requires <name> <key> arguments");
                return (true, 1);
            }
        }
    }

    // --delete-model-profile <name>
    if let Some(idx) = find_flag(args, "--delete-model-profile") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        match name {
            None => {
                eprintln!("error: --delete-model-profile requires a <name> argument");
                return (true, 1);
            }
            Some(name) => {
                let (deleted, active_cleared, remaining) = delete_profile(name, scope);
                let remaining_names: Vec<&String> = remaining.keys().collect();
                let output = json!({
                    "ok": true,
                    "action": "delete",
                    "name": name,
                    "deleted": deleted,
                    "activeProfileCleared": active_cleared,
                    "remainingProfiles": remaining_names,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                return (true, if deleted { 0 } else { 1 });
            }
        }
    }

    // --migrate-fallback-profile
    if find_flag(args, "--migrate-fallback-profile").is_some() {
        let target_name = get_option_value(args, "--name");
        let force = args.iter().any(|a| a == "--force");
        let activate_raw = get_option_value(args, "--activate");
        let activate = match activate_raw {
            Some("always") => "always",
            Some("never") => "never",
            Some("auto") | None => "auto",
            Some(other) => {
                eprintln!("error: --activate must be one of auto|always|never, got \"{}\"", other);
                return (true, 1);
            }
        };
        let result = migrate_fallback_profile(scope, target_name, force, activate);
        let output = serde_json::to_value(&result).unwrap_or(Value::Null);
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
        return (true, 0);
    }

    // --test-model-profile <name>
    if let Some(idx) = find_flag(args, "--test-model-profile") {
        let name = args.get(idx + 1).filter(|s| !s.starts_with("--"));
        match name {
            None => {
                eprintln!("error: --test-model-profile requires a <name> argument");
                return (true, 1);
            }
            Some(name) => {
                let profile = match get_profile_by_name(name) {
                    Some(p) => p,
                    None => {
                        eprintln!("error: profile \"{}\" not found", name);
                        return (true, 1);
                    }
                };
                let timeout_str = get_option_value(args, "--timeout");
                let timeout_ms = timeout_str.and_then(|s| s.parse::<u64>().ok());
                let result = test_profile(&profile, timeout_ms).await;
                let output = json!({
                    "ok": result.ok,
                    "action": "test",
                    "name": name,
                    "profile": desensitize_profile(&profile),
                    "result": result,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                return (true, if result.ok { 0 } else { 1 });
            }
        }
    }

    (false, 0)
}

/// Handle config CLI flags (--get/set/clear/list-mossen-config).
pub async fn handle_config_cli_flag(args: &[String]) -> (bool, i32) {
    let get_idx = args.iter().position(|a| a == "--get-mossen-config");
    let set_idx = args.iter().position(|a| a == "--set-mossen-config");
    let clear_idx = args.iter().position(|a| a == "--clear-mossen-config");
    let list_idx = args.iter().position(|a| a == "--list-mossen-config");

    if get_idx.is_none() && set_idx.is_none() && clear_idx.is_none() && list_idx.is_none() {
        return (false, 0);
    }

    let scope_idx = args.iter().position(|a| a == "--scope");
    let scope_str = scope_idx
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("override");
    let scope = match scope_str {
        "user" => ConfigOverrideScope::User,
        "project" => ConfigOverrideScope::Project,
        "override" => ConfigOverrideScope::Override,
        other => {
            eprintln!("error: invalid --scope \"{}\"; expected user|project|override", other);
            return (true, 1);
        }
    };

    if let Some(idx) = get_idx {
        let key = match args.get(idx + 1) {
            Some(k) => k,
            None => {
                eprintln!("error: --get-mossen-config requires a key argument");
                return (true, 1);
            }
        };
        let r = resolve_mossen_config(key, Value::Null);
        println!("{}", serde_json::to_string(&r.value).unwrap_or_default());
        return (true, 0);
    }

    if let Some(idx) = set_idx {
        let key = args.get(idx + 1);
        let value_str = args.get(idx + 2);
        match (key, value_str) {
            (Some(k), Some(v)) => {
                let parsed: Value = match serde_json::from_str(v) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("error: --set-mossen-config value must be valid JSON: {}", e);
                        return (true, 1);
                    }
                };
                set_mossen_config_override(k, parsed, scope);
                return (true, 0);
            }
            _ => {
                eprintln!("error: --set-mossen-config requires <key> <value-as-json>");
                return (true, 1);
            }
        }
    }

    if let Some(idx) = clear_idx {
        let key = match args.get(idx + 1) {
            Some(k) => k,
            None => {
                eprintln!("error: --clear-mossen-config requires a key argument");
                return (true, 1);
            }
        };
        super::facade::clear_mossen_config_overrides(scope, Some(key));
        return (true, 0);
    }

    if list_idx.is_some() {
        let all = super::facade::get_all_mossen_config_values();
        println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
        return (true, 0);
    }

    (false, 0)
}

/// TS `type ModelProfileFlag` — variants emitted by the CLI flag parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelProfileFlag {
    /// `--model-profile <name>`
    Named(String),
    /// `--model-profile=current` reuses the active profile.
    UseCurrent,
}
