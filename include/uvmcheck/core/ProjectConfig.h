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

/// Per-rule overrides from the "rules" section of the project config.
struct RuleSettings {
    std::optional<bool> enabled;
    std::optional<Severity> severity;
    PathSet excludePaths;
    json::Value options; // object, or null when absent
};

/// Why a violation was dropped before reaching the report.
enum class SuppressionReason {
    None,
    LibraryPath,   // file matches "library_paths"
    ExcludedPath,  // file matches "exclude_paths"
    RuleExcluded,  // file matches the rule's own "exclude_paths"
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

    const json::Value* findOption(std::string_view ruleId, std::string_view key) const;
};

} // namespace uvmcheck::core
