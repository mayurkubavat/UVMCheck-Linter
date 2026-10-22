#include "uvmcheck/waivers/WaiverEngine.h"
#include <fstream>
#include <iostream>
#include <sstream>

namespace uvmcheck::waivers {

WaiverEngine::WaiverEngine(const slang::SourceManager& sm)
    : sourceManager(sm) {}

std::string WaiverEngine::computeFingerprint(const core::Violation& v) const {
    // Extract base filename for portability across different CI working directories
    std::string filename = v.filePath;
    size_t lastSlash = filename.find_last_of("/\\");
    if (lastSlash != std::string::npos) {
        filename = filename.substr(lastSlash + 1);
    }
    return v.ruleId + "@" + filename + ":" + std::to_string(v.line);
}

bool WaiverEngine::hasInlineWaiver(const core::Violation& v) const {
    if (v.snippet.empty()) return false;

    // Supported inline waiver patterns:
    // // uvmcheck:waive RULE_ID
    // // uvmcheck:disable RULE_ID
    // // uvmcheck-disable-line RULE_ID
    // // uvmcheck-disable-next-line RULE_ID
    static const std::vector<std::string> waiverKeywords = {
        "uvmcheck:waive",
        "uvmcheck:disable",
        "uvmcheck-disable",
        "lint:waive",
        "lint:off"
    };

    for (const auto& kw : waiverKeywords) {
        size_t pos = v.snippet.find(kw);
        if (pos != std::string::npos) {
            std::string_view remainder = std::string_view(v.snippet).substr(pos + kw.length());
            // Check if rule ID is present or wildcard '*'
            if (remainder.find(v.ruleId) != std::string_view::npos ||
                remainder.find("*") != std::string_view::npos) {
                return true;
            }
        }
    }

    return false;
}

bool WaiverEngine::matchesExternalWaiver(const core::Violation& v) const {
    for (const auto& w : externalWaivers) {
        if (w.ruleId != "*" && w.ruleId != v.ruleId) {
            continue;
        }

        if (w.line != 0 && w.line != v.line) {
            continue;
        }

        if (w.glob) {
            if (!w.glob->matches(core::normalizePathForMatching(v.filePath))) continue;
        } else if (!w.pathPattern.empty()) {
            std::string pat = w.pathPattern;
            // Trim leading/trailing asterisks for flexible matching
            while (!pat.empty() && pat.front() == '*') pat.erase(0, 1);
            while (!pat.empty() && pat.back() == '*') pat.pop_back();

            if (!pat.empty() && v.filePath.find(pat) == std::string::npos) {
                continue;
            }
        }

        return true;
    }
    return false;
}

bool WaiverEngine::isWaived(const core::Violation& v) const {
    if (hasInlineWaiver(v)) {
        waivedCount++;
        return true;
    }

    if (matchesExternalWaiver(v)) {
        waivedCount++;
        return true;
    }

    std::string fp = computeFingerprint(v);
    if (baselineFingerprints.count(fp) > 0) {
        waivedCount++;
        return true;
    }

    return false;
}

std::vector<core::Violation> WaiverEngine::filter(const std::vector<core::Violation>& violations) const {
    std::vector<core::Violation> active;
    waivedCount = 0;
    for (const auto& v : violations) {
        if (!isWaived(v)) {
            active.push_back(v);
        }
    }
    return active;
}

bool WaiverEngine::loadWaivers(const std::string& waiverFilePath) {
    std::ifstream file(waiverFilePath);
    if (!file.is_open()) return false;

    std::string content((std::istreambuf_iterator<char>(file)),
                         std::istreambuf_iterator<char>());

    size_t pos = 0;
    while ((pos = content.find('{', pos)) != std::string::npos) {
        size_t endObj = content.find('}', pos);
        if (endObj == std::string::npos) break;

        std::string obj = content.substr(pos, endObj - pos + 1);
        pos = endObj + 1;

        auto extractStr = [&](std::string_view key) -> std::string {
            size_t kpos = obj.find("\"" + std::string(key) + "\"");
            if (kpos == std::string::npos) return "";
            size_t colon = obj.find(':', kpos);
            if (colon == std::string::npos) return "";
            size_t q1 = obj.find('"', colon);
            if (q1 == std::string::npos || q1 >= obj.length()) return "";
            size_t q2 = obj.find('"', q1 + 1);
            if (q2 == std::string::npos) return "";
            return obj.substr(q1 + 1, q2 - q1 - 1);
        };

        auto extractInt = [&](std::string_view key) -> int {
            size_t kpos = obj.find("\"" + std::string(key) + "\"");
            if (kpos == std::string::npos) return 0;
            size_t colon = obj.find(':', kpos);
            if (colon == std::string::npos) return 0;
            size_t numStart = obj.find_first_of("0123456789", colon);
            if (numStart == std::string::npos) return 0;
            size_t numEnd = obj.find_first_not_of("0123456789", numStart);
            if (numEnd == std::string::npos) numEnd = obj.length();
            try {
                return std::stoi(obj.substr(numStart, numEnd - numStart));
            } catch (...) {
                return 0;
            }
        };

        std::string rule = extractStr("rule");
        if (rule.empty()) rule = extractStr("id");
        if (rule.empty()) continue; // skip non-waiver objects

        ExternalWaiver w;
        w.ruleId = rule;
        w.pathPattern = extractStr("path");
        if (w.pathPattern.empty()) w.pathPattern = extractStr("file");
        w.line = static_cast<size_t>(std::max(0, extractInt("line")));
        w.reason = extractStr("reason");

        externalWaivers.push_back(std::move(w));
    }
    return true;
}

bool WaiverEngine::loadBaseline(const std::string& baselinePath) {
    std::ifstream file(baselinePath);
    if (!file.is_open()) return false;

    std::string line;
    while (std::getline(file, line)) {
        size_t pos = line.find("\"fingerprint\": \"");
        if (pos != std::string::npos) {
            size_t start = pos + 16;
            size_t end = line.rfind("\"");
            if (end != std::string::npos && end > start) {
                baselineFingerprints.insert(line.substr(start, end - start));
            }
        }
    }
    return true;
}

bool WaiverEngine::saveBaseline(const std::string& baselinePath, const std::vector<core::Violation>& violations) {
    std::ofstream file(baselinePath);
    if (!file.is_open()) return false;

    file << "{\n  \"version\": \"1.0\",\n  \"baseline\": [\n";
    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        file << "    {\n"
             << "      \"rule\": \"" << v.ruleId << "\",\n"
             << "      \"file\": \"" << v.filePath << "\",\n"
             << "      \"line\": " << v.line << ",\n"
             << "      \"fingerprint\": \"" << computeFingerprint(v) << "\"\n"
             << "    }" << (i + 1 < violations.size() ? "," : "") << "\n";
    }
    file << "  ]\n}\n";
    return true;
}

} // namespace uvmcheck::waivers
