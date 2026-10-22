#include "uvmcheck/core/ConfigManager.h"
#include <fstream>
#include <sstream>
#include <filesystem>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <unordered_set>
#include <unistd.h>

#ifndef UVMCHECK_PYTHON_SDK_DIR
#define UVMCHECK_PYTHON_SDK_DIR ""
#endif

namespace uvmcheck::core {

namespace fs = std::filesystem;

ConfigManager& ConfigManager::instance() {
    static ConfigManager mgr;
    return mgr;
}

ConfigManager::ConfigManager() {}

namespace {

std::string toUpper(std::string s) {
    for (char& c : s) c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
    return s;
}

std::optional<Severity> parseSeverity(const std::string& s) {
    std::string u = toUpper(s);
    if (u == "ERROR") return Severity::Error;
    if (u == "WARNING") return Severity::Warning;
    if (u == "INFO") return Severity::Info;
    return std::nullopt;
}

std::string shellQuote(const std::string& s) {
    std::string out = "'";
    for (char c : s) {
        if (c == '\'') out += "'\\''";
        else out += c;
    }
    return out + "'";
}

bool readFile(const std::string& path, std::string& out) {
    std::ifstream in(path, std::ios::binary);
    if (!in.is_open()) return false;
    out.assign(std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>());
    return true;
}

bool isPythonConfig(const std::string& path) {
    return fs::path(path).extension() == ".py";
}

/// Locates the directory holding the `uvmcheck` Python package.
std::string findPythonSdkDir() {
    auto hasSdk = [](const fs::path& dir) {
        std::error_code ec;
        return fs::exists(dir / "uvmcheck" / "config_runner.py", ec);
    };
    if (const char* env = std::getenv("UVMCHECK_PYTHON_SDK")) {
        if (hasSdk(env)) return env;
    }
    std::error_code ec;
    fs::path exe = fs::read_symlink("/proc/self/exe", ec);
    if (!ec) {
        for (const fs::path& cand : {exe.parent_path() / ".." / "python", exe.parent_path() / "python",
                                     exe.parent_path() / ".." / "share" / "uvmcheck" / "python"}) {
            if (hasSdk(cand)) return cand.lexically_normal().string();
        }
    }
    if (hasSdk(UVMCHECK_PYTHON_SDK_DIR)) return UVMCHECK_PYTHON_SDK_DIR;
    return ""; // rely on an installed `uvmcheck` package
}

} // namespace

bool ConfigManager::runPythonConfig(const std::string& filePath, std::string& jsonOut) {
    fs::path outPath = fs::temp_directory_path() /
                       ("uvmcheck_config_" + std::to_string(getpid()) + ".json");

    const char* pyEnv = std::getenv("UVMCHECK_PYTHON");
    std::string python = (pyEnv && *pyEnv) ? pyEnv : "python3";
    std::string sdk = findPythonSdkDir();

    std::string cmd;
    if (!sdk.empty()) cmd += "PYTHONPATH=" + shellQuote(sdk) + "${PYTHONPATH:+:$PYTHONPATH} ";
    cmd += shellQuote(python) + " -m uvmcheck.config_runner --config " + shellQuote(filePath) +
           " --out " + shellQuote(outPath.string()) + " 2>&1";

    FILE* pipe = popen(cmd.c_str(), "r");
    if (!pipe) return error("failed to start " + python + " to evaluate " + filePath);

    std::string output;
    char buffer[1024];
    while (fgets(buffer, sizeof(buffer), pipe) != nullptr) output += buffer;
    int rc = pclose(pipe);

    bool ok = rc == 0 && readFile(outPath.string(), jsonOut);
    std::error_code ec;
    fs::remove(outPath, ec);
    if (!ok) {
        while (!output.empty() && output.back() == '\n') output.pop_back();
        return error("Python config " + filePath + " failed to load:\n" + output);
    }
    return true;
}

void ConfigManager::applyStyle(const json::Value& styleBlock) {
    style::StyleConfiguration cfg = styleEngine.getConfig();
    auto getBool = [&](const char* key, bool& field) {
        if (const auto* v = styleBlock.find(key); v && v->isBool()) field = v->asBool();
    };
    if (const auto* v = styleBlock.find("max_line_length"); v && v->isNumber() && v->asNumber() >= 0) {
        cfg.maxLineLength = static_cast<size_t>(v->asNumber());
    }
    getBool("allow_tabs", cfg.allowTabs);
    getBool("allow_trailing_whitespace", cfg.allowTrailingWhitespace);
    getBool("one_statement_per_line", cfg.oneStatementPerLine);
    getBool("mandatory_matching_end_label", cfg.mandatoryMatchingEndLabel);
    getBool("require_file_header", cfg.requireFileHeader);
    if (const auto* v = styleBlock.find("file_header_pattern"); v && v->isString() && !v->asString().empty()) {
        cfg.fileHeaderPattern = v->asString();
    }
    getBool("file_matches_class_name", cfg.fileMatchesClassName);
    styleEngine.setConfig(cfg);
}

void ConfigManager::applyNaming(const json::Value& namingBlock) {
    for (const auto& [target, tBlock] : namingBlock.asObject()) {
        if (!tBlock.isObject()) {
            warn("naming." + target + " should be an object; ignored");
            continue;
        }
        style::NamingTargetSpec spec;
        if (const auto* existing = namingEngine.getRule(target)) {
            spec = *existing;
        } else {
            spec.targetKind = target;
        }

        auto getStr = [&](const char* key) -> std::string {
            const auto* v = tBlock.find(key);
            return (v && v->isString()) ? v->asString() : std::string();
        };
        if (auto s = getStr("prefix"); !s.empty()) spec.prefix = s;
        if (auto s = getStr("suffix"); !s.empty()) spec.suffix = s;
        if (auto s = getStr("pattern"); !s.empty()) spec.regexPattern = s;
        if (auto s = getStr("casing"); !s.empty()) {
            if (s == "snake_case") spec.casing = style::CasingStyle::SnakeCase;
            else if (s == "PascalCase") spec.casing = style::CasingStyle::PascalCase;
            else if (s == "camelCase") spec.casing = style::CasingStyle::CamelCase;
            else if (s == "ALL_CAPS") spec.casing = style::CasingStyle::AllCaps;
            else warn("naming." + target + ".casing: unknown style '" + s + "'");
        }
        if (auto s = getStr("severity"); !s.empty()) {
            if (auto sev = parseSeverity(s)) spec.severity = *sev;
            else warn("naming." + target + ".severity: unknown severity '" + s + "'");
        }
        if (const auto* v = tBlock.find("enabled"); v && v->isBool()) spec.enabled = v->asBool();
        namingEngine.setRule(target, spec);
    }
}

bool ConfigManager::readPathList(const json::Value& v, std::string_view key, const fs::path& baseDir,
                                 PathSet& out) {
    std::vector<const json::Value*> items;
    if (v.isString()) {
        items.push_back(&v);
    } else if (v.isArray()) {
        for (const auto& item : v.asArray()) items.push_back(&item);
    } else {
        return error(std::string(key) + " must be a list of path strings");
    }
    for (const auto* item : items) {
        if (!item->isString()) return error(std::string(key) + " must contain only strings");
        PathPattern p(item->asString(), baseDir);
        if (!p.isResolved()) {
            warn(std::string(key) + ": '" + item->asString() +
                 "' uses an unset environment variable; pattern ignored");
            continue;
        }
        out.add(std::move(p));
    }
    return true;
}

bool ConfigManager::applyRules(const json::Value& rulesBlock, const fs::path& baseDir) {
    if (!rulesBlock.isObject()) return error("'rules' must be an object keyed by rule ID or name");

    for (const auto& [key, entry] : rulesBlock.asObject()) {
        RuleSettings rs;
        std::string where = "rules." + key;

        if (entry.isBool()) {
            rs.enabled = entry.asBool();
        } else if (entry.isString()) {
            // Shorthand: "off" or a severity name.
            std::string u = toUpper(entry.asString());
            if (u == "OFF" || u == "DISABLED") rs.enabled = false;
            else if (auto sev = parseSeverity(u)) rs.severity = sev;
            else return error(where + ": expected \"off\" or a severity, got \"" + entry.asString() + "\"");
        } else if (entry.isObject()) {
            for (const auto& [field, val] : entry.asObject()) {
                if (field == "enabled") {
                    if (!val.isBool()) return error(where + ".enabled must be true or false");
                    rs.enabled = val.asBool();
                } else if (field == "severity") {
                    if (val.isNull()) continue;
                    if (!val.isString() || !parseSeverity(val.asString())) {
                        return error(where + ".severity must be ERROR, WARNING or INFO");
                    }
                    rs.severity = parseSeverity(val.asString());
                } else if (field == "exclude_paths") {
                    if (!readPathList(val, where + ".exclude_paths", baseDir, rs.excludePaths)) return false;
                    if (!rs.excludePaths.empty()) projectConfig.noteRuleExcludes();
                } else if (field == "options") {
                    if (!val.isObject()) return error(where + ".options must be an object");
                    rs.options = val;
                } else {
                    warn(where + ": unknown setting '" + field + "' ignored");
                }
            }
        } else {
            return error(where + ": expected an object, true/false, or \"off\"");
        }
        projectConfig.ruleSettings()[key] = std::move(rs);
    }
    return true;
}

bool ConfigManager::applyConfig(const json::Value& root, const fs::path& baseDir) {
    if (!root.isObject()) return error("top level must be an object");

    static const std::unordered_set<std::string_view> knownKeys = {
        "version", "source", "library_paths", "exclude_paths", "report_library_macro_expansions",
        "rules", "waivers", "style", "naming"};
    for (const auto& [key, _] : root.asObject()) {
        if (!knownKeys.count(key)) warn("unknown top-level key '" + key + "' ignored");
    }

    if (const auto* v = root.find("library_paths")) {
        if (!readPathList(*v, "library_paths", baseDir, projectConfig.libraryPaths())) return false;
    }
    if (const auto* v = root.find("exclude_paths")) {
        if (!readPathList(*v, "exclude_paths", baseDir, projectConfig.excludePaths())) return false;
    }
    if (const auto* v = root.find("report_library_macro_expansions")) {
        if (!v->isBool()) return error("report_library_macro_expansions must be true or false");
        projectConfig.setReportLibraryMacroExpansions(v->asBool());
    }
    if (const auto* v = root.find("rules")) {
        if (!applyRules(*v, baseDir)) return false;
    }
    if (const auto* v = root.find("waivers")) {
        if (!v->isArray()) return error("'waivers' must be a list");
        for (const auto& w : v->asArray()) {
            const auto* rule = w.find("rule");
            const auto* reason = w.find("reason");
            if (!rule || !rule->isString() || rule->asString().empty()) {
                return error("every waiver needs a \"rule\"");
            }
            if (!reason || !reason->isString() || reason->asString().empty()) {
                return error("waiver for " + rule->asString() + " needs a \"reason\"");
            }
            ConfigWaiver cw;
            cw.ruleId = rule->asString();
            cw.reason = reason->asString();
            if (const auto* p = w.find("path"); p && p->isString() && !p->asString().empty()) {
                cw.path.emplace(p->asString(), baseDir);
                if (!cw.path->isResolved()) {
                    warn("waiver for " + cw.ruleId + ": path '" + p->asString() +
                         "' uses an unset environment variable; waiver ignored");
                    continue;
                }
            }
            if (const auto* l = w.find("line"); l && l->isNumber() && l->asNumber() >= 1) {
                cw.line = static_cast<size_t>(l->asNumber());
            }
            waivers.push_back(std::move(cw));
        }
    }
    if (const auto* v = root.find("style"); v && v->isObject()) applyStyle(*v);
    if (const auto* v = root.find("naming"); v && v->isObject()) applyNaming(*v);
    return true;
}

bool ConfigManager::loadFromFile(const std::string& filePath) {
    lastError.clear();
    std::string content;
    if (isPythonConfig(filePath)) {
        std::error_code ec;
        if (!fs::exists(filePath, ec)) return error("config file not found: " + filePath);
        if (!runPythonConfig(filePath, content)) return false;
    } else if (!readFile(filePath, content)) {
        return error("config file not found: " + filePath);
    }

    json::ParseError perr;
    auto root = json::parse(content, perr);
    if (!root) {
        return error(filePath + ":" + std::to_string(perr.line) + ":" + std::to_string(perr.column) +
                     ": invalid JSON: " + perr.message);
    }

    std::error_code ec;
    fs::path baseDir = fs::absolute(fs::path(filePath), ec).parent_path();
    if (!applyConfig(*root, baseDir)) {
        lastError = filePath + ": " + lastError;
        return false;
    }

    configLoaded = true;
    loadedConfigPath = filePath;
    return true;
}

bool ConfigManager::autoDiscover(const std::string& startDir) {
    static const char* const names[] = {"uvmcheck_config.py", ".uvmcheck.py", ".uvmcheck.json", "uvmcheck.json"};
    lastError.clear();
    std::error_code ec;
    fs::path curr = fs::absolute(startDir, ec).lexically_normal();
    if (!curr.has_filename() && curr.has_parent_path()) curr = curr.parent_path(); // "dir/." -> "dir"
    while (true) {
        for (const char* name : names) {
            fs::path p = curr / name;
            if (fs::exists(p, ec)) return loadFromFile(p.string());
        }
        if (curr == curr.parent_path() || curr.empty()) break;
        curr = curr.parent_path();
    }
    return false;
}

void ConfigManager::validateRuleKeys(const std::unordered_map<std::string, std::string>& nameToId,
                                     const std::vector<std::string>& knownIds) {
    std::unordered_set<std::string> ids(knownIds.begin(), knownIds.end());
    for (const auto& [key, _] : projectConfig.ruleSettings()) {
        if (!ids.count(key) && !nameToId.count(key)) {
            warn("rules: '" + key + "' does not match any rule ID or name");
        }
    }
    projectConfig.resolveRuleNames(nameToId);
}

void ConfigManager::dumpDefaultConfig(std::ostream& os) const {
    os << R"PY("""UVMCheck project configuration.

Save as uvmcheck_config.py at the root of your verification project; uvmcheck
finds it automatically (or pass --config path/to/file.py). Paths are relative
to this file and may use $ENV_VARS, * / ? within a name, and ** across folders.
"""
from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity


class Project(ProjectConfig):
    # Third-party code: compiled so your testbench elaborates, never reported.
    library_paths = [
        "$UVM_HOME",
        # "vip/**",
    ]

    # Code generated by library macros (`uvm_object_utils, ...) in your files is
    # treated as library code. Set True to report it anyway.
    report_library_macro_expansions = False

    # Your own files that should not be linted at all.
    exclude_paths = [
        # "**/generated/**",
    ]

    # Per-rule settings. Name the class after the rule (ID via `rule = ...`).
    # Any extra attribute is passed to the rule as an option.
    class MaxLineLength(RuleConfig):
        max_columns = 120

    # class NoCasex(RuleConfig):
    #     rule = "SV-COR-009"
    #     severity = Severity.Error
    #     exclude_paths = ["legacy/**"]

    # class GenericReportId(RuleConfig):
    #     enabled = False

    waivers = [
        # Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142,
        #        reason="Objection in loop verified for stress bench"),
    ]

    style = {
        "max_line_length": 120,
        "allow_tabs": False,
        "allow_trailing_whitespace": False,
        "one_statement_per_line": True,
        "mandatory_matching_end_label": True,
        "require_file_header": False,
        "file_header_pattern": "Copyright|SPDX-License-Identifier",
    }

    naming = {
        "Package": {"suffix": "_pkg", "casing": "snake_case", "severity": "WARNING"},
        "Interface": {"suffix": "_if", "casing": "snake_case", "severity": "WARNING"},
        "VirtualInterface": {"suffix": "_vif", "casing": "snake_case", "severity": "WARNING"},
        "Class": {"casing": "PascalCase", "enabled": False},
        "UvmDriver": {"suffix": "_driver", "severity": "WARNING"},
        "UvmMonitor": {"suffix": "_monitor", "severity": "WARNING"},
        "UvmSequencer": {"suffix": "_sequencer", "severity": "WARNING"},
        "UvmSequence": {"suffix": "_seq", "severity": "WARNING"},
        "UvmSequenceItem": {"suffix": "_item", "severity": "WARNING"},
        "UvmAgent": {"suffix": "_agent", "severity": "WARNING"},
        "UvmEnv": {"suffix": "_env", "severity": "WARNING"},
        "UvmTest": {"suffix": "_test", "severity": "WARNING"},
        "UvmScoreboard": {"suffix": "_scoreboard", "severity": "WARNING"},
        "Covergroup": {"suffix": "_cg", "severity": "WARNING"},
        "Coverpoint": {"prefix": "cp_", "severity": "WARNING"},
        "Cross": {"prefix": "cross_", "severity": "WARNING"},
        "Assertion": {"suffix": "_a", "severity": "WARNING"},
        "Parameter": {"casing": "ALL_CAPS", "severity": "WARNING"},
        "TypedefEnum": {"suffix": "_e", "severity": "WARNING"},
        "Typedef": {"suffix": "_t", "severity": "WARNING"},
        "Constraint": {"suffix": "_c", "severity": "WARNING"},
    }
)PY";
}

} // namespace uvmcheck::core
