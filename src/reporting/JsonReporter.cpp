#include "uvmcheck/reporting/JsonReporter.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include <algorithm>
#include <unordered_map>
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
                        const std::vector<GuidelineResult>& conformance,
                        const std::vector<std::string>& fingerprints) {
    // Category of each rule ID; sub-rules take their reporting rule's category.
    std::unordered_map<std::string, std::string> categories;
    for (const auto& r : rules::RuleRegistry::instance().getRules()) {
        categories[std::string(r->getId())] = std::string(r->getCategory());
        for (const auto& sub : r->getSubRules()) categories[sub.id] = std::string(r->getCategory());
    }
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
           << "\"ruleName\": \"" << escapeJson(v.ruleName) << "\", ";
        if (auto c = categories.find(v.ruleId); c != categories.end())
            os << "\"category\": \"" << escapeJson(c->second) << "\", ";
        os
           << "\"severity\": \"" << core::severityToString(v.severity) << "\", "
           << "\"file\": \"" << escapeJson(v.filePath) << "\", "
           << "\"line\": " << v.line << ", "
           << "\"column\": " << v.column << ", ";
        if (v.endColumn > v.column) os << "\"endColumn\": " << v.endColumn << ", ";
        os << "\"message\": \"" << escapeJson(v.message) << "\"";
        if (!v.related.empty()) {
            os << ", \"related\": [";
            for (size_t r = 0; r < v.related.size(); ++r) {
                const auto& rl = v.related[r];
                os << (r ? ", " : "") << "{\"file\": \"" << escapeJson(rl.filePath) << "\", \"line\": " << rl.line
                   << ", \"column\": " << rl.column << ", \"message\": \"" << escapeJson(rl.message) << "\"}";
            }
            os << "]";
        }
        auto guidelines = rules::guidelinesForRule(v.ruleId);
        if (!guidelines.empty()) os << ", \"guidelines\": " << jsonList(guidelines);
        if (i < fingerprints.size()) os << ", \"fingerprint\": \"" << escapeJson(fingerprints[i]) << "\"";
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

static std::vector<std::string_view> profileNames(uint32_t mask) {
    using P = rules::RuleProfile;
    static const std::pair<P, std::string_view> names[] = {
        {P::Soundness, "soundness"}, {P::Methodology, "methodology"}, {P::Performance, "performance"},
        {P::Style, "style"},         {P::Security, "security"},       {P::Smells, "smells"},
        {P::Advisory, "advisory"}};
    std::vector<std::string_view> out;
    for (const auto& [p, n] : names) {
        if (rules::hasProfile(mask, p)) out.push_back(n);
    }
    return out;
}

static void writeCatalog(std::ostream& os, const std::vector<std::unique_ptr<rules::Rule>>& rules) {
    struct Entry {
        std::string id, name, category, severity, description, parent;
        std::vector<std::string_view> profiles, guidelines;
    };
    std::vector<Entry> entries;
    for (const auto& r : rules) {
        auto profiles = profileNames(r->getProfiles());
        auto guidelines = r->getGuidelines();
        entries.push_back({std::string(r->getId()), std::string(r->getName()),
                           std::string(r->getCategory()),
                           std::string(core::severityToString(r->getDefaultSeverity())),
                           std::string(r->getDescription()), "", profiles, guidelines});
        for (const auto& sub : r->getSubRules()) {
            entries.push_back({sub.id, sub.name, std::string(r->getCategory()),
                               std::string(core::severityToString(sub.severity)), sub.description,
                               std::string(r->getId()), profiles, rules::guidelinesForRule(sub.id)});
        }
    }
    std::sort(entries.begin(), entries.end(), [](const Entry& a, const Entry& b) { return a.id < b.id; });

    os << "{\n  \"version\": \"2.0\",\n  \"generated\": \"uvmcheck --list-rules --json\",\n"
       << "  \"categories\": " << jsonList(rules::ruleCategories()) << ",\n  \"rules\": [\n";
    for (size_t i = 0; i < entries.size(); ++i) {
        const auto& e = entries[i];
        os << "    {\"id\": \"" << escapeJson(e.id) << "\", \"name\": \"" << escapeJson(e.name)
           << "\", \"category\": \"" << escapeJson(e.category) << "\", \"severity\": \""
           << escapeJson(e.severity) << "\", \"profiles\": " << jsonList(e.profiles)
           << ", \"guidelines\": " << jsonList(e.guidelines);
        if (!e.parent.empty()) os << ", \"reportedBy\": \"" << escapeJson(e.parent) << "\"";
        os << ",\n     \"description\": \"" << escapeJson(e.description) << "\"}"
           << (i + 1 < entries.size() ? "," : "") << "\n";
    }
    os << "  ]\n}\n";
}

bool JsonReporter::writeRuleCatalog(const std::string& outputPath,
                                    const std::vector<std::unique_ptr<rules::Rule>>& rules) {
    if (outputPath == "-") {
        writeCatalog(std::cout, rules);
        return true;
    }
    std::ofstream file(outputPath);
    if (!file.is_open()) return false;
    writeCatalog(file, rules);
    return true;
}

bool JsonReporter::writeJson(const std::string& outputPath,
                             const std::vector<core::Violation>& violations,
                             size_t compileErrors,
                             size_t waivedCount,
                             const std::vector<GuidelineResult>& conformance,
                             const std::vector<std::string>& fingerprints) {
    if (outputPath == "-") {
        writeReport(std::cout, violations, compileErrors, waivedCount, conformance, fingerprints);
        return true;
    }

    std::ofstream file(outputPath);
    if (!file.is_open()) return false;
    writeReport(file, violations, compileErrors, waivedCount, conformance, fingerprints);
    return true;
}

} // namespace uvmcheck::reporting
