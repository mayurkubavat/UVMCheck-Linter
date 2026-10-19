#include "uvmcheck/core/ConfigManager.h"
#include <fstream>
#include <sstream>
#include <filesystem>
#include <algorithm>

namespace uvmcheck::core {

namespace fs = std::filesystem;

ConfigManager& ConfigManager::instance() {
    static ConfigManager mgr;
    return mgr;
}

ConfigManager::ConfigManager() {}


static std::string extractJsonString(const std::string& block, const std::string& key) {
    std::string needle = "\"" + key + "\"";
    size_t pos = block.find(needle);
    if (pos == std::string::npos) return "";
    size_t colon = block.find(':', pos);
    if (colon == std::string::npos) return "";
    size_t quoteStart = block.find('"', colon);
    if (quoteStart == std::string::npos) return "";
    size_t quoteEnd = block.find('"', quoteStart + 1);
    if (quoteEnd == std::string::npos) return "";
    return block.substr(quoteStart + 1, quoteEnd - quoteStart - 1);
}

static bool extractJsonBool(const std::string& block, const std::string& key, bool defaultVal) {
    std::string needle = "\"" + key + "\"";
    size_t pos = block.find(needle);
    if (pos == std::string::npos) return defaultVal;
    size_t colon = block.find(':', pos);
    if (colon == std::string::npos) return defaultVal;
    size_t truePos = block.find("true", colon);
    size_t falsePos = block.find("false", colon);
    size_t comma = block.find(',', colon);
    size_t brace = block.find('}', colon);
    size_t limit = std::min(comma, brace);

    if (truePos != std::string::npos && truePos < limit) return true;
    if (falsePos != std::string::npos && falsePos < limit) return false;
    return defaultVal;
}

static size_t extractJsonInt(const std::string& block, const std::string& key, size_t defaultVal) {
    std::string needle = "\"" + key + "\"";
    size_t pos = block.find(needle);
    if (pos == std::string::npos) return defaultVal;
    size_t colon = block.find(':', pos);
    if (colon == std::string::npos) return defaultVal;

    size_t start = block.find_first_of("0123456789", colon);
    if (start == std::string::npos) return defaultVal;
    size_t end = block.find_first_not_of("0123456789", start);
    std::string numStr = (end == std::string::npos) ? block.substr(start) : block.substr(start, end - start);
    try {
        return std::stoul(numStr);
    } catch (...) {
        return defaultVal;
    }
}

bool ConfigManager::parseJsonConfig(const std::string& content) {
    // 1. Parse Style Block
    size_t stylePos = content.find("\"style\"");
    if (stylePos != std::string::npos) {
        size_t openBrace = content.find('{', stylePos);
        size_t closeBrace = content.find('}', openBrace);
        if (openBrace != std::string::npos && closeBrace != std::string::npos) {
            std::string styleBlock = content.substr(openBrace, closeBrace - openBrace + 1);

            style::StyleConfiguration cfg = styleEngine.getConfig();
            cfg.maxLineLength = extractJsonInt(styleBlock, "max_line_length", cfg.maxLineLength);
            cfg.allowTabs = extractJsonBool(styleBlock, "allow_tabs", cfg.allowTabs);
            cfg.allowTrailingWhitespace = extractJsonBool(styleBlock, "allow_trailing_whitespace", cfg.allowTrailingWhitespace);
            cfg.oneStatementPerLine = extractJsonBool(styleBlock, "one_statement_per_line", cfg.oneStatementPerLine);
            cfg.mandatoryMatchingEndLabel = extractJsonBool(styleBlock, "mandatory_matching_end_label", cfg.mandatoryMatchingEndLabel);
            cfg.requireFileHeader = extractJsonBool(styleBlock, "require_file_header", cfg.requireFileHeader);
            std::string headerPat = extractJsonString(styleBlock, "file_header_pattern");
            if (!headerPat.empty()) cfg.fileHeaderPattern = headerPat;
            cfg.fileMatchesClassName = extractJsonBool(styleBlock, "file_matches_class_name", cfg.fileMatchesClassName);

            styleEngine.setConfig(cfg);
        }
    }

    // 2. Parse Naming Block
    size_t namingPos = content.find("\"naming\"");
    if (namingPos != std::string::npos) {
        size_t openBrace = content.find('{', namingPos);
        if (openBrace != std::string::npos) {
            size_t depth = 1;
            size_t i = openBrace + 1;
            while (i < content.size() && depth > 0) {
                if (content[i] == '{') depth++;
                else if (content[i] == '}') depth--;
                i++;
            }
            std::string namingBlock = content.substr(openBrace, i - openBrace);

            // Extract each target kind block
            static const std::vector<std::string> knownTargets = {
                "Package", "Interface", "VirtualInterface", "Class",
                "UvmDriver", "UvmMonitor", "UvmSequencer", "UvmSequence",
                "UvmSequenceItem", "UvmAgent", "UvmEnv", "UvmTest",
                "UvmScoreboard", "Covergroup", "Coverpoint", "Cross",
                "Assertion", "Parameter", "TypedefEnum", "Typedef", "Constraint"
            };

            for (const auto& target : knownTargets) {
                size_t tPos = namingBlock.find("\"" + target + "\"");
                if (tPos != std::string::npos) {
                    size_t tbOpen = namingBlock.find('{', tPos);
                    size_t tbClose = namingBlock.find('}', tbOpen);
                    if (tbOpen != std::string::npos && tbClose != std::string::npos) {
                        std::string tBlock = namingBlock.substr(tbOpen, tbClose - tbOpen + 1);

                        style::NamingTargetSpec spec;
                        if (const auto* existing = namingEngine.getRule(target)) {
                            spec = *existing;
                        } else {
                            spec.targetKind = target;
                        }

                        std::string prefix = extractJsonString(tBlock, "prefix");
                        if (!prefix.empty()) spec.prefix = prefix;

                        std::string suffix = extractJsonString(tBlock, "suffix");
                        if (!suffix.empty()) spec.suffix = suffix;

                        std::string pattern = extractJsonString(tBlock, "pattern");
                        if (!pattern.empty()) spec.regexPattern = pattern;

                        std::string casingStr = extractJsonString(tBlock, "casing");
                        if (!casingStr.empty()) {
                            if (casingStr == "snake_case") spec.casing = style::CasingStyle::SnakeCase;
                            else if (casingStr == "PascalCase") spec.casing = style::CasingStyle::PascalCase;
                            else if (casingStr == "camelCase") spec.casing = style::CasingStyle::CamelCase;
                            else if (casingStr == "ALL_CAPS") spec.casing = style::CasingStyle::AllCaps;
                        }

                        std::string sevStr = extractJsonString(tBlock, "severity");
                        if (!sevStr.empty()) {
                            if (sevStr == "ERROR") spec.severity = core::Severity::Error;
                            else if (sevStr == "WARNING") spec.severity = core::Severity::Warning;
                            else if (sevStr == "INFO") spec.severity = core::Severity::Info;
                        }

                        spec.enabled = extractJsonBool(tBlock, "enabled", spec.enabled);
                        namingEngine.setRule(target, spec);
                    }
                }
            }
        }
    }

    configLoaded = true;
    return true;
}

bool ConfigManager::loadFromFile(const std::string& filePath) {
    std::ifstream in(filePath);
    if (!in.is_open()) return false;

    std::string content((std::istreambuf_iterator<char>(in)),
                         std::istreambuf_iterator<char>());
    in.close();

    if (parseJsonConfig(content)) {
        loadedConfigPath = filePath;
        return true;
    }
    return false;
}

bool ConfigManager::autoDiscover(const std::string& startDir) {
    fs::path curr(startDir);
    while (true) {
        fs::path p1 = curr / ".uvmcheck.json";
        if (fs::exists(p1)) {
            return loadFromFile(p1.string());
        }

        fs::path p2 = curr / "uvmcheck.json";
        if (fs::exists(p2)) {
            return loadFromFile(p2.string());
        }

        if (curr == curr.parent_path() || curr.empty()) {
            break;
        }
        curr = curr.parent_path();
    }
    return false;
}

void ConfigManager::dumpDefaultConfig(std::ostream& os) const {
    os << "{\n"
       << "  \"style\": {\n"
       << "    \"max_line_length\": 120,\n"
       << "    \"allow_tabs\": false,\n"
       << "    \"allow_trailing_whitespace\": false,\n"
       << "    \"one_statement_per_line\": true,\n"
       << "    \"mandatory_matching_end_label\": true,\n"
       << "    \"require_file_header\": false,\n"
       << "    \"file_header_pattern\": \"Copyright|SPDX-License-Identifier\"\n"
       << "  },\n"
       << "  \"naming\": {\n"
       << "    \"Package\": { \"suffix\": \"_pkg\", \"casing\": \"snake_case\", \"severity\": \"WARNING\" },\n"
       << "    \"Interface\": { \"suffix\": \"_if\", \"casing\": \"snake_case\", \"severity\": \"WARNING\" },\n"
       << "    \"VirtualInterface\": { \"suffix\": \"_vif\", \"casing\": \"snake_case\", \"severity\": \"WARNING\" },\n"
       << "    \"Class\": { \"casing\": \"PascalCase\", \"enabled\": false },\n"
       << "    \"UvmDriver\": { \"suffix\": \"_driver\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmMonitor\": { \"suffix\": \"_monitor\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmSequencer\": { \"suffix\": \"_sequencer\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmSequence\": { \"suffix\": \"_seq\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmSequenceItem\": { \"suffix\": \"_item\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmAgent\": { \"suffix\": \"_agent\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmEnv\": { \"suffix\": \"_env\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmTest\": { \"suffix\": \"_test\", \"severity\": \"WARNING\" },\n"
       << "    \"UvmScoreboard\": { \"suffix\": \"_scoreboard\", \"severity\": \"WARNING\" },\n"
       << "    \"Covergroup\": { \"suffix\": \"_cg\", \"severity\": \"WARNING\" },\n"
       << "    \"Coverpoint\": { \"prefix\": \"cp_\", \"severity\": \"WARNING\" },\n"
       << "    \"Cross\": { \"prefix\": \"cross_\", \"severity\": \"WARNING\" },\n"
       << "    \"Assertion\": { \"suffix\": \"_a\", \"severity\": \"WARNING\" },\n"
       << "    \"Parameter\": { \"casing\": \"ALL_CAPS\", \"severity\": \"WARNING\" },\n"
       << "    \"TypedefEnum\": { \"suffix\": \"_e\", \"severity\": \"WARNING\" },\n"
       << "    \"Typedef\": { \"suffix\": \"_t\", \"severity\": \"WARNING\" },\n"
       << "    \"Constraint\": { \"suffix\": \"_c\", \"severity\": \"WARNING\" }\n"
       << "  }\n"
       << "}\n";
}

} // namespace uvmcheck::core
