#include "uvmcheck/reporting/SarifReporter.h"
#include <fstream>
#include <sstream>

namespace uvmcheck::reporting {

static std::string escapeJson(std::string_view s) {
    std::string out;
    for (char c : s) {
        if (c == '"') out += "\\\"";
        else if (c == '\\') out += "\\\\";
        else if (c == '\n') out += "\\n";
        else if (c == '\r') out += "\\r";
        else if (c == '\t') out += "\\t";
        else out += c;
    }
    return out;
}

static const char* sarifLevel(core::Severity s) {
    switch (s) {
        case core::Severity::Error:   return "error";
        case core::Severity::Warning: return "warning";
        case core::Severity::Info:    return "note";
    }
    return "warning";
}

bool SarifReporter::writeSarif(const std::string& outputPath,
                               const std::vector<core::Violation>& violations,
                               const std::vector<std::unique_ptr<rules::Rule>>& rules) {
    std::ofstream file(outputPath);
    if (!file.is_open()) return false;

    file << "{\n"
         << "  \"$schema\": \"https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json\",\n"
         << "  \"version\": \"2.1.0\",\n"
         << "  \"runs\": [\n"
         << "    {\n"
         << "      \"tool\": {\n"
         << "        \"driver\": {\n"
         << "          \"name\": \"uvmcheck\",\n"
         << "          \"version\": \"0.1.0\",\n"
         << "          \"informationUri\": \"https://github.com/uvmcheck/uvmcheck\",\n"
         << "          \"rules\": [\n";

    for (size_t i = 0; i < rules.size(); ++i) {
        const auto& r = rules[i];
        file << "            {\n"
             << "              \"id\": \"" << r->getId() << "\",\n"
             << "              \"name\": \"" << r->getName() << "\",\n"
             << "              \"shortDescription\": { \"text\": \"" << escapeJson(r->getDescription()) << "\" },\n"
             << "              \"properties\": { \"category\": \"" << r->getCategory() << "\" }\n"
             << "            }" << (i + 1 < rules.size() ? "," : "") << "\n";
    }

    file << "          ]\n"
         << "        }\n"
         << "      },\n"
         << "      \"results\": [\n";

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        size_t remediationMinutes = 5;
        if (v.ruleId == "SV-SML-004") remediationMinutes = 60;
        else if (v.ruleId == "SV-SML-002") remediationMinutes = 30;
        else if (v.ruleId == "SV-SML-001") remediationMinutes = 20;
        else if (v.ruleId == "SV-PRF-007" || v.ruleId == "SV-PRF-008") remediationMinutes = 5;
        else if (v.ruleId == "SV-PRF-005") remediationMinutes = 15;
        else if (v.ruleId == "SV-PRF-009") remediationMinutes = 10;
        else if (v.severity == core::Severity::Error) remediationMinutes = 45;
        else if (v.severity == core::Severity::Warning) remediationMinutes = 15;

        file << "        {\n"
             << "          \"ruleId\": \"" << v.ruleId << "\",\n"
             << "          \"level\": \"" << sarifLevel(v.severity) << "\",\n"
             << "          \"message\": { \"text\": \"" << escapeJson(v.message) << "\" },\n"
             << "          \"properties\": { \"remediationMinutes\": " << remediationMinutes << " },\n"
             << "          \"locations\": [\n"
             << "            {\n"
             << "              \"physicalLocation\": {\n"
             << "                \"artifactLocation\": { \"uri\": \"" << escapeJson(v.filePath) << "\" },\n"
             << "                \"region\": {\n"
             << "                  \"startLine\": " << v.line << ",\n"
             << "                  \"startColumn\": " << v.column << "\n"
             << "                }\n"
             << "              }\n"
             << "            }\n"
             << "          ]\n"
             << "        }" << (i + 1 < violations.size() ? "," : "") << "\n";
    }

    file << "      ]\n"
         << "    }\n"
         << "  ]\n"
         << "}\n";

    return true;
}

} // namespace uvmcheck::reporting
