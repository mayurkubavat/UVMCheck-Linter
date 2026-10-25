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

static std::string jsonList(const std::vector<std::string_view>& items) {
    std::string out = "[";
    for (size_t i = 0; i < items.size(); ++i) {
        out += (i ? ", \"" : "\"") + escapeJson(items[i]) + "\"";
    }
    return out + "]";
}

static void writeReport(std::ostream& os,
                        const std::vector<core::Violation>& violations,
                        size_t compileErrors,
                        size_t waivedCount,
                        const std::vector<GuidelineResult>& conformance) {
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
           << "\"message\": \"" << escapeJson(v.message) << "\"";
        auto guidelines = rules::guidelinesForRule(v.ruleId);
        if (!guidelines.empty()) os << ", \"guidelines\": " << jsonList(guidelines);
        os << "}";
    }

    os << (violations.empty() ? "]" : "\n  ]");
    if (!conformance.empty()) {
        os << ",\n  \"conformance\": [";
        for (size_t i = 0; i < conformance.size(); ++i) {
            const auto& r = conformance[i];
            os << (i == 0 ? "\n" : ",\n")
               << "    {\"guideline\": \"" << r.guideline->id << "\", "
               << "\"level\": \"" << rules::levelName(r.guideline->level) << "\", "
               << "\"status\": \"" << (r.passed() ? "pass" : "fail") << "\", "
               << "\"findings\": " << r.findings << ", "
               << "\"rules\": " << jsonList(r.rules) << "}";
        }
        os << "\n  ]";
    }
    os << "\n}\n";
}

static void writeGuide(std::ostream& os) {
    os << "{\n  \"guidelines\": [";
    const auto& all = rules::methodologyGuidelines();
    for (size_t i = 0; i < all.size(); ++i) {
        const auto& g = all[i];
        os << (i == 0 ? "\n" : ",\n")
           << "    {\"id\": \"" << g.id << "\", "
           << "\"chapter\": \"" << g.chapter << "\", "
           << "\"chapterTitle\": \"" << escapeJson(g.chapterTitle) << "\", "
           << "\"level\": \"" << rules::levelName(g.level) << "\", "
           << "\"statement\": \"" << escapeJson(g.statement) << "\", "
           << "\"rules\": " << jsonList(rules::rulesForGuideline(g.id)) << "}";
    }
    os << "\n  ]\n}\n";
}

bool JsonReporter::writeGuidelines(const std::string& outputPath) {
    if (outputPath == "-") {
        writeGuide(std::cout);
        return true;
    }
    std::ofstream file(outputPath);
    if (!file.is_open()) return false;
    writeGuide(file);
    return true;
}

bool JsonReporter::writeJson(const std::string& outputPath,
                             const std::vector<core::Violation>& violations,
                             size_t compileErrors,
                             size_t waivedCount,
                             const std::vector<GuidelineResult>& conformance) {
    if (outputPath == "-") {
        writeReport(std::cout, violations, compileErrors, waivedCount, conformance);
        return true;
    }

    std::ofstream file(outputPath);
    if (!file.is_open()) return false;
    writeReport(file, violations, compileErrors, waivedCount, conformance);
    return true;
}

} // namespace uvmcheck::reporting
