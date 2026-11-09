#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/core/Json.h"

namespace uvmcheck::core {

/// A single path glob, resolved against a base directory.
///
/// Syntax (gitignore-like):
///   *        any run of characters within one path segment
///   ?        one character within a segment
///   **       zero or more whole segments
///   $VAR / ${VAR} / leading ~   expanded from the environment
/// A pattern without any '/' (e.g. "uvm_mock.svh", "generated") matches that
/// name at any depth. Other relative patterns are anchored at the base
/// directory. A pattern that names a directory also covers everything below it.
class PathPattern {
public:
    PathPattern(std::string_view pattern, const std::filesystem::path& baseDir);

    bool matches(std::string_view normalizedPath) const;

    /// The pattern as written by the user (for diagnostics / reporting).
    const std::string& source() const { return original; }
    /// The pattern after environment expansion and anchoring.
    const std::string& resolved() const { return glob; }
    /// False when an environment variable in the pattern is unset; such a
    /// pattern never matches rather than silently widening to "/**".
    bool isResolved() const { return !glob.empty(); }

private:
    std::string original;
    std::string glob;
};

/// Matches a path against any of a list of patterns.
class PathSet {
public:
    void add(PathPattern p) { patterns.push_back(std::move(p)); }
    bool empty() const { return patterns.empty(); }
    bool matches(std::string_view normalizedPath) const;
    const std::vector<PathPattern>& getPatterns() const { return patterns; }

private:
    std::vector<PathPattern> patterns;
};

/// Converts a file name reported by Slang into the canonical form used for
/// pattern matching: absolute, lexically normalized, '/' separators.
std::string normalizePathForMatching(std::string_view path);

/// Full-path glob match (see PathPattern for syntax). Exposed for testing.
bool globMatch(std::string_view glob, std::string_view path);

/// Expands $VAR, ${VAR} and a leading ~ from the environment. Returns false
/// when a variable is unset.
bool expandEnvVars(std::string_view in, std::string& out);

/// What to compile when no source files are named on the command line, and
/// what the language server compiles around an open file. Paths are absolute
/// (resolved against the project root) with environment variables expanded.
struct BuildSettings {
    std::vector<std::string> sources;     // files or globs (slang -f style patterns)
    std::vector<std::string> filelists;   // .f command files, paths relative to the file
    std::vector<std::string> includeDirs; // +incdir / -I
    std::vector<std::string> defines;     // NAME or NAME=VALUE

    bool hasSources() const { return !sources.empty() || !filelists.empty(); }
    bool empty() const { return !hasSources() && includeDirs.empty() && defines.empty(); }
};

/// The "compat" section: which slang overlay overrides to apply (compat/Overlay.h).
/// Command-line --compat-profile and --override entries are appended after these.
struct CompatSettings {
    std::string profile;                // vcs, questa, xcelium, none, or empty
    std::vector<std::string> overrides; // "id" or "+id" adds, "-id" removes
    std::string uvmHome;                // uvm-core checkout (or its src/), for auto-uvm
    std::vector<std::string> ipStubs;   // stub files or folders for encrypted IP, for opaque-ip
    std::vector<std::string> systemTasks; // extra simulator tasks, for vendor-systasks
};

/// Per-rule overrides from the "rules" section of the project config.
struct RuleSettings {
    std::optional<bool> enabled;
    std::optional<Severity> severity;
    PathSet excludePaths;
    PathSet onlyPaths; // non-empty: the rule reports only in these files
    json::Value options; // object, or null when absent
};

/// Why a violation was dropped before reaching the report.
enum class SuppressionReason {
    None,
    LibraryPath,   // file matches "library_paths"
    ExcludedPath,  // file matches "exclude_paths"
    RuleExcluded,  // file matches the rule's own "exclude_paths", or misses its "only_paths"
};

/// Project-level linting policy: which files are third-party libraries or
/// excluded, and per-rule enable/severity/path/option overrides.
class ProjectConfig {
public:
    PathSet& libraryPaths() { return library; }
    PathSet& excludePaths() { return exclude; }
    const PathSet& libraryPaths() const { return library; }
    const PathSet& excludePaths() const { return exclude; }

    /// Settings keyed by rule ID or rule name, exactly as written in the config.
    std::unordered_map<std::string, RuleSettings>& ruleSettings() { return rules; }

    /// Finds settings for a rule by ID first, then by name.
    const RuleSettings* findRule(std::string_view ruleId, std::string_view ruleName = {}) const;

    /// Returns false only if the config explicitly disables the rule.
    bool isRuleEnabled(std::string_view ruleId, std::string_view ruleName) const;

    /// True when the config asks for the rule: it gives it settings without
    /// turning it off, or has the block its engine reads (`style` for
    /// SV-STY-100, `naming` for SV-NAM-100). Such a rule runs even when the
    /// default preset leaves its profile out.
    bool selectsRule(std::string_view ruleId, std::string_view ruleName) const;

    /// `preset`: the project's rule set, used when --preset is not given.
    const std::string& preset() const { return presetName; }
    void setPreset(std::string v) { presetName = std::move(v); }

    void noteStyleBlock() { hasStyleBlock = true; }
    void noteNamingBlock() { hasNamingBlock = true; }

    std::optional<Severity> severityOverride(std::string_view ruleId) const;

    /// Path classification that does not depend on the rule.
    SuppressionReason classifyPath(std::string_view normalizedPath) const;

    /// Rule-specific path exclusion.
    bool isExcludedForRule(std::string_view ruleId, std::string_view normalizedPath) const;

    /// Typed access to a rule's "options" object, falling back to the default.
    int64_t ruleOptionInt(std::string_view ruleId, std::string_view key, int64_t defaultValue) const;
    bool ruleOptionBool(std::string_view ruleId, std::string_view key, bool defaultValue) const;
    std::string ruleOptionString(std::string_view ruleId, std::string_view key,
                                 std::string_view defaultValue) const;

    /// When false (the default), a violation located in code expanded from a
    /// macro defined in a library file (e.g. `uvm_object_utils) is treated as
    /// library code even though the macro is invoked from a user file.
    bool reportLibraryMacroExpansions() const { return reportLibraryMacros; }
    void setReportLibraryMacroExpansions(bool v) { reportLibraryMacros = v; }

    BuildSettings& build() { return buildSettings; }
    const BuildSettings& build() const { return buildSettings; }

    CompatSettings& compat() { return compatSettings; }
    const CompatSettings& compat() const { return compatSettings; }

    bool hasPathFilters() const { return !library.empty() || !exclude.empty() || anyRuleExcludes; }
    void noteRuleExcludes() { anyRuleExcludes = true; }

    /// Collapses name-keyed entries onto rule IDs so lookups during linting
    /// are a single hash probe. Call once after all rules are registered.
    void resolveRuleNames(const std::unordered_map<std::string, std::string>& nameToId);

private:
    PathSet library;
    PathSet exclude;
    std::unordered_map<std::string, RuleSettings> rules;
    bool anyRuleExcludes = false;
    bool reportLibraryMacros = false;
    bool hasStyleBlock = false;
    bool hasNamingBlock = false;
    std::string presetName;
    BuildSettings buildSettings;
    CompatSettings compatSettings;

    const json::Value* findOption(std::string_view ruleId, std::string_view key) const;
};

} // namespace uvmcheck::core
