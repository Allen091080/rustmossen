use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Set of preapproved hosts for WebFetch tool.
static PREAPPROVED_HOSTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for entry in PREAPPROVED_ENTRIES {
        set.insert(*entry);
    }
    set
});

/// Hostname-only entries (no path component).
static HOSTNAME_ONLY: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for entry in PREAPPROVED_ENTRIES {
        if !entry.contains('/') {
            set.insert(*entry);
        }
    }
    set
});

/// Path-prefix entries keyed by hostname.
static PATH_PREFIXES: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
    for entry in PREAPPROVED_ENTRIES {
        if let Some(slash_idx) = entry.find('/') {
            let host = &entry[..slash_idx];
            let path = &entry[slash_idx..];
            map.entry(host).or_default().push(path);
        }
    }
    map
});

/// All preapproved host entries.
const PREAPPROVED_ENTRIES: &[&str] = &[
    "platform.mossen.invalid",
    "modelcontextprotocol.io",
    "github.com/mossen",
    "agentskills.io",
    "docs.python.org",
    "en.cppreference.com",
    "docs.oracle.com",
    "learn.microsoft.com",
    "developer.mozilla.org",
    "go.dev",
    "pkg.go.dev",
    "www.php.net",
    "docs.swift.org",
    "kotlinlang.org",
    "ruby-doc.org",
    "doc.rust-lang.org",
    "www.typescriptlang.org",
    "react.dev",
    "angular.io",
    "vuejs.org",
    "nextjs.org",
    "expressjs.com",
    "nodejs.org",
    "bun.sh",
    "jquery.com",
    "getbootstrap.com",
    "tailwindcss.com",
    "d3js.org",
    "threejs.org",
    "redux.js.org",
    "webpack.js.org",
    "jestjs.io",
    "reactrouter.com",
    "docs.djangoproject.com",
    "flask.palletsprojects.com",
    "fastapi.tiangolo.com",
    "pandas.pydata.org",
    "numpy.org",
    "www.tensorflow.org",
    "pytorch.org",
    "scikit-learn.org",
    "matplotlib.org",
    "requests.readthedocs.io",
    "jupyter.org",
    "laravel.com",
    "symfony.com",
    "wordpress.org",
    "docs.spring.io",
    "hibernate.org",
    "tomcat.apache.org",
    "gradle.org",
    "maven.apache.org",
    "asp.net",
    "dotnet.microsoft.com",
    "nuget.org",
    "blazor.net",
    "reactnative.dev",
    "docs.flutter.dev",
    "developer.apple.com",
    "developer.android.com",
    "keras.io",
    "spark.apache.org",
    "huggingface.co",
    "www.kaggle.com",
    "www.mongodb.com",
    "redis.io",
    "www.postgresql.org",
    "dev.mysql.com",
    "www.sqlite.org",
    "graphql.org",
    "prisma.io",
    "docs.aws.amazon.com",
    "cloud.google.com",
    "kubernetes.io",
    "www.docker.com",
    "www.terraform.io",
    "www.ansible.com",
    "vercel.com/docs",
    "docs.netlify.com",
    "devcenter.heroku.com",
    "cypress.io",
    "selenium.dev",
    "docs.unity.com",
    "docs.unrealengine.com",
    "git-scm.com",
    "nginx.org",
    "httpd.apache.org",
];

/// Check if a hostname + pathname is a preapproved domain.
pub fn is_preapproved_host(hostname: &str, pathname: &str) -> bool {
    if HOSTNAME_ONLY.contains(hostname) {
        return true;
    }
    if let Some(prefixes) = PATH_PREFIXES.get(hostname) {
        for p in prefixes {
            if pathname == *p || pathname.starts_with(&format!("{}/", p)) {
                return true;
            }
        }
    }
    false
}
