//! # clm_types — PowerShell Constrained Language Mode allowed types
//!
//! Translates `tools/PowerShellTool/clmTypes.ts`.
//! Types allowed under AppLocker/WDAC system lockdown.

use std::collections::HashSet;
use std::sync::LazyLock;

/// CLM allowed types — type literals NOT in this set should prompt for approval.
/// Stored lowercase; callers should lowercase their input before checking.
pub static CLM_ALLOWED_TYPES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let types = [
        // Type accelerators (short names)
        "alias", "allowemptycollection", "allowemptystring", "allownull",
        "argumentcompleter", "argumentcompletions", "array", "bigint", "bool",
        "byte", "char", "cimclass", "cimconverter", "ciminstance", "cimtype",
        "cmdletbinding", "cultureinfo", "datetime", "decimal", "double",
        "dsclocalconfigurationmanager", "dscproperty", "dscresource",
        "experimentaction", "experimental", "experimentalfeature", "float",
        "guid", "hashtable", "int", "int16", "int32", "int64", "ipaddress",
        "long", "mailaddress", "nonnegativedouble", "nonnegativeint", "nullable",
        "object", "obsolete", "ordered", "outputtype", "parameter", "phpdoc",
        "pscredential", "pscustomobject", "psdefaultvalue", "psobject",
        "pstypename", "ref", "regex", "sbyte", "scriptblock", "semver",
        "short", "single", "string", "supportsregularexpressions",
        "supportswildcards", "switch", "timespan", "type", "uint", "uint16",
        "uint32", "uint64", "ulong", "uri", "ushort", "validatecount",
        "validatedrive", "validateenumeratedarguments", "validatelength",
        "validatenotnull", "validatenotnullorempty", "validatepattern",
        "validaterange", "validatescript", "validateset",
        "validatetrusteddata", "validateuserdriveroot", "version",
        "void", "wildcard", "wildcardpattern", "wmi", "wmiclass",
        "wmisearcher", "xml",
        // Full .NET type names commonly used
        "system.array", "system.boolean", "system.byte", "system.char",
        "system.collections.arraylist", "system.collections.hashtable",
        "system.collections.generic.dictionary",
        "system.collections.generic.list",
        "system.collections.objectmodel.collection",
        "system.collections.specialized.ordereddictionary",
        "system.convert", "system.datetime", "system.datetimeoffset",
        "system.decimal", "system.double", "system.enum", "system.environment",
        "system.globalization.cultureinfo", "system.guid", "system.int16",
        "system.int32", "system.int64", "system.io.directoryinfo",
        "system.io.file", "system.io.fileinfo", "system.io.path",
        "system.io.streamreader", "system.io.streamwriter",
        "system.io.stringreader", "system.io.stringwriter",
        "system.math", "system.net.ipaddress", "system.net.mail.mailaddress",
        "system.net.networkinformation.ping", "system.object",
        "system.random", "system.sbyte", "system.security.securestring",
        "system.single", "system.string", "system.text.encoding",
        "system.text.regularexpressions.regex",
        "system.text.stringbuilder", "system.timespan", "system.type",
        "system.uint16", "system.uint32", "system.uint64", "system.uri",
        "system.version", "system.void", "system.xml.xmldocument",
        // Management types
        "system.management.automation.actionpreference",
        "system.management.automation.cmdletbindingattribute",
        "system.management.automation.errorrecord",
        "system.management.automation.informationrecord",
        "system.management.automation.language.codegeneration",
        "system.management.automation.parameterattribute",
        "system.management.automation.pscredential",
        "system.management.automation.pscustomobject",
        "system.management.automation.psobject",
        "system.management.automation.pstypename",
        "system.management.automation.scriptblock",
        "system.management.automation.switchparameter",
        "system.management.automation.validatesetattribute",
        "system.management.automation.wildcardpattern",
    ];
    types.iter().copied().collect()
});

/// `clmTypes.ts` `normalizeTypeName` — normalize a type name from AST
/// `TypeName.FullName` or `TypeName.Name`.
///
/// Handles:
/// - Array suffix `[]`: `"String[]"` → `"string"` (arrays of allowed types are allowed).
/// - Generic brackets `[...]`: `"List[int]"` → `"list"` (conservative — the
///   generic wrapper might be unsafe even if the type arg is safe).
pub fn normalize_type_name(name: &str) -> String {
    let lower = name.to_lowercase();
    // Strip array suffix and generic args from the END of the string.
    let trimmed = lower.trim_end();
    let stripped = if let Some(idx) = trimmed.rfind('[') {
        // Only strip if the closing bracket is at the end.
        if trimmed.ends_with(']') {
            &trimmed[..idx]
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    stripped.trim().to_string()
}

/// Alias matching the TS export name.
#[allow(non_snake_case)]
pub fn normalizeTypeName(name: &str) -> String {
    normalize_type_name(name)
}

/// Check if a type name is allowed under CLM.
/// Input should be the raw type name (will be lowercased internally).
pub fn is_clm_allowed_type(type_name: &str) -> bool {
    let lower = type_name.to_lowercase();
    // Strip leading/trailing brackets if present: [System.String] → system.string
    let cleaned = lower.trim_start_matches('[').trim_end_matches(']');
    CLM_ALLOWED_TYPES.contains(cleaned)
}
