#include "uvmcheck/waivers/WaiverEngine.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <regex>
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

/// Quoted message regex after a waiver keyword. No quote means "any message".
/// An unclosed quote is an error (err set, returns false).
bool extractMessagePattern(std::string_view rest, std::string& pattern, std::string& err) {
    pattern.clear();
    err.clear();
    size_t q = rest.find('"');
    if (q == std::string_view::npos) return true;
    std::string out;
    for (size_t i = q + 1; i < rest.size(); ++i) {
        if (rest[i] == '\\' && i + 1 < rest.size()) {
            out += rest[++i];
            continue;
        }
        if (rest[i] == '"') {
            pattern = std::move(out);
            return true;
        }
        out += rest[i];
    }
    err = "unterminated message regex";
    pattern = std::move(out);
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

const std::vector<std::string>& WaiverEngine::fileLinesOf(const std::string& path) {
    auto it = fileLines.find(path);
    if (it == fileLines.end()) {
        std::vector<std::string> lines;
        std::ifstream in(path);
        for (std::string l; std::getline(in, l);) {
            if (!l.empty() && l.back() == '\r') l.pop_back();
            lines.push_back(std::move(l));
        }
        it = fileLines.emplace(path, std::move(lines)).first;
    }
    return it->second;
}

std::string_view WaiverEngine::lineText(const core::Violation& v, size_t line) {
    if (line == v.line && !v.snippet.empty()) return v.snippet;
    if (v.filePath.empty() || line == 0) return {};
    const auto& lines = fileLinesOf(v.filePath);
    return line <= lines.size() ? std::string_view(lines[line - 1]) : std::string_view();
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

// On the finding's own line:
//   // uvmcheck:waive RULE_ID     // uvmcheck:disable RULE_ID     // lint:off RULE_ID
// On the line above:
//   // uvmcheck-disable-next-line RULE_ID     // uvmcheck:waive-next-line RULE_ID
static constexpr std::string_view kSameLine[] = {"uvmcheck:waive", "uvmcheck:disable",
                                                 "uvmcheck-disable", "lint:waive", "lint:off"};
static constexpr std::string_view kNextLine[] = {"uvmcheck-disable-next-line",
                                                 "uvmcheck:waive-next-line",
                                                 "uvmcheck:disable-next-line"};

const std::regex* WaiverEngine::messageRegexFor(const std::string& pattern, const std::string& file, size_t line) {
    auto it = compiledMessages.find(pattern);
    if (it == compiledMessages.end()) {
        std::shared_ptr<const std::regex> compiled;
        try {
            compiled = std::make_shared<const std::regex>(pattern);
        } catch (const std::regex_error& e) {
            badRegexes.push_back({file, line, pattern, e.what()});
        }
        it = compiledMessages.emplace(pattern, std::move(compiled)).first;
    } else if (!it->second) {
        // Already reported for this pattern. Still point at this comment if it is a new line.
        bool seen = false;
        for (const auto& err : badRegexes) {
            if (err.filePath == file && err.line == line && err.pattern == pattern) seen = true;
        }
        if (!seen) {
            badRegexes.push_back({file, line, pattern, "invalid regular expression"});
        }
    }
    return it->second.get();
}

bool WaiverEngine::commentWaives(std::string_view text, const core::Violation& v, size_t commentLine) {
    auto keywords = commentLine == v.line
                        ? std::vector<std::string_view>(std::begin(kSameLine), std::end(kSameLine))
                        : std::vector<std::string_view>(std::begin(kNextLine), std::end(kNextLine));
    // `// uvmcheck: waive RULE "regex"` — a space after the colon, same as uvmcheck:waive.
    keywords.push_back("uvmcheck: waive");

    for (auto kw : keywords) {
        for (size_t pos = text.find(kw); pos != std::string_view::npos; pos = text.find(kw, pos + 1)) {
            std::string_view rest = text.substr(pos + kw.size());
            // "uvmcheck:waive" is a prefix of "uvmcheck:waive-next-line"; don't treat that as same-line.
            if (!rest.empty() && (rest.front() == '-' || rest.front() == ':')) continue;
            if (rest.find(v.ruleId) == std::string_view::npos && rest.find('*') == std::string_view::npos) continue;
            std::string pattern, err;
            if (!extractMessagePattern(rest, pattern, err)) {
                badRegexes.push_back({v.filePath, commentLine, pattern, err});
                return false;
            }
            if (!pattern.empty()) {
                const std::regex* re = messageRegexFor(pattern, v.filePath, commentLine);
                if (!re || !std::regex_search(v.message.begin(), v.message.end(), *re)) continue;
            }
            usedInline.insert(v.filePath + '\n' + std::to_string(commentLine));
            return true;
        }
    }
    return false;
}

bool WaiverEngine::hasInlineWaiver(const core::Violation& v) {
    if (commentWaives(lineText(v, v.line), v, v.line)) return true;
    if (v.line > 1 && commentWaives(lineText(v, v.line - 1), v, v.line - 1)) return true;
    return false;
}

void WaiverEngine::addWaiver(ExternalWaiver waiver) {
    if (waiver.label.empty()) waiver.label = "waiver for " + waiver.ruleId;
    if (!waiver.messagePattern.empty() && !waiver.messageRegex) {
        try {
            waiver.messageRegex = std::make_shared<const std::regex>(waiver.messagePattern);
        } catch (const std::regex_error& e) {
            lastError = "waiver " + waiver.label + ": invalid message regex '" + waiver.messagePattern +
                        "': " + e.what();
            return;
        }
    }
    if (waiver.group >= groupMatches.size()) groupMatches.resize(waiver.group + 1, 0);
    externalWaivers.push_back(std::move(waiver));
}

const ExternalWaiver* WaiverEngine::matchExternalWaiver(const core::Violation& v) {
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
        if (w.messageRegex && !std::regex_search(v.message.begin(), v.message.end(), *w.messageRegex)) continue;
        if (w.group < groupMatches.size()) groupMatches[w.group]++;
        return &w;
    }
    return nullptr;
}

std::vector<core::Violation> WaiverEngine::filter(const std::vector<core::Violation>& violations) {
    std::vector<core::Violation> active;
    waivedCount = 0;
    matchedFingerprints.clear();
    usedInline.clear();
    waivedByRule.clear();
    waivedByOwner.clear();
    std::fill(groupMatches.begin(), groupMatches.end(), 0);
    const bool useBaseline = !baselineFingerprints.empty() || !legacyFingerprints.empty();
    auto fps = useBaseline ? fingerprints(violations) : std::vector<std::string>(violations.size());

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        // Evaluate both, so a waiver shadowed by an inline comment still counts as used.
        const bool inlineWaived = hasInlineWaiver(v);
        const ExternalWaiver* external = matchExternalWaiver(v);
        bool waived = inlineWaived || external;
        if (waived) {
            waivedByRule[v.ruleId]++;
            waivedByOwner[external && !external->owner.empty() ? external->owner
                          : inlineWaived ? std::string("(inline comment)") : std::string("(no owner)")]++;
        }
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

WaiverHygiene WaiverEngine::hygiene(const std::vector<std::string>& files,
                                    const std::unordered_set<std::string>& activeRules) {
    WaiverHygiene h;
    std::vector<bool> seenGroup(groupMatches.size(), false), ownerless(groupMatches.size(), false);
    for (const auto& w : externalWaivers) {
        if (w.group >= seenGroup.size() || seenGroup[w.group]) continue;
        seenGroup[w.group] = true;
        h.written++;
        if (w.owner.empty()) h.withoutOwner++;
        if (groupMatches[w.group] == 0) h.unused.push_back(w.label);
    }
    for (const auto& [rule, n] : waivedByRule) h.byRule.emplace_back(rule, n);
    for (const auto& [owner, n] : waivedByOwner) h.byOwner.emplace_back(owner, n);
    auto byCount = [](const auto& a, const auto& b) { return a.second != b.second ? a.second > b.second : a.first < b.first; };
    std::sort(h.byRule.begin(), h.byRule.end(), byCount);
    std::sort(h.byOwner.begin(), h.byOwner.end(), byCount);

    // Waiver comments in user files: unused if they name an active rule and waived nothing.
    static const std::regex ruleRe(R"([A-Z][A-Z0-9]*-[A-Z][A-Z0-9]*-[0-9]+[A-Za-z0-9.]*|\*)");
    for (const auto& file : files) {
        const auto& lines = fileLinesOf(file);
        for (size_t i = 0; i < lines.size(); ++i) {
            const std::string& l = lines[i];
            size_t at = std::string::npos;
            for (auto kw : kNextLine) at = std::min(at, l.find(kw));
            for (auto kw : kSameLine) at = std::min(at, l.find(kw));
            if (at == std::string::npos) continue;
            size_t comment = l.rfind("//", at);
            if (comment == std::string::npos && l.rfind("/*", at) == std::string::npos) continue;
            h.inlineComments++;
            if (usedInline.count(file + '\n' + std::to_string(i + 1))) continue;
            bool namesActive = false;
            std::string rest = l.substr(at);
            for (std::sregex_iterator m(rest.begin(), rest.end(), ruleRe), end; m != end; ++m) {
                namesActive |= m->str() == "*" || activeRules.count(m->str()) > 0;
            }
            if (namesActive) {
                std::string text = l.substr(comment == std::string::npos ? at : comment);
                h.unusedInline.push_back({file, i + 1, text});
            }
        }
    }
    h.baselineEntries = baselineFingerprints.size() + legacyFingerprints.size();
    h.staleBaseline = getStaleBaselineCount();
    return h;
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
        w.owner = str(obj, "owner");
        w.messagePattern = str(obj, "message");
        w.group = groupMatches.size();
        w.label = fs::path(waiverFilePath).filename().string() + " #" + std::to_string(externalWaivers.size() + 1) +
                  " (" + w.ruleId + (w.pathPattern.empty() ? "" : " in " + w.pathPattern) + ")";
        if (!w.messagePattern.empty()) {
            try {
                w.messageRegex = std::make_shared<const std::regex>(w.messagePattern);
            } catch (const std::regex_error& e) {
                lastError = waiverFilePath + ": waiver " + w.label + ": invalid message regex '" +
                            w.messagePattern + "': " + e.what();
                return false;
            }
        }
        addWaiver(std::move(w));
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
