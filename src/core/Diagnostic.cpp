#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/core/ProjectConfig.h"

namespace uvmcheck::core {

static bool isMockLibrary(std::string_view fileName) {
    return fileName.find("uvm_mock.svh") != std::string_view::npos;
}

const DiagnosticEngine::PathInfo& DiagnosticEngine::classify(std::string_view filePath) {
    auto it = pathCache.find(std::string(filePath));
    if (it == pathCache.end()) {
        PathInfo info;
        info.normalized = normalizePathForMatching(filePath);
        SuppressionReason r = projectConfig->classifyPath(info.normalized);
        info.cls = r == SuppressionReason::LibraryPath    ? PathClass::Library
                   : r == SuppressionReason::ExcludedPath ? PathClass::Excluded
                                                          : PathClass::Normal;
        it = pathCache.emplace(std::string(filePath), std::move(info)).first;
    }
    return it->second;
}

bool DiagnosticEngine::applyProjectPolicy(std::string_view ruleId, std::string_view filePath,
                                          std::string_view originFile, Severity& severity) {
    if (!projectConfig) return false;

    // Covers Python plugin rules, which the driver cannot skip up front.
    if (!projectConfig->isRuleEnabled(ruleId, {})) return true;

    if (projectConfig->hasPathFilters() && !filePath.empty()) {
        const PathInfo& info = classify(filePath);
        if (info.cls == PathClass::Library) {
            librarySuppressed++;
            return true;
        }
        if (info.cls == PathClass::Excluded || projectConfig->isExcludedForRule(ruleId, info.normalized)) {
            excludeSuppressed++;
            return true;
        }
        if (!originFile.empty() && originFile != filePath &&
            !projectConfig->reportLibraryMacroExpansions() &&
            classify(originFile).cls == PathClass::Library) {
            librarySuppressed++;
            return true;
        }
    }

    if (auto sev = projectConfig->severityOverride(ruleId)) severity = *sev;
    return false;
}

void DiagnosticEngine::addViolation(std::string_view ruleId,
                                    std::string_view ruleName,
                                    std::string_view message,
                                    Severity severity,
                                    slang::SourceLocation loc,
                                    std::vector<FixItHint> fixIts) {
    std::string_view fileName;
    std::string_view originFile;
    if (loc.valid()) {
        fileName = sourceManager.getFileName(loc);
        if (isMockLibrary(fileName)) return;
        if (projectConfig && sourceManager.isMacroLoc(loc)) {
            originFile = sourceManager.getFileName(sourceManager.getFullyOriginalLoc(loc));
        }
    }
    if (applyProjectPolicy(ruleId, fileName, originFile, severity)) return;

    Violation v;
    v.ruleId = ruleId;
    v.ruleName = ruleName;
    v.message = message;
    v.severity = severity;
    v.fixIts = std::move(fixIts);

    if (loc.valid()) {
        v.filePath = std::string(fileName);
        v.line = sourceManager.getLineNumber(loc);
        v.column = sourceManager.getColumnNumber(loc);
        v.snippet = std::string(sourceManager.getSourceLine(loc));
    }

    violations.push_back(std::move(v));
}

void DiagnosticEngine::addDirectViolation(std::string_view ruleId,
                                          std::string_view ruleName,
                                          std::string_view message,
                                          Severity severity,
                                          std::string_view filePath,
                                          size_t line,
                                          size_t column) {
    if (isMockLibrary(filePath)) return;
    if (applyProjectPolicy(ruleId, filePath, {}, severity)) return;

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

void DiagnosticEngine::absorb(DiagnosticEngine& other) {
    violations.insert(violations.end(), std::make_move_iterator(other.violations.begin()),
                      std::make_move_iterator(other.violations.end()));
    other.violations.clear();
    librarySuppressed += other.librarySuppressed;
    excludeSuppressed += other.excludeSuppressed;
    other.librarySuppressed = other.excludeSuppressed = 0;
}

void DiagnosticEngine::printSummary(std::ostream& os) const {
    const char* reset   = "\033[0m";
    const char* red     = "\033[1;31m";
    const char* yellow  = "\033[1;33m";
    const char* blue    = "\033[1;34m";
    const char* bold    = "\033[1m";
    const char* dim     = "\033[2m";

    for (const auto& v : violations) {
        const char* color = yellow;
        if (v.severity == Severity::Error) color = red;
        else if (v.severity == Severity::Info) color = blue;

        os << bold << v.filePath << ":" << v.line << ":" << v.column << ": "
           << color << "[" << severityToString(v.severity) << "]" << reset
           << " " << bold << v.ruleId << " (" << v.ruleName << "):" << reset
           << " " << v.message << "\n";

        if (!v.snippet.empty()) {
            os << "    " << dim << v.snippet << reset << "\n";
            os << "    " << color;
            for (size_t i = 1; i < v.column; ++i) os << " ";
            os << "^" << reset << "\n";
        }
    }

    os << "\n" << bold << "=== Lint Summary ===" << reset << "\n";
    os << "Total Violations: " << violations.size() << " ("
       << red << errorCount() << " errors" << reset << ", "
       << yellow << warningCount() << " warnings" << reset << ")\n";
    if (librarySuppressed > 0 || excludeSuppressed > 0) {
        os << dim << "Suppressed by project config: " << librarySuppressed
           << " in library paths, " << excludeSuppressed << " in excluded paths" << reset << "\n";
    }
}

} // namespace uvmcheck::core
