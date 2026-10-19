#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::core {

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
}

} // namespace uvmcheck::core
