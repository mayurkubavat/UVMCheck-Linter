#pragma once

#include <string>
#include <string_view>
#include <vector>
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

class DiagnosticEngine {
public:
    explicit DiagnosticEngine(const slang::SourceManager& sourceManager)
        : sourceManager(sourceManager) {}

    void addViolation(std::string_view ruleId,
                      std::string_view ruleName,
                      std::string_view message,
                      Severity severity,
                      slang::SourceLocation loc,
                      std::vector<FixItHint> fixIts = {}) {
        Violation v;
        v.ruleId = ruleId;
        v.ruleName = ruleName;
        v.message = message;
        v.severity = severity;
        v.fixIts = std::move(fixIts);

        if (loc.valid()) {
            std::string_view fileName = sourceManager.getFileName(loc);
            if (fileName.find("uvm_mock.svh") != std::string_view::npos) {
                return;
            }
            v.filePath = std::string(fileName);
            v.line = sourceManager.getLineNumber(loc);
            v.column = sourceManager.getColumnNumber(loc);
            v.snippet = std::string(sourceManager.getSourceLine(loc));
        }

        violations.push_back(std::move(v));
    }

    void addDirectViolation(std::string_view ruleId,
                            std::string_view ruleName,
                            std::string_view message,
                            Severity severity,
                            std::string_view filePath,
                            size_t line,
                            size_t column) {
        if (filePath.find("uvm_mock.svh") != std::string_view::npos) {
            return;
        }
        Violation v;
        v.ruleId = ruleId;
        v.ruleName = ruleName;
        v.message = message;
        v.severity = severity;
        v.filePath = std::string(filePath);
        v.line = line;
        v.column = column;
        violations.push_back(std::move(v));
    }

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

    void printSummary(std::ostream& os = std::cout) const;

private:
    const slang::SourceManager& sourceManager;
    std::vector<Violation> violations;
};

} // namespace uvmcheck::core
