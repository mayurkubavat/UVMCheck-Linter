#include "uvmcheck/core/ProjectConfig.h"
#include <cctype>
#include <cmath>
#include <cstdlib>

namespace uvmcheck::core {

namespace fs = std::filesystem;

namespace {

/// Expands $VAR, ${VAR} and a leading ~. Returns false if a variable is unset,
/// so that "$UVM_HOME/**" never silently degrades into "/**".
bool expandEnv(std::string_view in, std::string& out) {
    out.clear();
    size_t i = 0;
    if (!in.empty() && in[0] == '~' && (in.size() == 1 || in[1] == '/' || in[1] == '\\')) {
        const char* home = std::getenv("HOME");
        if (!home) return false;
        out += home;
        i = 1;
    }
    for (; i < in.size(); ++i) {
        if (in[i] != '$') {
            out += in[i];
            continue;
        }
        size_t nameStart = i + 1;
        size_t nameEnd;
        bool braced = nameStart < in.size() && in[nameStart] == '{';
        if (braced) {
            nameStart++;
            nameEnd = in.find('}', nameStart);
            if (nameEnd == std::string_view::npos) return false;
        } else {
            nameEnd = nameStart;
            while (nameEnd < in.size() &&
                   (std::isalnum(static_cast<unsigned char>(in[nameEnd])) || in[nameEnd] == '_')) {
                nameEnd++;
            }
        }
        if (nameEnd == nameStart) {
            out += '$';
            continue;
        }
        std::string name(in.substr(nameStart, nameEnd - nameStart));
        const char* value = std::getenv(name.c_str());
        if (!value || !*value) return false;
        out += value;
        i = braced ? nameEnd : nameEnd - 1;
    }
    return true;
}

bool segmentMatch(std::string_view pat, std::string_view s) {
    size_t p = 0, i = 0;
    size_t starP = std::string_view::npos, starI = 0;
    while (i < s.size()) {
        if (p < pat.size() && (pat[p] == '?' || pat[p] == s[i])) {
            p++;
            i++;
        } else if (p < pat.size() && pat[p] == '*') {
            starP = p++;
            starI = i;
        } else if (starP != std::string_view::npos) {
            p = starP + 1;
            i = ++starI;
        } else {
            return false;
        }
    }
    while (p < pat.size() && pat[p] == '*') p++;
    return p == pat.size();
}

std::vector<std::string_view> splitSegments(std::string_view s) {
    std::vector<std::string_view> out;
    size_t start = 0;
    while (true) {
        size_t slash = s.find('/', start);
        if (slash == std::string_view::npos) {
            out.push_back(s.substr(start));
            break;
        }
        out.push_back(s.substr(start, slash - start));
        start = slash + 1;
    }
    return out;
}

/// Matches glob segments against path segments. Once the glob is exhausted any
/// remaining path segments are accepted: a pattern naming a directory covers
/// everything beneath it.
bool matchSegments(const std::vector<std::string_view>& g, size_t gi,
                   const std::vector<std::string_view>& p, size_t pi) {
    while (gi < g.size()) {
        if (g[gi] == "**") {
            // Collapse consecutive '**'.
            while (gi + 1 < g.size() && g[gi + 1] == "**") gi++;
            if (gi + 1 == g.size()) return true;
            for (size_t k = pi; k <= p.size(); ++k) {
                if (matchSegments(g, gi + 1, p, k)) return true;
            }
            return false;
        }
        if (pi >= p.size() || !segmentMatch(g[gi], p[pi])) return false;
        gi++;
        pi++;
    }
    return true;
}

/// Lexically normalizes and resolves symlinks in the existing part of the path.
fs::path resolveSymlinks(const fs::path& p) {
    std::error_code ec;
    fs::path r = fs::weakly_canonical(p, ec);
    return ec ? p.lexically_normal() : r;
}

} // namespace

bool globMatch(std::string_view glob, std::string_view path) {
    if (glob.empty()) return false;
    return matchSegments(splitSegments(glob), 0, splitSegments(path), 0);
}

std::string normalizePathForMatching(std::string_view path) {
    std::error_code ec;
    fs::path p(path);
    fs::path abs = fs::absolute(p, ec);
    if (ec) abs = p;
    std::string s = resolveSymlinks(abs).generic_string();
    while (s.size() > 1 && s.back() == '/') s.pop_back();
    return s;
}

PathPattern::PathPattern(std::string_view pattern, const fs::path& baseDir) : original(pattern) {
    std::string expanded;
    if (!expandEnv(pattern, expanded)) {
        // Unresolvable: leave glob empty so the pattern never matches.
        return;
    }
    for (char& c : expanded) {
        if (c == '\\') c = '/';
    }
    while (expanded.size() > 1 && expanded.back() == '/') expanded.pop_back();
    if (expanded.empty()) return;

    if (expanded.find('/') == std::string::npos) {
        glob = "**/" + expanded;
        return;
    }

    fs::path p(expanded);
    if (p.is_relative()) {
        std::error_code ec;
        fs::path base = fs::absolute(baseDir.empty() ? fs::path(".") : baseDir, ec);
        p = base / p;
    }
    // Resolve symlinks in the wildcard-free prefix so patterns compare equal to
    // normalized file paths (e.g. macOS /tmp -> /private/tmp).
    fs::path prefix, rest;
    bool inWildcards = false;
    for (const auto& seg : p.lexically_normal()) {
        if (!inWildcards && seg.string().find_first_of("*?[") != std::string::npos) inWildcards = true;
        (inWildcards ? rest : prefix) /= seg;
    }
    glob = (resolveSymlinks(prefix) / rest).generic_string();
    while (glob.size() > 1 && glob.back() == '/') glob.pop_back();
}

bool PathPattern::matches(std::string_view normalizedPath) const {
    return globMatch(glob, normalizedPath);
}

bool PathSet::matches(std::string_view normalizedPath) const {
    for (const auto& p : patterns) {
        if (p.matches(normalizedPath)) return true;
    }
    return false;
}

const RuleSettings* ProjectConfig::findRule(std::string_view ruleId, std::string_view ruleName) const {
    if (rules.empty()) return nullptr;
    if (auto it = rules.find(std::string(ruleId)); it != rules.end()) return &it->second;
    if (!ruleName.empty()) {
        if (auto it = rules.find(std::string(ruleName)); it != rules.end()) return &it->second;
    }
    return nullptr;
}

bool ProjectConfig::isRuleEnabled(std::string_view ruleId, std::string_view ruleName) const {
    const auto* rs = findRule(ruleId, ruleName);
    return !(rs && rs->enabled.has_value() && !*rs->enabled);
}

std::optional<Severity> ProjectConfig::severityOverride(std::string_view ruleId) const {
    const auto* rs = findRule(ruleId);
    return rs ? rs->severity : std::nullopt;
}

SuppressionReason ProjectConfig::classifyPath(std::string_view normalizedPath) const {
    if (library.matches(normalizedPath)) return SuppressionReason::LibraryPath;
    if (exclude.matches(normalizedPath)) return SuppressionReason::ExcludedPath;
    return SuppressionReason::None;
}

bool ProjectConfig::isExcludedForRule(std::string_view ruleId, std::string_view normalizedPath) const {
    if (!anyRuleExcludes) return false;
    const auto* rs = findRule(ruleId);
    return rs && rs->excludePaths.matches(normalizedPath);
}

void ProjectConfig::resolveRuleNames(const std::unordered_map<std::string, std::string>& nameToId) {
    for (const auto& [name, id] : nameToId) {
        auto it = rules.find(name);
        if (it == rules.end() || rules.count(id)) continue;
        rules.emplace(id, std::move(it->second));
        rules.erase(it);
    }
}

const json::Value* ProjectConfig::findOption(std::string_view ruleId, std::string_view key) const {
    const auto* rs = findRule(ruleId);
    return rs ? rs->options.find(key) : nullptr;
}

int64_t ProjectConfig::ruleOptionInt(std::string_view ruleId, std::string_view key, int64_t defaultValue) const {
    const auto* v = findOption(ruleId, key);
    if (!v || !v->isNumber()) return defaultValue;
    return static_cast<int64_t>(std::llround(v->asNumber()));
}

bool ProjectConfig::ruleOptionBool(std::string_view ruleId, std::string_view key, bool defaultValue) const {
    const auto* v = findOption(ruleId, key);
    return (v && v->isBool()) ? v->asBool() : defaultValue;
}

std::string ProjectConfig::ruleOptionString(std::string_view ruleId, std::string_view key,
                                            std::string_view defaultValue) const {
    const auto* v = findOption(ruleId, key);
    return (v && v->isString()) ? v->asString() : std::string(defaultValue);
}

} // namespace uvmcheck::core
