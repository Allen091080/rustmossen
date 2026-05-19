use super::supported_settings::{get_options_for_setting, SettingSource, SUPPORTED_SETTINGS};

/// Config tool description.
pub const DESCRIPTION: &str = "Get or set Mossen configuration settings.";

/// Generate the full prompt documentation from the settings registry.
pub fn generate_prompt() -> String {
    let mut global_settings: Vec<String> = Vec::new();
    let mut project_settings: Vec<String> = Vec::new();

    for (key, config) in SUPPORTED_SETTINGS.iter() {
        if *key == "model" {
            continue;
        }

        let options = get_options_for_setting(key);
        let mut line = format!("- {}", key);

        if let Some(opts) = options {
            let opts_str: Vec<String> = opts.iter().map(|o| format!("\"{}\"", o)).collect();
            line.push_str(&format!(": {}", opts_str.join(", ")));
        } else if config.setting_type == "boolean" {
            line.push_str(": true/false");
        }

        line.push_str(&format!(" - {}", config.description));

        match config.source {
            SettingSource::Global => global_settings.push(line),
            SettingSource::Settings => project_settings.push(line),
        }
    }

    let model_section = generate_model_section();

    format!(
        "Get or set Mossen configuration settings.\n\n\
         View or change Mossen settings. Use when the user requests configuration changes, \
         asks about current settings, or when adjusting a setting would benefit them.\n\n\n\
         ## Usage\n\
         - **Get current value:** Omit the \"value\" parameter\n\
         - **Set new value:** Include the \"value\" parameter\n\n\
         ## Configurable settings list\n\
         The following settings are available for you to change:\n\n\
         ### Global Settings (stored in the global Mossen config file)\n\
         {global}\n\n\
         ### Project Settings (stored in .mossen/settings.json)\n\
         {project}\n\n\
         {model}\n\
         ## Examples\n\
         - Get theme: {{ \"setting\": \"theme\" }}\n\
         - Set dark theme: {{ \"setting\": \"theme\", \"value\": \"dark\" }}\n\
         - Enable vim mode: {{ \"setting\": \"editorMode\", \"value\": \"vim\" }}\n\
         - Enable verbose: {{ \"setting\": \"verbose\", \"value\": true }}\n\
         - Change model: {{ \"setting\": \"model\", \"value\": \"mossen-opus-4-6\" }}\n\
         - Change permission mode: {{ \"setting\": \"permissions.defaultMode\", \"value\": \"plan\" }}\n",
        global = global_settings.join("\n"),
        project = project_settings.join("\n"),
        model = model_section,
    )
}

fn generate_model_section() -> String {
    let default_models = vec![
        "mossen-sonnet-4-6",
        "mossen-opus-4-6",
        "mossen-haiku-4-5",
    ];
    let lines: Vec<String> = default_models
        .iter()
        .map(|m| format!("  - \"{}\"", m))
        .collect();
    format!(
        "## Model\n- model - Override the default model. Available options:\n{}",
        lines.join("\n")
    )
}
