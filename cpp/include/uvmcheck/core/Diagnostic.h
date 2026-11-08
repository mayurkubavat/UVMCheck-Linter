#pragma once

#include <string>
#include <string_view>
#include <vector>
#include <unordered_map>
#include <iostream>
#include "slang/text/SourceLocation.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::core {

enum class Severity {
    Info,
    Warning,
    Error
};

inline const char* severityToString(Severity s) {
    switch (s) {
        case Severity::Info:    return "INFO";
        case Severity::Warning: return "WARNING";
        case Severity::Error:   return "ERROR";
    }
    return "UNKNOWN";
}

/// How automatically a fix may be written back to a source file.
/// Safe fixes preserve simulation behaviour (labels, whitespace, equivalent
/// rewrites). Unsafe fixes can change it (X/Z matching, scheduling, widths,
/// randomization, or factory / config_db / objection / phase semantics) and
/// are applied only with --unsafe-fixes. DisplayOnly is a suggestion shown in
/// reports and the editor; it is never written by --fix.
enum class FixSafety {
    Safe,
    Unsafe,
    DisplayOnly
};

inline const char* fixSafetyToString(FixSafety s) {
    switch (s) {
        case FixSafety::Safe:        return "safe";
        case FixSafety::Unsafe:      return "unsafe";
        case FixSafety::DisplayOnly: return "display-only";
    }
    return "safe";
}

inline FixSafety fixSafetyFromString(std::string_view s) {
    if (s == "unsafe") return FixSafety::Unsafe;
    if (s == "display-only" || s == "display_only") return FixSafety::DisplayOnly;
    return FixSafety::Safe;
}

struct FixItHint {
    slang::SourceRange range;
    std::string replacementText;
    std::string description;
    FixSafety safety = FixSafety::Safe;
    /// 1-based region, filled when the finding is recorded so reporters do not
    /// need a SourceManager. 0 means the region is unknown (for example a
    /// macro expansion).
    size_t startLine = 0;
    size_t startColumn = 0;
    size_t endLine = 0;
    size_t endColumn = 0;
};

/// Another place that explains a finding: the first copy of a clone, the
/// base-class member being shadowed, the declaration a use refers to.
struct RelatedLocation {
    std::string filePath;
    size_t line = 0;
    size_t column = 0;
    std::string message;
};

struct Violation {
    std::string ruleId;
    std::string ruleName;
    std::string message;
    Severity severity;
    std::string filePath;
    size_t line = 0;
    size_t column = 0;
    /// One past the last column of the flagged text on `line` (0 = unknown).
    /// Defaults to the end of the identifier or token at `column`.
    size_t endColumn = 0;
    std::string snippet;
    std::vector<FixItHint> fixIts;
    std::vector<RelatedLocation> related;
};

class ProjectConfig;

class DiagnosticEngine {
public:
    explicit DiagnosticEngine(const slang::SourceManager& sourceManager)
        : sourceManager(sourceManager) {}

    /// Attach a project config: violations in library/excluded paths are
    /// dropped as they are reported and per-rule severity overrides applied.
    void setProjectConfig(const ProjectConfig* config) { projectConfig = config; }
    const ProjectConfig* getProjectConfig() const { return projectConfig; }

    /// Moves `other`'s violations to the end of this engine and adds its
    /// suppression counts to ours. Used to merge per-rule engines.
    void absorb(DiagnosticEngine& other);

    /// Records a finding. Returns it so the caller can attach related
    /// locations or a range, or nullptr when project policy dropped it. The
    /// pointer is valid until the next finding is added.
    Violation* addViolation(std::string_view ruleId,
                            std::string_view ruleName,
                            std::string_view message,
                            Severity severity,
                            slang::SourceLocation loc,
                            std::vector<FixItHint> fixIts = {});

    /// Same, with the full extent of the flagged code (end kept when it is on
    /// the start line) and related locations.
    Violation* addViolation(std::string_view ruleId,
                            std::string_view ruleName,
                            std::string_view message,
                            Severity severity,
                            slang::SourceRange range,
                            std::vector<std::pair<slang::SourceLocation, std::string>> related,
                            std::vector<FixItHint> fixIts = {});

    /// Attaches a related location to a finding (no-op on nullptr or an invalid location).
    void addRelated(Violation* v, slang::SourceLocation loc, std::string message) const;
    /// Sets the finding's end column from a range ending on its start line.
    void setRange(Violation* v, slang::SourceRange range) const;

    void addDirectViolation(std::string_view ruleId,
                            std::string_view ruleName,
                            std::string_view message,
                            Severity severity,
                            std::string_view filePath,
                            size_t line,
                            size_t column,
                            std::vector<FixItHint> fixIts = {});

    const std::vector<Violation>& getViolations() const { return violations; }
    std::vector<Violation>& getViolations() { return violations; }

    size_t errorCount() const {
        size_t count = 0;
        for (const auto& v : violations) {
            if (v.severity == Severity::Error) count++;
        }
        return count;
    }

    size_t warningCount() const {
        size_t count = 0;
        for (const auto& v : violations) {
            if (v.severity == Severity::Warning) count++;
        }
        return count;
    }

    /// Violations dropped because they were inside library_paths.
    size_t librarySuppressedCount() const { return librarySuppressed; }
    /// Violations dropped by exclude_paths or a rule's own exclude_paths.
    size_t excludeSuppressedCount() const { return excludeSuppressed; }

    void printSummary(std::ostream& os = std::cout) const;

private:
    enum class PathClass : uint8_t { Normal, Library, Excluded };
    struct PathInfo {
        PathClass cls;
        std::string normalized;
    };

    const slang::SourceManager& sourceManager;
    std::vector<Violation> violations;
    const ProjectConfig* projectConfig = nullptr;
    std::unordered_map<std::string, PathInfo> pathCache;
    size_t librarySuppressed = 0;
    size_t excludeSuppressed = 0;

    /// Returns true if the violation should be dropped; may rewrite severity.
    /// originFile is the file a macro-expanded location was written in (empty if not a macro).
    bool applyProjectPolicy(std::string_view ruleId, std::string_view filePath,
                            std::string_view originFile, Severity& severity);
    const PathInfo& classify(std::string_view filePath);
    /// Fills start/end line and column on hints that still have a source range.
    void noteFixLocations(std::vector<FixItHint>& fixes) const;
};

} // namespace uvmcheck::core
