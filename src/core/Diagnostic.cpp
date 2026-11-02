#include <cctype>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/core/FixPlanner.h"
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

void DiagnosticEngine::noteFixLocations(std::vector<FixItHint>& fixes) const {
    for (auto& f : fixes) {
        if (f.startLine != 0) continue;
        auto s = f.range.start(), e = f.range.end();
        if (!s.valid() || !e.valid()) continue;
        if (sourceManager.isMacroLoc(s) || sourceManager.isMacroLoc(e)) continue;
        // Rewriting written text around a macro call with text from its expansion would
        // inline the macro; show such a fix but never apply it.
        if (f.safety != FixSafety::DisplayOnly && s.buffer() == e.buffer() &&
            fix::dropsMacroCall(sourceManager.getSourceText(s.buffer()), s.offset(), e.offset(),
                                f.replacementText)) {
            f.safety = FixSafety::DisplayOnly;
            f.description += " (not applied: it would remove a macro call)";
        }
        f.startLine = sourceManager.getLineNumber(s);
        f.startColumn = sourceManager.getColumnNumber(s);
        f.endLine = sourceManager.getLineNumber(e);
        f.endColumn = sourceManager.getColumnNumber(e);
    }
}

Violation* DiagnosticEngine::addViolation(std::string_view ruleId,
                                    std::string_view ruleName,
                                    std::string_view message,
                                    Severity severity,
                                    slang::SourceLocation loc,
                                    std::vector<FixItHint> fixIts) {
    std::string_view fileName;
    std::string_view originFile;
    if (loc.valid()) {
        fileName = sourceManager.getFileName(loc);
        if (isMockLibrary(fileName)) return nullptr;
        if (projectConfig && sourceManager.isMacroLoc(loc)) {
            originFile = sourceManager.getFileName(sourceManager.getFullyOriginalLoc(loc));
        }
    }
    if (applyProjectPolicy(ruleId, fileName, originFile, severity)) return nullptr;

    Violation v;
    v.ruleId = ruleId;
    v.ruleName = ruleName;
    v.message = message;
    v.severity = severity;
    noteFixLocations(fixIts);
    v.fixIts = std::move(fixIts);

    if (loc.valid()) {
        v.filePath = std::string(fileName);
        v.line = sourceManager.getLineNumber(loc);
        v.column = sourceManager.getColumnNumber(loc);
        v.snippet = std::string(sourceManager.getSourceLine(loc));
        // Default extent: the identifier (or single token character) at the column.
        size_t end = v.column > 0 ? v.column - 1 : 0;
        if (end < v.snippet.size()) {
            auto ident = [](char c) {
                return std::isalnum(static_cast<unsigned char>(c)) || c == '_' || c == '$' || c == '`';
            };
            size_t start = end;
            while (end < v.snippet.size() && ident(v.snippet[end])) ++end;
            if (end == start) ++end;
            v.endColumn = end + 1;
        }
    }

    violations.push_back(std::move(v));
    return &violations.back();
}

Violation* DiagnosticEngine::addViolation(std::string_view ruleId,
                                          std::string_view ruleName,
                                          std::string_view message,
                                          Severity severity,
                                          slang::SourceRange range,
                                          std::vector<std::pair<slang::SourceLocation, std::string>> related,
                                          std::vector<FixItHint> fixIts) {
    Violation* v = addViolation(ruleId, ruleName, message, severity, range.start(), std::move(fixIts));
    setRange(v, range);
    for (auto& [loc, msg] : related) addRelated(v, loc, std::move(msg));
    return v;
}

void DiagnosticEngine::setRange(Violation* v, slang::SourceRange range) const {
    if (!v || !range.start().valid() || !range.end().valid()) return;
    auto start = sourceManager.getFullyOriginalLoc(range.start());
    auto end = sourceManager.getFullyOriginalLoc(range.end());
    if (start.buffer() != end.buffer() || end.offset() <= start.offset()) return;
    if (sourceManager.getLineNumber(end) != v->line) return;
    size_t col = sourceManager.getColumnNumber(end);
    if (col > v->column) v->endColumn = col;
}

void DiagnosticEngine::addRelated(Violation* v, slang::SourceLocation loc, std::string message) const {
    if (!v || !loc.valid()) return;
    loc = sourceManager.getFullyOriginalLoc(loc);
    RelatedLocation r;
    r.filePath = std::string(sourceManager.getFileName(loc));
    r.line = sourceManager.getLineNumber(loc);
    r.column = sourceManager.getColumnNumber(loc);
    r.message = std::move(message);
    v->related.push_back(std::move(r));
}

void DiagnosticEngine::addDirectViolation(std::string_view ruleId,
                                          std::string_view ruleName,
                                          std::string_view message,
                                          Severity severity,
                                          std::string_view filePath,
                                          size_t line,
                                          size_t column,
                                          std::vector<FixItHint> fixIts) {
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
    v.fixIts = std::move(fixIts);
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
            os << "^";
            for (size_t i = v.column + 1; i < v.endColumn; ++i) os << "~";
            os << reset << "\n";
        }
        for (const auto& r : v.related) {
            os << "    " << dim << "note: " << r.filePath << ":" << r.line << ":" << r.column << ": "
               << r.message << reset << "\n";
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
