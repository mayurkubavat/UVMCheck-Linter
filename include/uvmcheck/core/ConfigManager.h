#pragma once

#include <string>
#include <string_view>
#include <memory>
#include <iostream>
#include "uvmcheck/style/NamingEngine.h"
#include "uvmcheck/style/StyleEngine.h"

namespace uvmcheck::core {

class ConfigManager {
public:
    static ConfigManager& instance();

    /// Load naming and style configurations from a JSON file
    bool loadFromFile(const std::string& filePath);

    /// Attempt auto-discovery of .uvmcheck.json or uvmcheck.json starting in current directory
    bool autoDiscover(const std::string& startDir = ".");

    /// Check if an external configuration was loaded
    bool hasLoadedConfig() const { return configLoaded; }
    const std::string& getLoadedConfigPath() const { return loadedConfigPath; }

    /// Access engines
    style::NamingEngine& getNamingEngine() { return namingEngine; }
    const style::NamingEngine& getNamingEngine() const { return namingEngine; }

    style::StyleEngine& getStyleEngine() { return styleEngine; }
    const style::StyleEngine& getStyleEngine() const { return styleEngine; }

    /// Emit a starter/default JSON configuration file
    void dumpDefaultConfig(std::ostream& os) const;

private:
    ConfigManager();

    style::NamingEngine namingEngine;
    style::StyleEngine styleEngine;
    bool configLoaded = false;
    std::string loadedConfigPath;

    bool parseJsonConfig(const std::string& jsonContent);
};

} // namespace uvmcheck::core
