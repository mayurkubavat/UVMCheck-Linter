#include "uvmcheck/reporting/JsonReporter.h"
#include <cstdio>
#include <fstream>
#include <iostream>

namespace uvmcheck::reporting {

static std::string escapeJson(std::string_view s) {
    std::string out;
    out.reserve(s.size());
    for (char c : s) {
        switch (c) {
            case '"':  out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned>(c));
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out;
}

static void writeReport(std::ostream& os,
                        const std::vector<core::Violation>& violations,
                        size_t compileErrors,
                        size_t waivedCount) {
    os << "{\n"
       << "  \"tool\": \"uvmcheck\",\n"
       << "  \"version\": \"0.1.0\",\n"
       << "  \"compileErrors\": " << compileErrors << ",\n"
       << "  \"waived\": " << waivedCount << ",\n"
       << "  \"violations\": [";

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        os << (i == 0 ? "\n" : ",\n")
           << "    {"
           << "\"ruleId\": \"" << escapeJson(v.ruleId) << "\", "
           << "\"ruleName\": \"" << escapeJson(v.ruleName) << "\", "
           << "\"severity\": \"" << core::severityToString(v.severity) << "\", "
           << "\"file\": \"" << escapeJson(v.filePath) << "\", "
           << "\"line\": " << v.line << ", "
           << "\"column\": " << v.column << ", "
           << "\"message\": \"" << escapeJson(v.message) << "\"}";
    }

    os << (violations.empty() ? "]\n" : "\n  ]\n") << "}\n";
}

bool JsonReporter::writeJson(const std::string& outputPath,
                             const std::vector<core::Violation>& violations,
                             size_t compileErrors,
                             size_t waivedCount) {
    if (outputPath == "-") {
        writeReport(std::cout, violations, compileErrors, waivedCount);
        return true;
    }

    std::ofstream file(outputPath);
    if (!file.is_open()) return false;
    writeReport(file, violations, compileErrors, waivedCount);
    return true;
}

} // namespace uvmcheck::reporting
