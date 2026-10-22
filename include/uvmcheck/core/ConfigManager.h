#pragma once

#include <string>
#include <string_view>
#include <memory>
#include <iostream>
#include <unordered_map>
#include <vector>
#include "uvmcheck/core/Json.h"
#include "uvmcheck/core/ProjectConfig.h"
#include "uvmcheck/style/NamingEngine.h"
#include "uvmcheck/style/StyleEngine.h"

namespace uvmcheck::core {

/// A waiver declared in the project config. Paths use PathPattern globs
/// anchored at the config file's directory.
struct ConfigWaiver {
    std::string ruleId;
    std::optional<PathPattern> path;
    size_t line = 0; // 0 = any line
    std::string reason;
};

class ConfigManager {
public:
    static ConfigManager& instance();

    /// Load a project config: Python (.py, the primary format) or JSON.
    /// On failure returns false and getLastError() explains why.
    bool loadFromFile(const std::string& filePath);

    /// Search the start directory and its parents for uvmcheck_config.py,
    /// .uvmcheck.py, .uvmcheck.json or uvmcheck.json (first hit wins).
    /// Returns false if nothing was found or the found file failed to load;
    /// distinguish the two with getLastError().
    bool autoDiscover(const std::string& startDir = ".");

    /// Check if an external configuration was loaded
    bool hasLoadedConfig() const { return configLoaded; }
    const std::string& getLoadedConfigPath() const { return loadedConfigPath; }
    const std::string& getLastError() const { return lastError; }
    const std::vector<std::string>& getWarnings() const { return warnings; }

    /// Access engines
    style::NamingEngine& getNamingEngine() { return namingEngine; }
    const style::NamingEngine& getNamingEngine() const { return namingEngine; }

    style::StyleEngine& getStyleEngine() { return styleEngine; }
    const style::StyleEngine& getStyleEngine() const { return styleEngine; }

    /// Library/exclude paths and per-rule settings.
    ProjectConfig& getProjectConfig() { return projectConfig; }
    const ProjectConfig& getProjectConfig() const { return projectConfig; }

    const std::vector<ConfigWaiver>& getWaivers() const { return waivers; }

    /// Warns about rule keys that match no registered rule, and maps rule
    /// names onto IDs. Call after the config is loaded and rules registered.
    void validateRuleKeys(const std::unordered_map<std::string, std::string>& nameToId,
                          const std::vector<std::string>& knownIds);

    /// Emit a starter Python configuration file
    void dumpDefaultConfig(std::ostream& os) const;

private:
    ConfigManager();

    style::NamingEngine namingEngine;
    style::StyleEngine styleEngine;
    ProjectConfig projectConfig;
    std::vector<ConfigWaiver> waivers;
    bool configLoaded = false;
    std::string loadedConfigPath;
    std::string lastError;
    std::vector<std::string> warnings;

    bool applyConfig(const json::Value& root, const std::filesystem::path& baseDir);
    bool runPythonConfig(const std::string& filePath, std::string& jsonOut);
    void applyStyle(const json::Value& style);
    void applyNaming(const json::Value& naming);
    bool applyRules(const json::Value& rules, const std::filesystem::path& baseDir);
    bool readPathList(const json::Value& v, std::string_view key, const std::filesystem::path& baseDir,
                      PathSet& out);
    void warn(std::string msg) { warnings.push_back(std::move(msg)); }
    bool error(std::string msg) {
        lastError = std::move(msg);
        return false;
    }
};

} // namespace uvmcheck::core
