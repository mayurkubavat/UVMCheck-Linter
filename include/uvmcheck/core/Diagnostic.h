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

struct FixItHint {
    slang::SourceRange range;
    std::string replacementText;
    std::string description;
};

struct Violation {
    std::string ruleId;
    std::string ruleName;
    std::string message;
    Severity severity;
    std::string filePath;
    size_t line = 0;
    size_t column = 0;
    std::string snippet;
    std::vector<FixItHint> fixIts;
};

class ProjectConfig;

class DiagnosticEngine {
public:
    explicit DiagnosticEngine(const slang::SourceManager& sourceManager)
        : sourceManager(sourceManager) {}

    /// Attach a project config: violations in library/excluded paths are
    /// dropped as they are reported and per-rule severity overrides applied.
    void setProjectConfig(const ProjectConfig* config) { projectConfig = config; }

    void addViolation(std::string_view ruleId,
                      std::string_view ruleName,
                      std::string_view message,
                      Severity severity,
                      slang::SourceLocation loc,
                      std::vector<FixItHint> fixIts = {});

    void addDirectViolation(std::string_view ruleId,
                            std::string_view ruleName,
                            std::string_view message,
                            Severity severity,
                            std::string_view filePath,
                            size_t line,
                            size_t column);

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
};

} // namespace uvmcheck::core
