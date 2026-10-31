#include "uvmcheck/waivers/WaiverEngine.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <map>
#include <numeric>
#include <sstream>
#include <tuple>

#include "uvmcheck/core/Json.h"

namespace fs = std::filesystem;

namespace uvmcheck::waivers {

namespace {

std::string escapeJson(std::string_view s) {
    std::string out;
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
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

/// Whitespace runs collapsed to one space, trimmed.
std::string collapseWhitespace(std::string_view s) {
    std::string out;
    bool space = false;
    for (char c : s) {
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
            space = !out.empty();
            continue;
        }
        if (space) out += ' ';
        space = false;
        out += c;
    }
    return out;
}

/// 64-bit FNV-1a, hex.
std::string hashHex(std::string_view s) {
    uint64_t h = 1469598103934665603ULL;
    for (unsigned char c : s) {
        h ^= c;
        h *= 1099511628211ULL;
    }
    char buf[17];
    std::snprintf(buf, sizeof(buf), "%016llx", static_cast<unsigned long long>(h));
    return buf;
}

std::optional<core::json::Value> parseFile(const std::string& path, std::string& error) {
    std::ifstream file(path);
    if (!file.is_open()) {
        error = "cannot open " + path;
        return std::nullopt;
    }
    std::string text((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    core::json::ParseError pe;
    auto v = core::json::parse(text, pe);
    if (!v) error = path + ":" + std::to_string(pe.line) + ":" + std::to_string(pe.column) + ": " + pe.message;
    return v;
}

std::string str(const core::json::Value& obj, std::string_view key) {
    const auto* v = obj.find(key);
    return v && v->isString() ? v->asString() : std::string();
}

/// True if `text` holds `keyword` followed (anywhere after it) by the rule ID or '*'.
bool waiverCommentCovers(std::string_view text, std::string_view keyword, std::string_view ruleId) {
    for (size_t pos = text.find(keyword); pos != std::string_view::npos;
         pos = text.find(keyword, pos + 1)) {
        std::string_view rest = text.substr(pos + keyword.size());
        if (rest.find(ruleId) != std::string_view::npos || rest.find('*') != std::string_view::npos)
            return true;
    }
    return false;
}

} // namespace

WaiverEngine::WaiverEngine(const slang::SourceManager& sm) : sourceManager(sm) {}

void WaiverEngine::setFileText(const std::string& filePath, std::string_view text) {
    std::vector<std::string> lines;
    size_t pos = 0;
    while (pos <= text.size()) {
        size_t end = text.find('\n', pos);
        std::string_view l = text.substr(pos, end == std::string_view::npos ? std::string_view::npos : end - pos);
        if (!l.empty() && l.back() == '\r') l.remove_suffix(1);
        lines.emplace_back(l);
        if (end == std::string_view::npos) break;
        pos = end + 1;
    }
    fileLines[filePath] = std::move(lines);
}

std::string_view WaiverEngine::lineText(const core::Violation& v, size_t line) {
    if (line == v.line && !v.snippet.empty()) return v.snippet;
    if (v.filePath.empty() || line == 0) return {};
    auto it = fileLines.find(v.filePath);
    if (it == fileLines.end()) {
        std::vector<std::string> lines;
        std::ifstream in(v.filePath);
        for (std::string l; std::getline(in, l);) lines.push_back(std::move(l));
        it = fileLines.emplace(v.filePath, std::move(lines)).first;
    }
    return line <= it->second.size() ? std::string_view(it->second[line - 1]) : std::string_view();
}

std::string WaiverEngine::legacyFingerprint(const core::Violation& v) const {
    std::string filename = v.filePath;
    size_t lastSlash = filename.find_last_of("/\\");
    if (lastSlash != std::string::npos) filename = filename.substr(lastSlash + 1);
    return v.ruleId + "@" + filename + ":" + std::to_string(v.line);
}

std::string WaiverEngine::baselinePath(std::string_view filePath) const {
    if (filePath.empty()) return {};
    std::error_code ec;
    fs::path abs = fs::weakly_canonical(fs::absolute(fs::path(filePath), ec), ec);
    fs::path root = fs::weakly_canonical(fs::absolute(projectRoot, ec), ec);
    fs::path rel = abs.lexically_relative(root);
    if (!rel.empty() && *rel.begin() != "..") return rel.generic_string();
    return abs.generic_string();
}

std::vector<std::string> WaiverEngine::fingerprints(const std::vector<core::Violation>& violations) {
    // Occurrence index among identical (rule, file, line text) findings, counted in source order.
    std::vector<size_t> order(violations.size());
    std::iota(order.begin(), order.end(), 0);
    std::stable_sort(order.begin(), order.end(), [&](size_t a, size_t b) {
        const auto& x = violations[a];
        const auto& y = violations[b];
        return std::tie(x.filePath, x.line, x.column) < std::tie(y.filePath, y.line, y.column);
    });

    std::vector<std::string> result(violations.size());
    std::map<std::string, size_t> seen;
    for (size_t i : order) {
        const auto& v = violations[i];
        std::string key = v.ruleId + '\n' + baselinePath(v.filePath) + '\n' +
                          collapseWhitespace(lineText(v, v.line));
        size_t occurrence = seen[key]++;
        result[i] = hashHex(key + '\n' + std::to_string(occurrence));
    }
    return result;
}

bool WaiverEngine::hasInlineWaiver(const core::Violation& v) {
    // On the finding's own line:
    //   // uvmcheck:waive RULE_ID     // uvmcheck:disable RULE_ID     // lint:off RULE_ID
    // On the line above:
    //   // uvmcheck-disable-next-line RULE_ID     // uvmcheck:waive-next-line RULE_ID
    static constexpr std::string_view sameLine[] = {"uvmcheck:waive", "uvmcheck:disable",
                                                    "uvmcheck-disable", "lint:waive", "lint:off"};
    static constexpr std::string_view nextLine[] = {"uvmcheck-disable-next-line",
                                                    "uvmcheck:waive-next-line",
                                                    "uvmcheck:disable-next-line"};
    std::string_view here = lineText(v, v.line);
    for (auto kw : sameLine) {
        if (waiverCommentCovers(here, kw, v.ruleId)) return true;
    }
    if (v.line > 1) {
        std::string_view above = lineText(v, v.line - 1);
        for (auto kw : nextLine) {
            if (waiverCommentCovers(above, kw, v.ruleId)) return true;
        }
    }
    return false;
}

bool WaiverEngine::matchesExternalWaiver(const core::Violation& v) const {
    for (const auto& w : externalWaivers) {
        if (w.ruleId != "*" && w.ruleId != v.ruleId) continue;
        if (w.line != 0 && w.line != v.line) continue;

        if (w.glob) {
            if (!w.glob->matches(core::normalizePathForMatching(v.filePath))) continue;
        } else if (!w.pathPattern.empty()) {
            std::string pat = w.pathPattern;
            // Trim leading/trailing asterisks for flexible matching
            while (!pat.empty() && pat.front() == '*') pat.erase(0, 1);
            while (!pat.empty() && pat.back() == '*') pat.pop_back();
            if (!pat.empty() && v.filePath.find(pat) == std::string::npos) continue;
        }
        return true;
    }
    return false;
}

std::vector<core::Violation> WaiverEngine::filter(const std::vector<core::Violation>& violations) {
    std::vector<core::Violation> active;
    waivedCount = 0;
    matchedFingerprints.clear();
    const bool useBaseline = !baselineFingerprints.empty() || !legacyFingerprints.empty();
    auto fps = useBaseline ? fingerprints(violations) : std::vector<std::string>(violations.size());

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        bool waived = hasInlineWaiver(v) || matchesExternalWaiver(v);
        if (!waived && useBaseline) {
            if (baselineFingerprints.count(fps[i])) {
                matchedFingerprints.insert(fps[i]);
                waived = true;
            } else if (auto legacy = legacyFingerprint(v); legacyFingerprints.count(legacy)) {
                matchedFingerprints.insert(legacy);
                waived = true;
            }
        }
        if (waived) {
            waivedCount++;
        } else {
            active.push_back(v);
        }
    }
    return active;
}

size_t WaiverEngine::getStaleBaselineCount() const {
    size_t total = baselineFingerprints.size() + legacyFingerprints.size();
    return total > matchedFingerprints.size() ? total - matchedFingerprints.size() : 0;
}

bool WaiverEngine::loadWaivers(const std::string& waiverFilePath) {
    auto root = parseFile(waiverFilePath, lastError);
    if (!root) return false;

    const core::json::Value* list = &*root;
    if (root->isObject()) list = root->find("waivers");
    if (!list || !list->isArray()) {
        lastError = waiverFilePath + ": expected a list of waivers, or an object with \"waivers\"";
        return false;
    }

    for (const auto& obj : list->asArray()) {
        if (!obj.isObject()) continue;
        ExternalWaiver w;
        w.ruleId = str(obj, "rule");
        if (w.ruleId.empty()) w.ruleId = str(obj, "id");
        if (w.ruleId.empty()) continue;
        w.pathPattern = str(obj, "path");
        if (w.pathPattern.empty()) w.pathPattern = str(obj, "file");
        if (const auto* l = obj.find("line"); l && l->isNumber() && l->asNumber() > 0)
            w.line = static_cast<size_t>(l->asNumber());
        w.reason = str(obj, "reason");
        externalWaivers.push_back(std::move(w));
    }
    return true;
}

bool WaiverEngine::loadBaseline(const std::string& baselinePath) {
    auto root = parseFile(baselinePath, lastError);
    if (!root) return false;
    const auto* list = root->isObject() ? root->find("baseline") : nullptr;
    if (!list || !list->isArray()) {
        lastError = baselinePath + ": expected an object with a \"baseline\" list";
        return false;
    }
    const bool legacy = str(*root, "version") == "1.0";
    for (const auto& entry : list->asArray()) {
        if (!entry.isObject()) continue;
        std::string fp = str(entry, "fingerprint");
        if (fp.empty()) continue;
        (legacy ? legacyFingerprints : baselineFingerprints).insert(std::move(fp));
    }
    return true;
}

bool WaiverEngine::saveBaseline(const std::string& path, const std::vector<core::Violation>& violations) {
    auto fps = fingerprints(violations);

    // Write in source order so the file diffs cleanly between runs.
    std::vector<size_t> order(violations.size());
    std::iota(order.begin(), order.end(), 0);
    std::stable_sort(order.begin(), order.end(), [&](size_t a, size_t b) {
        const auto& x = violations[a];
        const auto& y = violations[b];
        return std::tie(x.filePath, x.line, x.column, x.ruleId) <
               std::tie(y.filePath, y.line, y.column, y.ruleId);
    });

    std::ofstream file(path);
    if (!file.is_open()) return false;
    file << "{\n  \"version\": \"2.0\",\n"
         << "  \"note\": \"Known findings, matched by fingerprint (rule, file relative to the project "
            "root, text of the flagged line). Line numbers are for reading only.\",\n"
         << "  \"baseline\": [\n";
    for (size_t k = 0; k < order.size(); ++k) {
        const auto& v = violations[order[k]];
        file << "    {\"rule\": \"" << escapeJson(v.ruleId) << "\", \"file\": \""
             << escapeJson(baselinePath(v.filePath)) << "\", \"line\": " << v.line
             << ", \"fingerprint\": \"" << fps[order[k]] << "\",\n"
             << "     \"message\": \"" << escapeJson(v.message) << "\"}"
             << (k + 1 < order.size() ? "," : "") << "\n";
    }
    file << "  ]\n}\n";
    return true;
}

} // namespace uvmcheck::waivers
