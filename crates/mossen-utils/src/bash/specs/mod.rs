//! Built-in command specs for bash completion and prefix detection.
//!
//! Translated from `specs/index.ts` and individual spec files.

use super::registry::{Argument, CommandSpec, SpecOption};

/// Get all built-in command specs.
pub fn get_all_specs() -> Vec<CommandSpec> {
    vec![
        pyright_spec(),
        timeout_spec(),
        sleep_spec(),
        alias_spec(),
        nohup_spec(),
        time_spec(),
        srun_spec(),
    ]
}

fn pyright_spec() -> CommandSpec {
    CommandSpec {
        name: "pyright".to_string(),
        description: Some("Type checker for Python".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("files".to_string()),
            description: Some("Specify files or directories to analyze (overrides config file)".to_string()),
            is_variadic: true,
            is_optional: true,
            ..Default::default()
        }],
        options: vec![
            SpecOption { names: vec!["--help".into(), "-h".into()], description: Some("Show help message".into()), ..Default::default() },
            SpecOption { names: vec!["--version".into()], description: Some("Print pyright version and exit".into()), ..Default::default() },
            SpecOption { names: vec!["--watch".into(), "-w".into()], description: Some("Continue to run and watch for changes".into()), ..Default::default() },
            SpecOption { names: vec!["--project".into(), "-p".into()], description: Some("Use the configuration file at this location".into()), args: vec![Argument { name: Some("FILE OR DIRECTORY".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["-".into()], description: Some("Read file or directory list from stdin".into()), ..Default::default() },
            SpecOption { names: vec!["--createstub".into()], description: Some("Create type stub file(s) for import".into()), args: vec![Argument { name: Some("IMPORT".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--typeshedpath".into(), "-t".into()], description: Some("Use typeshed type stubs at this location".into()), args: vec![Argument { name: Some("DIRECTORY".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--verifytypes".into()], description: Some("Verify completeness of types in py.typed package".into()), args: vec![Argument { name: Some("IMPORT".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--ignoreexternal".into()], description: Some("Ignore external imports for --verifytypes".into()), ..Default::default() },
            SpecOption { names: vec!["--pythonpath".into()], description: Some("Path to the Python interpreter".into()), args: vec![Argument { name: Some("FILE".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--pythonplatform".into()], description: Some("Analyze for platform".into()), args: vec![Argument { name: Some("PLATFORM".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--pythonversion".into()], description: Some("Analyze for Python version".into()), args: vec![Argument { name: Some("VERSION".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--venvpath".into(), "-v".into()], description: Some("Directory that contains virtual environments".into()), args: vec![Argument { name: Some("DIRECTORY".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--outputjson".into()], description: Some("Output results in JSON format".into()), ..Default::default() },
            SpecOption { names: vec!["--verbose".into()], description: Some("Emit verbose diagnostics".into()), ..Default::default() },
            SpecOption { names: vec!["--stats".into()], description: Some("Print detailed performance stats".into()), ..Default::default() },
            SpecOption { names: vec!["--dependencies".into()], description: Some("Emit import dependency information".into()), ..Default::default() },
            SpecOption { names: vec!["--level".into()], description: Some("Minimum diagnostic level".into()), args: vec![Argument { name: Some("LEVEL".into()), ..Default::default() }], ..Default::default() },
            SpecOption { names: vec!["--skipunannotated".into()], description: Some("Skip type analysis of unannotated functions".into()), ..Default::default() },
            SpecOption { names: vec!["--warnings".into()], description: Some("Use exit code of 1 if warnings are reported".into()), ..Default::default() },
            SpecOption { names: vec!["--threads".into()], description: Some("Use up to N threads to parallelize type checking".into()), args: vec![Argument { name: Some("N".into()), is_optional: true, ..Default::default() }], ..Default::default() },
        ],
    }
}

fn timeout_spec() -> CommandSpec {
    CommandSpec {
        name: "timeout".to_string(),
        description: Some("Run a command with a time limit".to_string()),
        subcommands: Vec::new(),
        args: vec![
            Argument {
                name: Some("duration".to_string()),
                description: Some("Duration to wait before timing out (e.g., 10, 5s, 2m)".to_string()),
                is_optional: false,
                ..Default::default()
            },
            Argument {
                name: Some("command".to_string()),
                description: Some("Command to run".to_string()),
                is_command: true,
                ..Default::default()
            },
        ],
        options: Vec::new(),
    }
}

fn sleep_spec() -> CommandSpec {
    CommandSpec {
        name: "sleep".to_string(),
        description: Some("Delay for a specified amount of time".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("duration".to_string()),
            description: Some("Duration to sleep (seconds or with suffix like 5s, 2m, 1h)".to_string()),
            is_optional: false,
            ..Default::default()
        }],
        options: Vec::new(),
    }
}

fn alias_spec() -> CommandSpec {
    CommandSpec {
        name: "alias".to_string(),
        description: Some("Create or list command aliases".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("definition".to_string()),
            description: Some("Alias definition in the form name=value".to_string()),
            is_optional: true,
            is_variadic: true,
            ..Default::default()
        }],
        options: Vec::new(),
    }
}

fn nohup_spec() -> CommandSpec {
    CommandSpec {
        name: "nohup".to_string(),
        description: Some("Run a command immune to hangups".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("command".to_string()),
            description: Some("Command to run with nohup".to_string()),
            is_command: true,
            ..Default::default()
        }],
        options: Vec::new(),
    }
}

fn time_spec() -> CommandSpec {
    CommandSpec {
        name: "time".to_string(),
        description: Some("Time a command".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("command".to_string()),
            description: Some("Command to time".to_string()),
            is_command: true,
            ..Default::default()
        }],
        options: Vec::new(),
    }
}

fn srun_spec() -> CommandSpec {
    CommandSpec {
        name: "srun".to_string(),
        description: Some("Run a command on SLURM cluster nodes".to_string()),
        subcommands: Vec::new(),
        args: vec![Argument {
            name: Some("command".to_string()),
            description: Some("Command to run on the cluster".to_string()),
            is_command: true,
            ..Default::default()
        }],
        options: vec![
            SpecOption {
                names: vec!["-n".into(), "--ntasks".into()],
                description: Some("Number of tasks".into()),
                args: vec![Argument { name: Some("count".into()), description: Some("Number of tasks to run".into()), ..Default::default() }],
                ..Default::default()
            },
            SpecOption {
                names: vec!["-N".into(), "--nodes".into()],
                description: Some("Number of nodes".into()),
                args: vec![Argument { name: Some("count".into()), description: Some("Number of nodes to allocate".into()), ..Default::default() }],
                ..Default::default()
            },
        ],
    }
}
